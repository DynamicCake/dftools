use std::net::{Ipv4Addr, SocketAddr};

use poem::{listener::TcpListener, Request, Route};
use poem_openapi::{auth::ApiKey, payload::PlainText, OpenApi, OpenApiService, SecurityScheme};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

#[derive(Debug, Serialize, Deserialize)]
struct Plot {
    plot_id: u64,
    owner: String,
}

/// ApiKey authorization
#[derive(SecurityScheme)]
#[oai(
    ty = "api_key",
    key_name = "User-Agent",
    key_in = "header",
    checker = "api_checker"
)]
struct PlotAuth(Plot);

fn parse_user_agent(header: &str) -> Option<Plot> {
    // Hypercube/7.2 (23612, DynamicCake)
    //
    let start = "Hypercube/7.2 (";
    if !header.starts_with(start) {
        return None;
    }
    let (_, right) = header.split_once("(")?;
    let (plot_id, username) = right.split_once(", ")?;
    let (username, _) = username.split_once(")")?;
    let plot_id: u64 = plot_id.parse().ok()?;
    Some(Plot {
        plot_id,
        owner: username.to_string(),
    })
}

async fn api_checker(req: &Request, api_key: ApiKey) -> Option<Plot> {
    let remote_addr = match *req.remote_addr().as_socket_addr()? {
        SocketAddr::V4(addr) => addr,
        SocketAddr::V6(_) => return None,
    };
    const VALID_IPS: [Ipv4Addr; 2] = [
        Ipv4Addr::new(127, 0, 0, 1),
        Ipv4Addr::new(51, 222, 245, 229),
    ];
    if !VALID_IPS.contains(remote_addr.ip()) {
        info!("Denied ip {}", req.remote_addr());
        return None;
    }
    if let Some(plot) = parse_user_agent(&api_key.key) {
        Some(plot)
    } else {
        error!("Malformed user agent {}", api_key.key);
        None
    }
}

struct Api;

#[derive(Deserialize, Debug)]
struct Config {
    redis_url: String,
    port: u16,
}

#[OpenApi]
#[allow(unused_variables)]
impl Api {
    /// This API returns the currently logged in user.
    #[oai(path = "/hello", method = "get")]
    async fn hello(&self, auth: PlotAuth) -> PlainText<String> {
        PlainText(auth.0.plot_id.to_string())
    }
}

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
        OpenApiService::new(Api, "Test API", "1.0").server("http://localhost:3000/api");
    let ui = api_service.swagger_ui();
    let app = Route::new().nest("/api", api_service).nest("/", ui);

    let _ = poem::Server::new(TcpListener::bind(format!("0.0.0.0:{}", config.port)))
        .run(app)
        .await;
    Ok(())
}
