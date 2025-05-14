use redis::{aio::MultiplexedConnection, AsyncCommands};
use serde::Deserialize;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

pub mod baton;
pub mod instance;

#[derive(Clone)]
pub struct Store {
    redis: MultiplexedConnection,
    pg: Pool<Postgres>,
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
