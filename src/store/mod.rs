use redis::{aio::MultiplexedConnection, AsyncCommands};
use serde::Deserialize;
use sqlx::{query_as, Pool, Postgres};
use uuid::Uuid;

use crate::api::PlotId;
pub mod instance;

#[derive(Clone)]
pub struct Store {
    redis: MultiplexedConnection,
    pg: Pool<Postgres>,
}


/// Baton
impl Store {
    pub async fn fetch_plot_trust(&self, plot: PlotId) -> color_eyre::Result<Vec<PlotId>> {
        let mut redis = self.redis.clone();
        let attempt: Option<Vec<PlotId>> = redis.get(format!("plot:{}:baton_trust", plot)).await?;
        Ok(if let Some(trusts) = attempt {
            trusts
        } else {
            struct TrustRow {
                trusted: PlotId,
            }
            let trusts: Vec<PlotId> = query_as!(
                TrustRow,
                "SELECT trusted FROM baton_trust WHERE plot = $1;",
                plot
            )
            .fetch_all(&self.pg)
            .await?
            .into_iter()
            .map(|it| it.trusted)
            .collect();

            let _: () = redis
                .set(format!("plot:{}:baton_trust", plot), &trusts)
                .await?;
            trusts
        })
    }
}

/// Misc
impl Store {
    pub async fn get_uuid(&self, name: String) -> color_eyre::Result<Option<Uuid>> {
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
