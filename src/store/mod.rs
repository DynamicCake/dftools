use base64::Engine;
use chrono::Local;
use color_eyre::eyre::Context;
use ed25519_dalek::{ed25519::signature::SignerMut, Signature, SigningKey, VerifyingKey};
use hmac::Hmac;
use jwt::{FromBase64, SignWithKey, VerifyWithKey};
use rand::distr::{Alphanumeric, SampleString};
use redis::{aio::MultiplexedConnection, AsyncCommands};
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{prelude::FromRow, query, query_as, Pool, Postgres};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::{
    api::{
        auth::{ExternalServer, Plot},
        instance::VerificationResponse,
        PlotId,
    },
    instance::{ExternalDomain, Instance, InstanceDomain},
    BASE64,
};

pub mod baton;
pub mod instance;

pub struct Store {
    redis: MultiplexedConnection,
    pg: Pool<Postgres>,
    client: Client,
    jwt_key: Hmac<Sha256>,
    secret_key: RwLock<SigningKey>,
    public_key: VerifyingKey,
}

/// Misc
impl Store {
    pub async fn verify_key(&self, key: &str) -> color_eyre::Result<Option<Plot>> {
        let mut redis = self.redis.clone();
        let res: Option<Plot> = redis.get(format!("key:{key}")).await?;
        if let Some(plot) = res {
            return Ok(if plot.plot_id == -1 { None } else { Some(plot) });
        }

        #[derive(FromRow)]
        struct Row {
            plot: PlotId,
            owner_uuid: Uuid,
            domain: Option<String>,
            public_key: Option<Vec<u8>>,
        }

        let plot = query_as!(
            Row,
            "
            SELECT
                key.plot,
                p.owner_uuid,
                instance.domain,
                instance.public_key
            FROM api_key key
            JOIN plot p ON key.plot = p.id
            LEFT JOIN known_instance instance ON instance.id = p.instance
            WHERE
                key.hashed_key = sha256($1) AND
                key.disabled = false;
            ",
            key.as_bytes()
        )
        .fetch_optional(&self.pg)
        .await?;

        let key = BASE64.encode(Sha256::digest(key));
        if let Some(plot) = plot {
            let plot = if let Some(key) = plot.public_key {
                let instance = Instance::from_row(key, plot.domain)?;
                Plot {
                    plot_id: plot.plot,
                    owner: plot.owner_uuid,
                    instance,
                }
            } else {
                Plot {
                    plot_id: plot.plot,
                    owner: plot.owner_uuid,
                    instance: self.construct_current_instance(),
                }
            };
            let _: () = redis.set(format!("key:{}", key), &plot).await?;
            Ok(Some(plot))
        } else {
            let _: () = redis
                .set(
                    format!("key:{}", key),
                    // Yes... magic values due to redis
                    Plot {
                        plot_id: -1,
                        owner: Uuid::from_u128(0),
                        instance: Instance::new(self.public_key, InstanceDomain::Current),
                    },
                )
                .await?;
            Ok(None)
        }
    }
    pub fn construct_current_instance(&self) -> Instance {
        Instance {
            key: self.public_key,
            domain: InstanceDomain::Current,
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
            let key = BASE64.encode(row.hashed_key);
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
    pub async fn sign(&self, msg: &[u8]) -> Signature {
        self.secret_key.write().await.sign(msg)
    }
    pub async fn ping_instance(
        &self,
        instance: &ExternalDomain,
    ) -> color_eyre::Result<VerifyingKey> {
        let domain = instance.inner().as_inner();

        let verify_body = Local::now()
            .format("DFTOOLS VERIFY %Y-%m-%d %H:%M:%S%.3f")
            .to_string();

        #[cfg(debug_assertions)]
        let url = format!("http://{}/instance/v0/sign", domain);
        #[cfg(not(debug_assertions))]
        let url = format!("https://{}/instance/v0/sign", domain);
        info!("{}", url);
        let req = self
            .client
            .get(url)
            .query(&[("tosign", &verify_body)])
            .send()
            .await?;
        let body = req.text().await?;
        let json: VerificationResponse =
            serde_json::from_str(&body).wrap_err("Probably due to not being a dftools server")?;
        let key = VerifyingKey::from_bytes(
            BASE64
                .decode(json.server_key)
                .wrap_err("Server key")?
                .as_slice()
                .try_into()
                .wrap_err("Expected 32 bytes")?,
        )
        .wrap_err("Interpreting server key")?;
        let sig = Signature::from_bytes(
            BASE64
                .decode(json.signature)
                .wrap_err("Signature")?
                .as_slice()
                .try_into()
                .wrap_err("Expected 64 bytes for sig")?,
        );
        let _: () = key
            .verify_strict(verify_body.as_bytes(), &sig)
            .wrap_err("Invalid signature")?;
        Ok(key)
    }

    pub fn public_key(&self) -> VerifyingKey {
        self.public_key
    }
}

#[derive(Deserialize)]
struct MojangResponse {
    id: Uuid,
}
