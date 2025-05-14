use std::net::{Ipv4Addr, SocketAddr};

use poem::{error::ResponseError, Request};
use poem_openapi::{auth::ApiKey, Object, SecurityScheme};
use redis_macros::{FromRedisValue, ToRedisArgs};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::store::Store;

use super::PlotId;

#[derive(SecurityScheme)]
pub enum Auth {
    KeyAuth(KeyAuth),
    PlotAuth(PlotAuth),
}

impl Auth {
    pub fn plot_id(&self) -> PlotId {
        match self {
            Auth::KeyAuth(a) => a.plot_id(),
            Auth::PlotAuth(a) => a.plot_id(),
        }
    }
}

// key auth

/// Guaranteed to be registered
#[derive(Debug, Serialize, Deserialize, ToRedisArgs, FromRedisValue, Object)]
pub struct UuidPlot {
    pub plot_id: PlotId,
    pub owner: Uuid,
}

#[derive(SecurityScheme)]
#[oai(
    ty = "api_key",
    key_name = "X-API-Key",
    key_in = "header",
    checker = "key_checker"
)]
pub struct KeyAuth(UuidPlot);
impl KeyAuth {
    #[inline]
    pub fn plot_id(&self) -> PlotId {
        self.0.plot_id
    }
    #[inline]
    pub fn owner_uuid(&self) -> Uuid {
        self.0.owner
    }
}

async fn key_checker(req: &Request, auth: ApiKey) -> poem::Result<UuidPlot> {
    let store: &Store = req.data().expect("Store should be there");
    Ok(store
        .verify_key(&auth.key)
        .await
        .expect("key check shouldn't fail")
        .ok_or(KeyAuthError::InvalidApiKey)?)
}

#[derive(Debug, thiserror::Error)]
enum KeyAuthError {
    #[error("Invalid API key")]
    InvalidApiKey,
}

impl ResponseError for KeyAuthError {
    fn status(&self) -> reqwest::StatusCode {
        StatusCode::UNAUTHORIZED
    }
}

// plot auth

#[derive(Debug, Serialize, Deserialize, Object)]
pub struct NamePlot {
    pub plot_id: PlotId,
    pub owner: String,
}

/// Plot authorization
#[derive(SecurityScheme)]
#[oai(
    ty = "api_key",
    key_name = "User-Agent",
    key_in = "header",
    checker = "api_checker"
)]
pub struct PlotAuth(NamePlot);
impl PlotAuth {
    #[inline]
    pub fn plot_id(&self) -> PlotId {
        self.0.plot_id
    }
    #[inline]
    pub fn owner_name(&self) -> &str {
        &self.0.owner
    }
}

#[cfg(debug_assertions)]
const DF_IPS: [Ipv4Addr; 2] = [
    Ipv4Addr::new(127, 0, 0, 1),
    Ipv4Addr::new(51, 222, 245, 229),
];
/// DynamicCake: I will only add IPs I see with my own two eyes
#[cfg(not(debug_assertions))]
const DF_IPS: [Ipv4Addr; 1] = [Ipv4Addr::new(51, 222, 245, 229)];

async fn api_checker(req: &Request, user_agent: ApiKey) -> poem::Result<NamePlot> {
    let addr = req
        .remote_addr()
        .as_socket_addr()
        .ok_or(PlotAuthError::NotInternetSocketAddr)?;
    let remote_addr = match *addr {
        SocketAddr::V4(addr) => addr,
        SocketAddr::V6(_) => return Err(PlotAuthError::NotIpv4.into()),
    };
    if !DF_IPS.contains(remote_addr.ip()) {
        info!("Denied ip {}", req.remote_addr());
        return Err(PlotAuthError::InvalidIp.into());
    }
    if let Some(plot) = parse_user_agent(&user_agent.key) {
        Ok(plot)
    } else {
        error!("Malformed user agent {}", user_agent.key);
        Err(PlotAuthError::MalformedUserAgent.into())
    }
}

#[derive(Debug, thiserror::Error)]
enum PlotAuthError {
    #[error("Must be socket error")]
    NotInternetSocketAddr,
    #[error("Must be ipv4")]
    NotIpv4,
    #[error("Ip doesn't match ips: {:?}\nDid you mean to use X-API-Key auth?", DF_IPS)]
    InvalidIp,
    #[error("Malfored User-Agent")]
    MalformedUserAgent,
}

impl ResponseError for PlotAuthError {
    fn status(&self) -> reqwest::StatusCode {
        StatusCode::UNAUTHORIZED
    }
}

fn parse_user_agent(header: &str) -> Option<NamePlot> {
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
    Some(NamePlot {
        plot_id,
        owner: username.to_string(),
    })
}
