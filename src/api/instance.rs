use std::time::{SystemTime, UNIX_EPOCH};

use ascii_domain::dom::Domain;
use base64::{prelude::BASE64_STANDARD, DecodeError, Engine};
use ed25519_dalek::VerifyingKey;
use jwt::{FromBase64, SignWithKey};
use poem_openapi::{
    param::Query,
    payload::{Json, PlainText},
    ApiResponse, Object, OpenApi,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    instance::{Instance, InstanceDomain, SendInstance},
    store::{
        instance::{PlotEditError, RegisterError},
        Store,
    },
};

use super::{
    auth::{Auth, ExternalServer, PlotAuth, UnregisteredAuth},
    PlotId,
};

pub struct InstanceApi {
    pub store: Store,
    pub domain: Domain<String>,
    pub self_check_key: String,
}

#[derive(ApiResponse)]
pub enum VibeCheckResult {
    #[oai(status = 204)]
    Passed,
    #[oai(status = 200)]
    KeyPassed(PlainText<&'static str>),
    #[oai(status = 400)]
    KeyVibecheckFailed,
}

#[derive(ApiResponse)]
pub enum FetchTokenResponse {
    #[oai(status = 400)]
    InstanceParseError,
    /// Inconsistent Keys, returned body is the actual key
    #[oai(status = 403)]
    InconsistentKeys(PlainText<String>),
    #[oai(status = 200)]
    Ok(PlainText<String>),
}

#[OpenApi]
impl InstanceApi {
    /// Give the server a vibe check
    ///
    /// If you are another instance, call this endpoint to get info on this server.
    /// To verify that the config domain is you own domain, hit this endpoint with your self check key.
    #[oai(path = "/ping", method = "get")]
    async fn vibecheck(&self, key: Query<Option<String>>) -> VibeCheckResult {
        if let Some(key) = key.0 {
            let hash: [u8; 32] = Sha256::digest(key).into();
            let actual = self.get_vibecheck_hash();
            if hash == actual {
                return VibeCheckResult::KeyPassed(PlainText("You are you"));
            } else {
                return VibeCheckResult::KeyVibecheckFailed;
            }
        }
        VibeCheckResult::Passed
    }

    #[oai(path = "/server-token", method = "get")]
    async fn get_server_token(&self, instance: Json<SendInstance>) -> FetchTokenResponse {
        let pinstance = if let Ok(inst) = instance.0.parse() {
            inst
        } else {
            return FetchTokenResponse::InstanceParseError;
        };
        let ok = self.store.ping_instance(&pinstance).await.expect("Error while verifying instance");
        if !ok {
            return FetchTokenResponse::InconsistentKeys()
        }

        const JWT_EXPIRY: u64 = 60 * 60 * 3;
        let issued = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let token = ExternalServer {
            sub: instance.0,
            iat: issued,
            exp: issued + JWT_EXPIRY,
            jti: Uuid::new_v4()
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

    /// Register the plot to an instance
    ///
    /// Leave the body blank to register to this instance
    #[oai(path = "/plot", method = "post")]
    async fn register(
        &self,
        instance_key: PlainText<String>,
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

        let key = match VerifyingKey::from_base64(&instance_key.0) {
            Ok(key) => key,
            Err(err) => return RegisterResult::InvalidKeyFormat(PlainText(err.to_string())),
        };
        match self
            .store
            .register_plot(plot.plot_id, uuid, &key)
            .await
            .expect("store shouldn't fail")
        {
            Ok(_) => RegisterResult::Ok,
            Err(err) => match err {
                RegisterError::PlotTaken => RegisterResult::PlotAlreadyExists,
                RegisterError::InstanceNotFound => RegisterResult::InstanceNotRegisterd,
            },
        }
    }

    /// Change the plot instance
    #[oai(path = "/plot", method = "put")]
    async fn replace_instance(
        &self,
        instance_key: Json<String>,
        auth: Auth,
    ) -> ReplaceInstanceResult {
        let plot = auth.plot();

        let key = match VerifyingKey::from_base64(&instance_key.0) {
            Ok(key) => key,
            Err(err) => return ReplaceInstanceResult::InvalidKeyFormat(PlainText(err.to_string())),
        };
        if let Err(err) = self
            .store
            .edit_plot(plot.plot_id, &key)
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

impl InstanceApi {
    fn get_vibecheck_hash(&self) -> [u8; 32] {
        let hashed = Sha256::digest(&self.self_check_key);
        hashed.into()
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
    InstanceNotRegisterd,
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
