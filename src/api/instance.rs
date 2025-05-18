use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use ascii_domain::dom::Domain;
use base64::Engine;
use ed25519_dalek::VerifyingKey;
use poem_openapi::{
    param::Query,
    payload::{Json, PlainText},
    ApiResponse, Object, OpenApi,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    instance::{InstanceDomain, SendInstance},
    store::{
        instance::{PlotEditError, RegisterError},
        Store,
    },
    BASE64,
};

use super::{
    auth::{Auth, ExternalServer, PlotAuth, UnregisteredAuth},
    PlotId,
};

pub struct InstanceApi {
    pub store: Arc<Store>,
    pub domain: Domain<String>,
}

#[derive(Serialize, Deserialize, Object)]
pub struct VerificationResponse {
    /// Base64 encoded public key
    pub server_key: String,
    /// The signature to the of the sent text
    pub signature: String,
}

#[derive(ApiResponse)]
pub enum FetchTokenResponse {
    /// Internal domain used
    #[oai(status = 400)]
    InternalDomainUsed,
    /// Instance parse error
    #[oai(status = 400)]
    InstanceParseError(PlainText<String>),

    /// Cannot ping instance
    #[oai(status = 500)]
    CannotPingInstance,
    /// Inconsistent Keys, returned body is the actual key
    #[oai(status = 403)]
    InconsistentKeys(PlainText<String>),
    /// Ok
    #[oai(status = 200)]
    Ok(PlainText<String>),
}

#[OpenApi]
impl InstanceApi {
    /// Get the server's public key
    #[oai(path = "/sign", method = "get")]
    async fn vibecheck(&self, tosign: Query<String>) -> Json<VerificationResponse> {
        let sig = self.store.sign(tosign.0.as_bytes()).await;
        Json(VerificationResponse {
            server_key: BASE64.encode(self.store.public_key()),
            signature: BASE64.encode(sig.to_bytes()),
        })
    }

    /// Provide your server domain and identity key for a jwt to communicate with the server
    #[oai(path = "/server-token", method = "get")]
    async fn get_server_token(
        &self,
        key: Query<String>,
        domain: Query<String>,
    ) -> FetchTokenResponse {
        let send_instance = SendInstance {
            key: key.0,
            domain: domain.0,
        };
        let claimed_instance = match send_instance.parse() {
            Ok(inst) => inst,
            Err(err) => return FetchTokenResponse::InstanceParseError(PlainText(err.to_string())),
        };
        let domain = if let InstanceDomain::External(ext) = claimed_instance.domain {
            ext
        } else {
            return FetchTokenResponse::InternalDomainUsed;
        };
        if self.store.public_key() == claimed_instance.key {
            return FetchTokenResponse::InternalDomainUsed;
        }
        let tok = if let Ok(tok) = self.store.ping_instance(&domain).await {
            tok
        } else {
            return FetchTokenResponse::CannotPingInstance;
        };
        if claimed_instance.key != tok {
            return FetchTokenResponse::InconsistentKeys(PlainText(BASE64.encode(tok)));
        }

        const JWT_EXPIRY: u64 = 60 * 60 * 3;
        let issued = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let token = ExternalServer {
            sub: send_instance,
            iat: issued,
            exp: issued + JWT_EXPIRY,
            jti: Uuid::new_v4(),
        };
        let signed = self.store.sign_jwt(&token).expect("signing failed");

        FetchTokenResponse::Ok(PlainText(signed))
    }

    /// Get the plot id
    #[oai(path = "/whoami", method = "get")]
    async fn whoami(&self, auth: Auth) -> Json<PlotId> {
        Json(auth.plot().plot_id)
    }

    /// Get the plot's instance
    #[oai(path = "/plot", method = "get")]
    async fn get_plot_instance(&self, id: Query<PlotId>) -> PlotFetchResult {
        if let Some(plot) = self
            .store
            .get_plot(id.0)
            .await
            .expect("Store ops shouldn't fail")
        {
            PlotFetchResult::Ok(PlainText(plot.instance.encode(&self.domain)))
        } else {
            PlotFetchResult::NotFound
        }
    }

