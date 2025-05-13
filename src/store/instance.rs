use ascii_domain::{char_set::ASCII_LOWERCASE, dom::Domain};
use redis::{aio::MultiplexedConnection, AsyncCommands};
use redis_macros::{FromRedisValue, ToRedisArgs};
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as, Pool, Postgres};
use uuid::Uuid;

use crate::{api::PlotId, DOMAIN_SET};

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

    pub async fn find_plot(&self, plot_id: PlotId) -> color_eyre::Result<Option<PlotValue>> {
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
            let instance = {
                match row.instance {
                    Some(inst) => Some(Domain::try_from_bytes(inst, &DOMAIN_SET)?),
                    None => None,
                }
            };
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
    pub async fn register_plot<T: AsRef<str>>(
        &self,
        plot_id: PlotId,
        uuid: Uuid,
        name: Option<&Domain<T>>,
    ) -> color_eyre::Result<Result<(), RegisterError>> {
        if let Some(name) = name {
            let vibes = self.domain_vibe_check(name.into()).await;
            if !vibes {
                return Ok(Err(RegisterError::InvalidDomain));
            }
        }

        self.invalidate_plot_cache(plot_id).await?;
        let str = name.map(|domain| domain.as_inner().as_ref().to_owned());
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
                    sqlx::error::ErrorKind::UniqueViolation => {
                        Ok(Err(RegisterError::PlotTaken))
                    }
                    _ => Err(err.into()),
                },
                err => Err(err.into()),
            },
        }
    }
    /// If result is Ok(true) it means success,
    /// Ok(false) means the instance didn't pass the vibe check
    pub async fn edit_plot<T: AsRef<str>>(
        &self,
        plot_id: PlotId,
        instance_domain: Option<&Domain<T>>,
    ) -> color_eyre::Result<Result<(), PlotEditError>> {
        if let Some(name) = instance_domain {
            let vibes = self.domain_vibe_check(name.into()).await;
            if !vibes {
                return Ok(Err(PlotEditError::InvalidDomain));
            }
        }

        self.invalidate_plot_cache(plot_id).await?;
        let domain = instance_domain.map(|domain| domain.as_inner().as_ref().to_owned());
        let res = query!(
            "UPDATE plot SET
            instance = $2
            WHERE id = $1
            RETURNING id;",
            plot_id,
            domain
        )
        .fetch_optional(&self.pg)
        .await
        .expect("db shouldn't fail");
        if res.is_some() {
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
    async fn domain_vibe_check<T: AsRef<str>>(&self, _domain: Domain<T>) -> bool {
        // TODO: Implmeent domain vibe check
        true
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterError {
    #[error("Invalid domain")]
    InvalidDomain,
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
    pub instance: Option<Domain<String>>,
}

#[allow(dead_code)]
pub struct PlotRow {
    id: i32,
    owner_uuid: Uuid,
    instance: Option<String>,
}
