use base64::{prelude::BASE64_STANDARD, Engine};
use futures::{stream, StreamExt};
use rand::distr::{Alphanumeric, SampleString};
use redis::{aio::MultiplexedConnection, AsyncCommands};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{query, query_as, Pool, Postgres};
use tracing::info;
use uuid::Uuid;

use crate::api::{auth::UuidPlot, PlotId};

pub mod baton;
pub mod instance;

#[derive(Clone)]
pub struct Store {
    redis: MultiplexedConnection,
    pg: Pool<Postgres>,
}

pub struct KeyRow {
    plot: PlotId,
    owner_uuid: Uuid,
}

/// Misc
impl Store {
    pub async fn verify_key(&self, key: &str) -> color_eyre::Result<Option<UuidPlot>> {
        let mut redis = self.redis.clone();
        let res: Option<UuidPlot> = redis.get(format!("key:{key}")).await?;
        if let Some(plot) = res {
            return Ok(if plot.plot_id == -1 {
                None
            } else {
                Some(plot)
            });
        }

        let plot = query_as!(
            KeyRow,
            "
            SELECT
                ak.plot,
                p.owner_uuid
            FROM
                api_key ak
            JOIN
                plot p ON ak.plot = p.id
            WHERE
                ak.hashed_key = sha256($1) AND
                ak.disabled = false;
            ",
            key.as_bytes()
        )
        .fetch_optional(&self.pg)
        .await?;

        let key = BASE64_STANDARD.encode(Sha256::digest(key));
        if let Some(plot) = plot {
            let uuid_plot = UuidPlot {
                plot_id: plot.plot,
                owner: plot.owner_uuid,
            };
            let _: () = redis.set(format!("key:{}", key), &uuid_plot).await?;
            Ok(Some(uuid_plot))
        } else {
            let _: () = redis
                .set(
                    format!("key:{}", key),
                    UuidPlot {
                        plot_id: -1,
                        owner: Uuid::from_u128(0)
                    },
                )
                .await?;
            Ok(None)
        }
    }
    pub async fn create_key(&self, plot_id: PlotId) -> color_eyre::Result<String> {
        let key = Alphanumeric.sample_string(&mut rand::rng(), 32);
        query!(
            "INSERT INTO api_key (plot, hashed_key) VALUES ($1, sha256($2))",
            plot_id,
            key.as_bytes()
        )
        .execute(&self.pg)
        .await?;
        Ok(key)
    }
    pub async fn disable_all_keys(&self, plot_id: PlotId) -> color_eyre::Result<()> {
        let deleted = query!(
            "WITH disabled_keys AS (
                UPDATE api_key SET
                    disabled = true
                WHERE 
                    plot = $1 
                    AND disabled = false
                RETURNING hashed_key
            ) SELECT hashed_key FROM disabled_keys;",
            plot_id
        )
        .fetch_all(&self.pg)
        .await?;
        for row in deleted {
            let key = BASE64_STANDARD.encode(row.hashed_key);
            info!("{key}");
            let _: () = self.redis.clone().del(format!("key:{key}")).await?;
        }

        Ok(())
    }
    pub async fn get_uuid(&self, name: &str) -> color_eyre::Result<Option<Uuid>> {
        let found: Option<String> = self
            .redis
            .clone()
            .get(format!("player:{}:uuid", name))
            .await?;

        Ok(if let Some(uuid) = found {
            Some(uuid.parse()?)
        } else {
            let call = format!("https://api.mojang.com/users/profiles/minecraft/{}", name);

            let uuid_fetch = reqwest::get(call).await?;
            let text = uuid_fetch.text().await?;

            let json: MojangResponse = serde_json::from_str(&text)?;

            let _: () = self
                .redis
                .clone()
                .set(format!("player:{}:uuid", name), json.id.to_string())
                .await?;
            Some(json.id)
        })
    }
}

#[derive(Deserialize)]
struct MojangResponse {
    id: Uuid,
}
