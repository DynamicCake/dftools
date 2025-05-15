
use api::{baton::BatonApi, instance::InstanceApi};
use ascii_domain::{char_set::ASCII_HYPHEN_DIGITS_LOWERCASE, dom::Domain};
use poem::{listener::TcpListener, EndpointExt, Route};
use poem_openapi::OpenApiService;
use rand::distr::{Alphanumeric, SampleString};
use serde::Deserialize;
use sqlx::PgPool;
use store::Store;

pub mod api;
pub mod instance;
pub mod store;

const DOMAIN_SET: ascii_domain::char_set::AllowedAscii<[u8; 37]> = ASCII_HYPHEN_DIGITS_LOWERCASE;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install().unwrap();
    // Initialize config
    dotenvy::dotenv()?;
    let config = match envy::from_env::<Config>() {
        Ok(it) => it,
        Err(err) => panic!("{:?} (envs are case insensitive)", err),
    };
    tracing_subscriber::fmt::init();

    let pg = PgPool::connect(&config.database_url).await?;
    let client = redis::Client::open(config.redis_url).unwrap();
    let redis = client.get_multiplexed_async_connection().await?;
    let store = Store::new(redis, pg);

    let instance_api_service = OpenApiService::new(
        InstanceApi {
            store: store.clone(),
            instance_domain: Domain::try_from_bytes(config.domain, &DOMAIN_SET)
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


    let _ = poem::Server::new(TcpListener::bind(format!("0.0.0.0:{}", config.port)))
        .run(app)
        .await;
    println!("simply no");
    Ok(())
}

fn random_key() -> String {
    Alphanumeric.sample_string(&mut rand::rng(), 42)
}

#[derive(Deserialize, Debug)]
pub struct Config {
    redis_url: String,
    database_url: String,
    port: u16,
    domain: String,
}
