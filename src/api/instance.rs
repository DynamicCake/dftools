use std::hash::Hash;

use hmac::digest::generic_array::GenericArray;
use poem_openapi::{
    param::Query,
    payload::{Json, PlainText},
    ApiResponse, Object, OpenApi,
};
use redis::{aio::MultiplexedConnection, AsyncCommands};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{query, query_as, Pool, Postgres};
use tracing::error;
use uuid::Uuid;

use super::{PlotAuth, PlotId};

pub struct InstanceApi {
    pub pg: Pool<Postgres>,
    pub redis: MultiplexedConnection,
    pub instance_domain: String,
    pub self_check_key: String,
}

impl InstanceApi {
    fn get_vibecheck_hash(&self) -> [u8; 32] {
        let hashed = Sha256::digest(&self.self_check_key);
        hashed.into()
    }
    async fn get_uuid(&self, name: String) -> Option<Uuid> {
        let found: Option<String> = self
            .redis
            .clone()
            .get(format!("player:{}:uuid", name))
            .await
            .map_err(|err| error!("Insert failed {}", err))
            .ok()?;

        if let Some(uuid) = found {
            Some(
                uuid.parse()
                    .map_err(|err| error!("Malfored uuid in redis {}", err))
                    .ok()?,
            )
        } else {
            let call = format!("https://api.mojang.com/users/profiles/minecraft/{}", name);

            let uuid_fetch = if let Ok(it) = reqwest::get(call).await {
                it
            } else {
                error!("Cannot fetch uuid for {}", name);
                return None;
            };
            let text = if let Ok(it) = uuid_fetch.text().await {
                it
            } else {
                error!("Cannot fetch uuid for {}", name);
                return None;
            };

            let json: MojangResponse = if let Ok(it) = serde_json::from_str(&text) {
                error!("Cannot fetch uuid for {}", name);
                it
            } else {
                return None;
            };

            let _: () = self
                .redis
                .clone()
                .set(format!("player:{}:uuid", name), json.id.to_string())
                .await
                .map_err(|err| error!("Insert failed {}", err))
                .ok()?;
            Some(json.id)
        }
    }
    async fn domain_vibe_check(&self, _domain: &str) -> bool {
        // TODO: Implmeent domain vibe check
        true
    }
}

#[derive(ApiResponse)]
pub enum VibeCheckResult {
    #[oai(status = 204)]
    Passed,
    #[oai(status = 200)]
    KeyPassed(PlainText<&'static str>),
    #[oai(status = 400)]
    KeyVibecheckFailed,
}

#[OpenApi]
impl InstanceApi {
    #[oai(path = "/vibecheck", method = "get")]
    async fn vibecheck(&self, key: Query<Option<String>>) -> VibeCheckResult {
        if let Some(key) = key.0 {
            let hash: [u8; 32] = Sha256::digest(key).into();
            let actual = self.get_vibecheck_hash();
            if hash == actual {
                return VibeCheckResult::KeyPassed(PlainText("You are you"));
            } else {
                return VibeCheckResult::KeyVibecheckFailed;
            }
        }
        VibeCheckResult::Passed
    }

    #[oai(path = "/plot", method = "get")]
    async fn get_plot(&self, id: Query<PlotId>) -> PlotFetchResult {
        let id = id.0 as i32;
        let row = query_as!(
            PlotRow,
            "SELECT id, owner_uuid, instance FROM plot
            WHERE id = $1;",
            id
        )
        .fetch_optional(&self.pg)
        .await
        .expect("db shouldn't fail");
        let row = if let Some(it) = row {
            it
        } else {
            return PlotFetchResult::NotFound;
        };

        PlotFetchResult::Ok(Json(Plot {
            plot: row.id as PlotId,
            owner: row.owner_uuid.to_string(),
            instance: row.instance.unwrap_or(self.instance_domain.clone()),
        }))
    }

    #[oai(path = "/plot", method = "post")]
    async fn register(&self, instance: Json<Instance>, auth: PlotAuth) -> RegisterResult {
        let id = auth.0.plot_id;
        if let Some(name) = &instance.name {
            if !self.domain_vibe_check(name).await {
                return RegisterResult::InvalidDomain;
            }
        }
        let uuid = self
            .get_uuid(auth.0.owner)
            .await
            .expect("mojang should cooperate");
        match query!(
            "INSERT INTO plot (id, owner_uuid, instance) VALUES ($1, $2, $3)",
            id,
            uuid,
            instance.name
        )
        .execute(&self.pg)
        .await
        {
            Ok(_) => (),
            Err(err) => match err {
                sqlx::Error::Database(err) => match err.kind() {
                    sqlx::error::ErrorKind::UniqueViolation => {
                        return RegisterResult::UserAlreadyExists
                    }
                    err => panic!("{:?}", err),
                },
                err => panic!("{:?}", err),
            },
        }

        RegisterResult::Success
    }
}

#[derive(ApiResponse)]
enum RegisterResult {
    /// Try again until mojang servers cooperate
    #[oai(status = 500)]
    CannotFetchUuid,
    #[oai(status = 400)]
    InvalidDomain,
    #[oai(status = 409)]
    UserAlreadyExists,
    #[oai(status = 200)]
    Success,
}

#[derive(Deserialize)]
struct MojangResponse {
    id: Uuid,
}

#[derive(Object)]
struct Instance {
    name: Option<String>,
}

#[derive(ApiResponse)]
enum PlotFetchResult {
    #[oai(status = 404)]
    NotFound,
    #[oai(status = 200)]
    Ok(Json<Plot>),
}

#[derive(Object)]
struct Plot {
    plot: PlotId,
    owner: String,
    instance: String,
}

pub struct PlotRow {
    id: i32,
    owner_uuid: Uuid,
    instance: Option<String>,
}
