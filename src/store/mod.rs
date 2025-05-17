use base64::{prelude::BASE64_STANDARD, Engine};
use ed25519_dalek::{ed25519::signature::Keypair, SigningKey, VerifyingKey};
use hmac::Hmac;
use jwt::{FromBase64, SignWithKey, VerifyWithKey};
use rand::distr::{Alphanumeric, SampleString};
use redis::{aio::MultiplexedConnection, AsyncCommands};
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{query, query_as, Pool, Postgres};
use tracing::info;
use uuid::Uuid;

use crate::{
    api::{auth::{ExternalServer, Plot}, PlotId},
    instance::{Instance, InstanceDomain},
};

pub mod baton;
pub mod external;
pub mod instance;

#[derive(Clone)]
pub struct Store {
    redis: MultiplexedConnection,
    pg: Pool<Postgres>,
    client: Client,
    jwt_key: Hmac<Sha256>,
    secret_key: SigningKey,
}

pub struct KeyRow {
    plot: PlotId,
    owner_uuid: Uuid,
    instance: Option<String>,
}

/// Misc
impl Store {
    pub async fn verify_key(&self, key: &str) -> color_eyre::Result<Option<Plot>> {
        let mut redis = self.redis.clone();
        let res: Option<Plot> = redis.get(format!("key:{key}")).await?;
        if let Some(plot) = res {
            return Ok(if plot.plot_id == -1 { None } else { Some(plot) });
        }

        let plot = query!(
            "
            SELECT
                key.plot,
                p.owner_uuid,
                instance.domain,
                instance.public_key
            FROM api_key key
            JOIN plot p ON key.plot = p.id
            JOIN known_instance instance ON instance.id = p.id
            WHERE
                key.hashed_key = sha256($1) AND
                key.disabled = false;
            ",
            key.as_bytes()
        )
        .fetch_optional(&self.pg)
        .await?;

        let key = BASE64_STANDARD.encode(Sha256::digest(key));
        if let Some(plot) = plot {
            let instance = Instance::from_row(plot.public_key, plot.domain)?;
            let uuid_plot = Plot {
                plot_id: plot.plot,
                owner: plot.owner_uuid,
                instance,
            };
            let _: () = redis.set(format!("key:{}", key), &uuid_plot).await?;
            Ok(Some(uuid_plot))
        } else {
            let _: () = redis
                .set(
                    format!("key:{}", key),
                    // Yes... magic values due to redis
                    Plot {
                        plot_id: -1,
                        owner: Uuid::from_u128(0),
                        instance: Instance::new(
                            VerifyingKey::from_bytes(b"--------------------------------")
                                .expect("dummy value"),
                            InstanceDomain::try_from(None).expect("dummy value"),
                        ),
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
    pub fn verify_jwt<T: FromBase64>(&self, jwt: &str) -> Option<T> {
        VerifyWithKey::<T>::verify_with_key(jwt, &self.jwt_key).ok()
    }
    pub fn sign_jwt(&self, jwt: &ExternalServer) -> Result<String, jwt::Error> {
        jwt.sign_with_key(&self.jwt_key)
    }
    pub async fn ping_instance(&self, instance: &InstanceDomain) -> color_eyre::Result<VerifyingKey> {
        let domain: Option<&str> = instance.into();


        self.client
            .get(format!("https://{}/instance/v0/sign", domain));
        return Ok(true)
    }
}

#[derive(Deserialize)]
struct MojangResponse {
    id: Uuid,
}
