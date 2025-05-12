use api::{baton::BatonApi, instance::InstanceApi};
use ascii_domain::{char_set::ASCII_LOWERCASE, dom::Domain};
use poem::{listener::TcpListener, Route};
use poem_openapi::OpenApiService;
use rand::distr::{Alphanumeric, SampleString};
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

    let instance_api_service = OpenApiService::new(
        InstanceApi {
            pg: pg.clone(),
            redis: redis.clone(),
            instance_domain: Domain::try_from_bytes(config.domain, &ASCII_LOWERCASE),
            self_check_key: random_key(),
        },
        "Instance API",
        "0.0.1",
    )
    .server("http://localhost:3000/instance/v0");
    let baton_api_service = OpenApiService::new(BatonApi { pg, redis }, "Baton API", "0.0.1");

    let app = Route::new()
        .nest("/instance/v0/docs", instance_api_service.swagger_ui())
        .nest("/instance/v0", instance_api_service)
        .nest("/baton/v0/docs", baton_api_service.swagger_ui())
        .nest("/baton/v0", baton_api_service);

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
