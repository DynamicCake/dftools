use std::net::{Ipv4Addr, SocketAddr};

use poem::Request;
use poem_openapi::{auth::ApiKey, SecurityScheme};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

pub mod baton;

#[derive(Debug, Serialize, Deserialize)]
pub struct Plot {
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
pub struct PlotAuth(Plot);

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
