use std::net::{Ipv4Addr, SocketAddr};

use poem::{error::ResponseError, Request};
use poem_openapi::{auth::ApiKey, ApiResponse, SecurityScheme};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

pub mod baton;
pub mod instance;

// They cannot be negative, it is just because postgres can return negatives
pub type PlotId = i32;

#[derive(Debug, Serialize, Deserialize)]
pub struct Plot {
    plot_id: PlotId,
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

#[cfg(debug_assertions)]
const DF_IPS: [Ipv4Addr; 2] = [
    Ipv4Addr::new(127, 0, 0, 1),
    Ipv4Addr::new(51, 222, 245, 229),
];
/// DynamicCake: I will only add IPs I see with my own two eyes
#[cfg(not(debug_assertions))]
const DF_IPS: [Ipv4Addr; 1] = [Ipv4Addr::new(51, 222, 245, 229)];

async fn api_checker(req: &Request, api_key: ApiKey) -> poem::Result<Plot> {
    let addr = req
        .remote_addr()
        .as_socket_addr()
        .ok_or(ApiCheckError::NotInternetSocketAddr)?;
    let remote_addr = match *addr {
        SocketAddr::V4(addr) => addr,
        SocketAddr::V6(_) => return Err(ApiCheckError::NotIpv4.into()),
    };
    if !DF_IPS.contains(remote_addr.ip()) {
        info!("Denied ip {}", req.remote_addr());
        return Err(ApiCheckError::InvalidIp.into());
    }
    if let Some(plot) = parse_user_agent(&api_key.key) {
        Ok(plot)
    } else {
        error!("Malformed user agent {}", api_key.key);
        Err(ApiCheckError::MalformedUserAgent.into())
    }
}

#[derive(Debug, thiserror::Error)]
enum ApiCheckError {
    #[error("Must be socket error")]
    NotInternetSocketAddr,
    #[error("Must be ipv4")]
    NotIpv4,
    #[error("Ip doesn't match ips: {:?}", DF_IPS)]
    InvalidIp,
    #[error("Malfored user-agent")]
    MalformedUserAgent,
}

impl ResponseError for ApiCheckError {
    fn status(&self) -> reqwest::StatusCode {
        return StatusCode::UNAUTHORIZED;
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
    let plot_id: PlotId = plot_id.parse().ok()?;
    Some(Plot {
        plot_id,
        owner: username.to_string(),
    })
}
