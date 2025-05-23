use std::{fs::read_to_string, sync::Arc};

use api::{baton::BatonApi, instance::InstanceApi};
use base64::{engine::GeneralPurpose, prelude::BASE64_URL_SAFE, Engine};
use color_eyre::eyre::Context;
use dfjson::DfJson;
use ed25519_dalek::SigningKey;
use hmac::{Hmac, HmacCore};
use instance::ExternalDomain;
use poem::{listener::TcpListener, EndpointExt, Route};
use poem_openapi::OpenApiService;
use reqwest::Client;
use schemars::schema_for;
use serde::Deserialize;
use sha2::{
    digest::{core_api::CoreWrapper, KeyInit},
    Sha256,
};
use sqlx::PgPool;
use store::Store;
use tracing::{error, warn};

pub mod api;
pub mod dfjson;
pub mod instance;
pub mod store;

const BASE64: GeneralPurpose = BASE64_URL_SAFE;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install().unwrap();
    tracing_subscriber::fmt::init();

    const PATH: &str = ".env";
    // Initialize config
    let _ = dotenvy::from_path(PATH);
    let config = match envy::from_env::<Config>() {
        Ok(it) => it,
        Err(err) => panic!("{:?} (envs are case insensitive)", err),
    };
    let jwt_key: Hmac<Sha256> = if let Some(key) = config.jwt_key {
        let key = BASE64.decode(key).wrap_err("jwt key")?;
        <CoreWrapper<HmacCore<_>> as KeyInit>::new_from_slice(key.as_slice())?
    } else {
        error!("JWT_KEY is not found, generate one with dftools_secret.sh gen-jwt");
        return Ok(());
    };
    let signing_key = if let Some(key) = config.secret_key {
        if let Ok(file) = read_to_string(PATH) {
            if file.contains(&key) {
                warn!("Secret key found in .env file. Generally it is a bad idea to store this in a plaintext file");
            }
        }
        let key = BASE64.decode(key).wrap_err("jwt key")?;
        SigningKey::from_bytes(key.as_slice().try_into().wrap_err("signed key")?)
    } else {
        error!("SECRET_KEY is not found, generate one with dftools_secret.sh gen-key");
        return Ok(());
    };

    let pg = PgPool::connect(&config.database_url).await?;
    let client = redis::Client::open(config.redis_url).unwrap();
    let redis = client.get_multiplexed_async_connection().await?;
    let store = Arc::new(Store::new(redis, pg, Client::new(), jwt_key, signing_key));

    let instance_api_service = OpenApiService::new(
        InstanceApi {
            store: store.clone(),
            domain: ExternalDomain::try_from(config.domain)
                .expect("Malformed domain in config")
                .into_inner(),
        },
        "Instance API",
        "0.0.1",
    )
    .server(format!("http://localhost:{}/instance/v0", config.port));
    let baton_api_service = OpenApiService::new(
        BatonApi {
            store: store.clone(),
        },
        "Baton API",
        "0.0.1",
    )
    .server(format!("http://localhost:{}/baton/v0", config.port));

    let app = Route::new();
    // This is an open source project and protocol, it is fine to expose the swagger ui
    // #[cfg(debug_assertions)]
    let app = app
        .nest("/instance/v0/docs", instance_api_service.swagger_ui())
        .nest("/baton/v0/docs", baton_api_service.swagger_ui());
    let app = app
        .nest("/instance/v0", instance_api_service)
        .nest("/baton/v0", baton_api_service)
        .data(store);

    poem::Server::new(TcpListener::bind(format!("0.0.0.0:{}", config.port)))
        .run(app)
        .await?;
    Ok(())
}

#[derive(Deserialize, Debug)]
struct Config {
    redis_url: String,
    database_url: String,
    port: u16,
    domain: String,
    jwt_key: Option<String>,
    /// VERY SECRET KEY, IF THIS GETS COMPROMISED YOUR INSTANCE IS COOKED
    secret_key: Option<String>,
}

#[allow(dead_code)]
fn get_schema() -> String {
    serde_json::to_string_pretty(&schema_for!(DfJson)).unwrap()
}
