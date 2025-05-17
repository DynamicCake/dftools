use ed25519_dalek::{SigningKey, VerifyingKey};
use hmac::Hmac;
use redis::{aio::MultiplexedConnection, AsyncCommands};
use redis_macros::{FromRedisValue, ToRedisArgs};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::{query, Pool, Postgres};
use uuid::Uuid;

use crate::{
    api::{auth::Plot, PlotId},
    instance::{Instance, InstanceDomain},
};

use super::Store;

impl Store {
    pub fn new(
        redis: MultiplexedConnection,
        pg: Pool<Postgres>,
        client: Client,
        jwt_key: Hmac<Sha256>,
        secret_key: SigningKey,
    ) -> Self {
        Self {
            redis,
            pg,
            client,
            jwt_key,
            secret_key,
        }
    }

    pub async fn plot_exists(&self, plot_id: PlotId) -> color_eyre::Result<bool> {
        let mut redis = self.redis.clone();
        let found: Option<()> = redis.get(format!("plot:{}", plot_id)).await?;
        if let Some(_val) = found {
            Ok(true)
        } else {
            let cache_res = self.cache_plot(plot_id).await?;
            Ok(cache_res.is_some())
        }
    }

    pub async fn get_plot(&self, plot_id: PlotId) -> color_eyre::Result<Option<Plot>> {
        let mut redis = self.redis.clone();
        let found: Option<Plot> = redis.get(format!("plot:{}", plot_id)).await?;

        if let Some(val) = found {
            Ok(Some(val))
        } else {
            Ok(self.cache_plot(plot_id).await?)
        }
    }

    async fn cache_plot(&self, plot_id: PlotId) -> color_eyre::Result<Option<Plot>> {
        let row = query!(
            "SELECT plot.id, owner_uuid, known_instance.public_key, known_instance.domain FROM plot
            INNER JOIN known_instance ON plot.instance = known_instance.id
            WHERE plot.id = $1;",
            plot_id
        )
        .fetch_optional(&self.pg)
        .await?;
        Ok(if let Some(row) = row {
            let instance: Instance = Instance::from_row(row.public_key, row.domain)?;
            let value = Plot {
                plot_id: row.id,
                owner: row.owner_uuid,
                instance,
            };

            let moved = value.clone();
            let mut redis = self.redis.clone();
            tokio::spawn(async move {
                let _: () = redis
                    .set(format!("plot:{}", plot_id), moved)
                    .await
                    .expect("Cache cannot be written to");
            });
            Some(value)
        } else {
            None
        })
    }

    /// You are supposed to unwrap the eyre result, which is almost always ok,
    /// and handle the inner Result
    pub async fn register_plot(
        &self,
        plot_id: PlotId,
        uuid: Uuid,
        instance_key: &VerifyingKey,
    ) -> color_eyre::Result<Result<(), RegisterError>> {
        self.invalidate_plot_cache(plot_id).await?;
        let key = instance_key.as_bytes();
        let mut ta = self.pg.begin().await?;
        let id = query!("SELECT id FROM known_instance WHERE public_key = $1", key)
            .fetch_optional(&mut *ta)
            .await?
            .ok_or(RegisterError::InstanceNotFound)?
            .id;

        match query!(
            "INSERT INTO plot (id, owner_uuid, instance) VALUES ($1, $2, $3)",
            plot_id,
            uuid,
            id
        )
        .execute(&mut *ta)
        .await
        {
            Ok(_) => (),
            Err(kind) => {
                return match kind {
                    sqlx::Error::Database(err) => match err.kind() {
                        sqlx::error::ErrorKind::UniqueViolation => {
                            Ok(Err(RegisterError::PlotTaken))
                        }
                        _ => Err(err.into()),
                    },
                    err => Err(err.into()),
                }
            }
        };
        ta.commit().await?;
        Ok(Ok(()))
    }
    /// If result is Ok(true) it means success,
    /// Ok(false) means the instance didn't pass the vibe check
    pub async fn edit_plot(
        &self,
        plot_id: PlotId,
        instance_key: &VerifyingKey,
    ) -> color_eyre::Result<Result<(), PlotEditError>> {
        self.invalidate_plot_cache(plot_id).await?;
        let key = instance_key.as_bytes();
        let mut ta = self.pg.begin().await?;
        let id = query!("SELECT id FROM known_instance WHERE public_key = $1", key)
            .fetch_optional(&mut *ta)
            .await?
            .ok_or(RegisterError::InstanceNotFound)?
            .id;

        let res = query!(
            "UPDATE plot SET
            instance = $2
            WHERE id = $1",
            plot_id,
            id
        )
        .execute(&self.pg)
        .await
        .expect("db shouldn't fail")
        .rows_affected();
        if res != 1 {
            return Ok(Err(PlotEditError::PlotNotFound));
        }
        ta.commit().await?;
        Ok(Ok(()))
    }
    /// Do not `tokio::task` this
    /// Invalidating caches should be a part of the update operation
    async fn invalidate_plot_cache(&self, plot_id: PlotId) -> color_eyre::Result<()> {
        let mut redis = self.redis.clone();
        let _: () = redis.del(format!("plot:{}", plot_id)).await?;
        let _: () = redis.del(format!("plot:{}:baton_trust", plot_id)).await?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterError {
    #[error("Instance not found, perhaps register it?")]
    InstanceNotFound,
    #[error("Plot is already registered")]
    PlotTaken,
}

#[derive(Debug, thiserror::Error)]
pub enum PlotEditError {
    #[error("Instance not found, perhaps register it?")]
    InstanceNotFound,
    #[error("Plot not found")]
    PlotNotFound,
}

#[derive(Serialize, Deserialize, FromRedisValue, ToRedisArgs, Clone)]
pub struct PlotValue {
    pub owner: Uuid,
    pub instance: InstanceDomain,
}

// #[allow(dead_code)]
// pub struct PlotRow {
//     id: i32,
//     owner_uuid: Uuid,
//     instance: Option<String>,
// }
