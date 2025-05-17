use std::ffi::OsString;

use api::{baton::BatonApi, instance::InstanceApi};
use ascii_domain::{char_set::ASCII_HYPHEN_DIGITS_LOWERCASE, dom::Domain};
use dfjson::DfJson;
use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use poem::{listener::TcpListener, EndpointExt, Route};
use poem_openapi::OpenApiService;
use rand::distr::{Alphanumeric, SampleString};
use reqwest::Client;
use schemars::schema_for;
use serde::Deserialize;
use sqlx::PgPool;
use store::Store;
use tracing::error;

pub mod api;
pub mod dfjson;
pub mod instance;
pub mod store;

const DOMAIN_SET: ascii_domain::char_set::AllowedAscii<[u8; 37]> = ASCII_HYPHEN_DIGITS_LOWERCASE;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install().unwrap();
    tracing_subscriber::fmt::init();
    // Initialize config
    dotenvy::dotenv()?;
    let config = match envy::from_env::<Config>() {
        Ok(it) => it,
        Err(err) => panic!("{:?} (envs are case insensitive)", err),
    };
    let jwt_key = if let Some(key) = config.dft_jwt_key {
        Hmac::new_from_slice(key.as_encoded_bytes()).expect("Invalid length")
    } else {
        error!("DFT_JWT_KEY is not found, generate one with dftools gen-key");
        return Ok(());
    };
    let signing_key = if let Some(key) = config.dft_secret_key {
        SigningKey::from_bytes(key.as_encoded_bytes().try_into()?)
    } else {
        error!("DFT_SECRET_KEY is not found, generate one with dftools gen-key");
        return Ok(());
    };

    let pg = PgPool::connect(&config.dft_database_url).await?;
    let client = redis::Client::open(config.dft_redis_url).unwrap();
    let redis = client.get_multiplexed_async_connection().await?;
    let store = Store::new(redis, pg, Client::new(), jwt_key, signing_key);

    let instance_api_service = OpenApiService::new(
        InstanceApi {
            store: store.clone(),
            domain: Domain::try_from_bytes(config.dft_domain, &DOMAIN_SET)
                .expect("Malformed domain in config"),
            self_check_key: random_key(),
        },
        "Instance API",
        "0.0.1",
    )
    .server("http://localhost:3000/instance/v0");
    let baton_api_service = OpenApiService::new(
        BatonApi {
            store: store.clone(),
        },
        "Baton API",
        "0.0.1",
    )
    .server("http://localhost:3000/baton/v0");

    let app = Route::new()
        .nest("/instance/v0/docs", instance_api_service.swagger_ui())
        .nest("/instance/v0", instance_api_service)
        .nest("/baton/v0/docs", baton_api_service.swagger_ui())
        .nest("/baton/v0", baton_api_service)
        .data(store);

    let _ = poem::Server::new(TcpListener::bind(format!("0.0.0.0:{}", config.dft_port)))
        .run(app)
        .await;
    Ok(())
}

fn random_key() -> String {
    Alphanumeric.sample_string(&mut rand::rng(), 42)
}

#[derive(Deserialize, Debug)]
struct Config {
    dft_redis_url: String,
    dft_database_url: String,
    dft_port: u16,
    dft_domain: String,
    dft_jwt_key: Option<OsString>,
    /// VERY SECRET KEY, IF THIS GETS COMPROMISED YOUR INSTANCE IS COOKED
    dft_secret_key: Option<OsString>,
}

#[allow(dead_code)]
fn get_schema() -> String {
    serde_json::to_string_pretty(&schema_for!(DfJson)).unwrap()
}
