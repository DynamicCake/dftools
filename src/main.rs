use api::baton::BatonApi;
use poem::{listener::TcpListener, Route};
use poem_openapi::OpenApiService;
use serde::Deserialize;

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

    let api_service =
        OpenApiService::new(BatonApi, "Test API", "1.0").server("http://localhost:3000/api");
    let ui = api_service.swagger_ui();
    let app = Route::new().nest("/baton/v1", api_service).nest("/", ui);

    let _ = poem::Server::new(TcpListener::bind(format!("0.0.0.0:{}", config.port)))
        .run(app)
        .await;
    Ok(())
}

#[derive(Deserialize, Debug)]
pub struct Config {
    redis_url: String,
    port: u16,
}
