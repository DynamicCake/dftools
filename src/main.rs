use api::{baton::BatonApi, instance::InstanceApi};
use poem::{listener::TcpListener, Route};
use poem_openapi::OpenApiService;
use rand::{distr::{Alphanumeric, SampleString}, random};
use serde::Deserialize;
use sqlx::PgPool;

pub mod api;

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

    let api_service = OpenApiService::new(
        InstanceApi {
            pg,
            redis,
            instance_domain: config.domain,
            self_check_key: random_key(),
        },
        "Instance API",
        "1.0",
    )
    .server("http://localhost:3000/instance/v1");
    let ui = api_service.swagger_ui();
    let app = Route::new().nest("/instance/v1", api_service).nest("/", ui);

    let _ = poem::Server::new(TcpListener::bind(format!("0.0.0.0:{}", config.port)))
        .run(app)
        .await;
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
