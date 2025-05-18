use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use poem::{error::ResponseError, Request};
use poem_openapi::{auth::ApiKey, Object, SecurityScheme};
use redis_macros::{FromRedisValue, ToRedisArgs};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    instance::{Instance, SendInstance},
    store::Store,
};

use super::PlotId;

#[derive(Debug, Serialize, Deserialize, Object)]
pub struct ExternalServer {
    pub sub: SendInstance,
    pub iat: u64,
    pub exp: u64,
    pub jti: Uuid,
}

#[derive(SecurityScheme)]
#[oai(
    ty = "api_key",
    key_name = "X-Server-Key",
    key_in = "header",
    checker = "check_server"
)]
pub struct ExternalServerAuth(pub ExternalServer);

const JWT_VERSION: u64 = 1747450744;

pub async fn check_server(req: &Request, key: ApiKey) -> poem::Result<ExternalServer> {
    let store = req.data::<Store>().expect("Store should here");
    let server = store
        .verify_jwt::<ExternalServer>(&key.key)
        .ok_or(ServerAuthError::CannotVerify)?;

    if server.iat < JWT_VERSION {
        return Err(ServerAuthError::VersionMismatch.into());
    }
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    if server.exp < time {
        return Err(ServerAuthError::Expired.into());
    }
    Ok(server)
}

#[derive(Debug, thiserror::Error)]
pub enum ServerAuthError {
    #[error("Cannot verify token")]
    CannotVerify,
    #[error("Token expired")]
    Expired,
    #[error("Version mismatch (please regenerate token)")]
    VersionMismatch,
}

impl ResponseError for ServerAuthError {
    fn status(&self) -> StatusCode {
        StatusCode::UNAUTHORIZED
    }
}

pub struct UnregisteredPlot {
    pub plot_id: PlotId,
    pub owner: String,
}

#[derive(SecurityScheme)]
#[oai(
    ty = "api_key",
    key_name = "User-Agent",
    key_in = "header",
    checker = "check_unreg_plot"
)]
pub struct UnregisteredAuth(pub UnregisteredPlot);

pub async fn check_unreg_plot(req: &Request, user_agent: ApiKey) -> poem::Result<UnregisteredPlot> {
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

#[derive(SecurityScheme)]
pub enum Auth {
    KeyAuth(KeyAuth),
    PlotAuth(PlotAuth),
}

impl Auth {
    pub fn plot(self) -> Plot {
        match self {
            Auth::KeyAuth(a) => a.0,
            Auth::PlotAuth(a) => a.0,
        }
    }
}

// key auth

/// Guaranteed to be registered
#[derive(Debug, Serialize, Deserialize, ToRedisArgs, FromRedisValue, FromRow, Clone)]
pub struct Plot {
    pub plot_id: PlotId,
    pub owner: Uuid,
    pub instance: Instance,
}

#[derive(SecurityScheme)]
#[oai(
    ty = "api_key",
    key_name = "X-API-Key",
    key_in = "header",
    checker = "key_checker"
)]
pub struct KeyAuth(pub Plot);

async fn key_checker(req: &Request, auth: ApiKey) -> poem::Result<Plot> {
    let store: &Arc<Store> = req.data().expect("Store should be there");
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

/// Plot authorization
#[derive(SecurityScheme)]
#[oai(
    ty = "api_key",
    key_name = "User-Agent",
    key_in = "header",
    checker = "plot_checker"
)]
pub struct PlotAuth(pub Plot);

#[cfg(debug_assertions)]
const DF_IPS: [Ipv4Addr; 2] = [
    Ipv4Addr::new(127, 0, 0, 1),
    Ipv4Addr::new(51, 222, 245, 229),
];
/// DynamicCake: I will only add IPs I see with my own two eyes
#[cfg(not(debug_assertions))]
const DF_IPS: [Ipv4Addr; 1] = [Ipv4Addr::new(51, 222, 245, 229)];

async fn plot_checker(req: &Request, user_agent: ApiKey) -> poem::Result<Plot> {
    let unreg = check_unreg_plot(req, user_agent).await?;
    let store: &Arc<Store> = req.data().expect("Server should have store");
    let plot = store
        .get_plot(unreg.plot_id)
        .await
        .expect("Cannot get plot")
        .ok_or(PlotAuthError::PlotNotRegistered)?;
    Ok(Plot {
        plot_id: unreg.plot_id,
        owner: plot.owner,
        instance: plot.instance,
    })
}

#[derive(Debug, thiserror::Error)]
enum PlotAuthError {
    #[error("Plot not registered")]
    PlotNotRegistered,
    #[error("Must be socket error for plot auth")]
    NotInternetSocketAddr,
    #[error("Must be ipv4 for plot auth")]
    NotIpv4,
    #[error(
        "Ip doesn't match ips: {:?}\nDid you mean to use X-API-Key auth?",
        DF_IPS
    )]
    InvalidIp,
    #[error("Malfored User-Agent")]
    MalformedUserAgent,
}

impl ResponseError for PlotAuthError {
    fn status(&self) -> reqwest::StatusCode {
        StatusCode::UNAUTHORIZED
    }
}

fn parse_user_agent(header: &str) -> Option<UnregisteredPlot> {
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
    Some(UnregisteredPlot {
        plot_id,
        owner: username.to_string(),
    })
}