    /// Register the plot to an instance with the public key
    #[oai(path = "/plot", method = "post")]
    async fn register(
        &self,
        instance_key: Json<Option<String>>,
        auth: UnregisteredAuth,
    ) -> RegisterResult {
        let plot = auth.0;
        let uuid = if let Some(id) = self
            .store
            .get_uuid(&plot.owner)
            .await
            .expect("Store ops shouldn't fail")
        {
            id
        } else {
            return RegisterResult::CannotFetchUuid;
        };

        let key = if let Some(key) = &instance_key.0 {
            let key = match BASE64.decode(key) {
                Ok(key) => key,
                Err(err) => {
                    return RegisterResult::InvalidKeyFormat(PlainText(format!(
                        "base64 decode: {}",
                        err
                    )))
                }
            };
            let key: [u8; 32] = match key.as_slice().try_into() {
                Ok(key) => key,
                Err(err) => return RegisterResult::InvalidKeyFormat(PlainText(err.to_string())),
            };
            match VerifyingKey::from_bytes(&key) {
                Ok(key) => Some(key),
                Err(err) => {
                    return RegisterResult::InvalidKeyFormat(PlainText(format!(
                        "converting to verify key failed: {}",
                        err
                    )))
                }
            }
        } else {
            None
        };
        match self
            .store
            .register_plot(plot.plot_id, uuid, key.as_ref())
            .await
            .expect("store shouldn't fail")
        {
            Ok(_) => RegisterResult::Ok,
            Err(err) => match err {
                RegisterError::PlotTaken => RegisterResult::PlotAlreadyExists,
                RegisterError::InstanceNotFound => {
                    RegisterResult::InstanceNotRegistered(PlainText("Instance not registered"))
                }
            },
        }
    }

    /// Change the plot instance with the public key
    #[oai(path = "/plot", method = "put")]
    async fn replace_instance(
        &self,
        instance_key: Json<Option<String>>,
        auth: Auth,
    ) -> ReplaceInstanceResult {
        let plot = auth.plot();

        let key = if let Some(key) = &instance_key.0 {
            let key = match BASE64.decode(key) {
                Ok(key) => key,
                Err(err) => {
                    return ReplaceInstanceResult::InvalidKeyFormat(PlainText(format!(
                        "base64 decode: {}",
                        err
                    )))
                }
            };
            let key: [u8; 32] = match key.as_slice().try_into() {
                Ok(key) => key,
                Err(err) => {
                    return ReplaceInstanceResult::InvalidKeyFormat(PlainText(err.to_string()))
                }
            };
            match VerifyingKey::from_bytes(&key) {
                Ok(key) => Some(key),
                Err(err) => {
                    return ReplaceInstanceResult::InvalidKeyFormat(PlainText(format!(
                        "converting to verify key failed: {}",
                        err
                    )))
                }
            }
        } else {
            None
        };
        if let Err(err) = self
            .store
            .edit_plot(plot.plot_id, key.as_ref())
            .await
            .expect("store ops shouldn't fail")
        {
            match err {
                PlotEditError::PlotNotFound => ReplaceInstanceResult::PlotNotFound,
                PlotEditError::InstanceNotFound => ReplaceInstanceResult::InstanceNotRegisterd,
            }
        } else {
            ReplaceInstanceResult::Success
        }
    }

    /// Create an api key
    #[oai(path = "/key", method = "post")]
    async fn create_api_key(&self, auth: PlotAuth) -> Json<String> {
        let key = self
            .store
            .create_key(auth.0.plot_id)
            .await
            .expect("store ops shouldn't fail");
        Json(key)
    }
    /// Purge all api keys
    #[oai(path = "/key", method = "delete")]
    async fn delete_all_api_keys(&self, auth: Auth) {
        self.store
            .disable_all_keys(auth.plot().plot_id)
            .await
            .expect("store ops shouldn't fail");
    }
}

#[derive(ApiResponse)]
enum ReplaceInstanceResult {
    /// Plot not found
    #[oai(status = 404)]
    PlotNotFound,
    /// An instance with this key is not registered
    #[oai(status = 400)]
    InstanceNotRegisterd,
    /// Invalid key format
    #[oai(status = 400)]
    InvalidKeyFormat(PlainText<String>),
    /// Success
    #[oai(status = 200)]
    Success,
}

#[derive(ApiResponse)]
enum RegisterResult {
    /// Try again until mojang servers cooperate
    #[oai(status = 500)]
    CannotFetchUuid,
    /// An instance with this key is not registered
    #[oai(status = 400)]
    InstanceNotRegistered(PlainText<&'static str>),
    /// Invalid key format
    #[oai(status = 400)]
    InvalidKeyFormat(PlainText<String>),
    /// Plot already registered
    #[oai(status = 409)]
    PlotAlreadyExists,
    /// Ok
    #[oai(status = 200)]
    Ok,
}

#[derive(ApiResponse)]
enum PlotFetchResult {
    /// Ok
    #[oai(status = 200)]
    Ok(PlainText<String>),
    /// Plot not found
    #[oai(status = 404)]
    NotFound,
}

#[derive(Object)]
pub struct PlotResponse {
    plot: PlotId,
    owner: String,
    instance: String,
}
