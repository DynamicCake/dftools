use redis::{aio::MultiplexedConnection, AsyncCommands};
use redis_macros::{FromRedisValue, ToRedisArgs};
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as, Pool, Postgres};
use uuid::Uuid;

use crate::{api::PlotId, instance::Instance};

use super::Store;

impl Store {
    pub fn new(redis: MultiplexedConnection, pg: Pool<Postgres>) -> Self {
        Self { redis, pg }
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

    pub async fn get_plot_instance(&self, plot_id: PlotId) -> color_eyre::Result<Option<PlotValue>> {
        let mut redis = self.redis.clone();
        let found: Option<PlotValue> = redis.get(format!("plot:{}", plot_id)).await?;

        if let Some(val) = found {
            Ok(Some(val))
        } else {
            Ok(self.cache_plot(plot_id).await?)
        }
    }

    async fn cache_plot(&self, plot_id: PlotId) -> color_eyre::Result<Option<PlotValue>> {
        let row = query_as!(
            PlotRow,
            "SELECT id, owner_uuid, instance FROM plot
                WHERE id = $1;",
            plot_id
        )
        .fetch_optional(&self.pg)
        .await?;
        Ok(if let Some(row) = row {
            let instance: Instance = row.instance.try_into()?;
            let value = PlotValue {
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
        instance: &Instance,
    ) -> color_eyre::Result<Result<(), RegisterError>> {
        if !instance.vibe_check().await {
            return Ok(Err(RegisterError::DomainCheckFailed));
        }

        self.invalidate_plot_cache(plot_id).await?;
        let str: Option<&String> = instance.into();
        match query!(
            "INSERT INTO plot (id, owner_uuid, instance) VALUES ($1, $2, $3)",
            plot_id,
            uuid,
            str
        )
        .execute(&self.pg)
        .await
        {
            Ok(_) => Ok(Ok(())),
            Err(kind) => match kind {
                sqlx::Error::Database(err) => match err.kind() {
                    sqlx::error::ErrorKind::UniqueViolation => Ok(Err(RegisterError::PlotTaken)),
                    _ => Err(err.into()),
                },
                err => Err(err.into()),
            },
        }
    }
    /// If result is Ok(true) it means success,
    /// Ok(false) means the instance didn't pass the vibe check
    pub async fn edit_plot(
        &self,
        plot_id: PlotId,
        instance_domain: &Instance,
    ) -> color_eyre::Result<Result<(), PlotEditError>> {
        if !instance_domain.vibe_check().await {
            return Ok(Err(PlotEditError::InvalidDomain));
        }

        self.invalidate_plot_cache(plot_id).await?;
        let domain: Option<&String> = instance_domain.into();
        let res = query!(
            "UPDATE plot SET
            instance = $2
            WHERE id = $1",
            plot_id,
            domain
        )
        .execute(&self.pg)
        .await
        .expect("db shouldn't fail")
        .rows_affected();
        if res == 1 {
            Ok(Ok(()))
        } else {
            Ok(Err(PlotEditError::PlotNotFound))
        }
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
    #[error("Domain check failed")]
    DomainCheckFailed,
    #[error("Plot is already registered")]
    PlotTaken,
}

#[derive(Debug, thiserror::Error)]
pub enum PlotEditError {
    #[error("Invalid domain")]
    InvalidDomain,
    #[error("Plot not found")]
    PlotNotFound,
}

#[derive(Serialize, Deserialize, FromRedisValue, ToRedisArgs, Clone)]
pub struct PlotValue {
    pub owner: Uuid,
    pub instance: Instance,
}

#[allow(dead_code)]
pub struct PlotRow {
    id: i32,
    owner_uuid: Uuid,
    instance: Option<String>,
}
