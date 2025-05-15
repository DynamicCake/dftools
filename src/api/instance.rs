use ascii_domain::dom::Domain;
use poem_openapi::{
    param::Query,
    payload::{Json, PlainText},
    ApiResponse, Object, OpenApi,
};
use sha2::{Digest, Sha256};

use crate::{
    instance::Instance, store::{
        instance::{PlotEditError, RegisterError},
        Store,
    }
};

use super::{
    auth::{Auth, PlotAuth, UnregisteredAuth},
    PlotId,
};

pub struct InstanceApi {
    pub store: Store,
    pub instance_domain: Domain<String>,
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

#[OpenApi]
impl InstanceApi {
    #[oai(path = "/vibecheck", method = "get")]
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

    #[oai(path = "/whoami", method = "get")]
    async fn whoami(&self, auth: Auth) -> Json<PlotId> {
        Json(auth.plot().plot_id)
    }

    #[oai(path = "/plot", method = "get")]
    async fn get_plot_instance(&self, id: Query<PlotId>) -> PlotFetchResult {
        if let Some(plot) = self
            .store
            .get_plot(id.0)
            .await
            .expect("Store ops shouldn't fail")
        {
            PlotFetchResult::Ok(Json(plot.instance.into()))
        } else {
            PlotFetchResult::NotFound
        }
    }

    #[oai(path = "/plot", method = "post")]
    async fn register(&self, instance: Json<String>, auth: UnregisteredAuth) -> RegisterResult {
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

        let domain = if let Ok(str) = instance.0.try_into() {
            str
        } else {
            return RegisterResult::InvalidDomain;
        };
        match self
            .store
            .register_plot(plot.plot_id, uuid, &domain)
            .await
            .expect("store shouldn't fail")
        {
            Ok(_) => RegisterResult::Success,
            Err(err) => match err {
                RegisterError::DomainCheckFailed => RegisterResult::InvalidDomain,
                RegisterError::PlotTaken => RegisterResult::PlotAlreadyExists,
            },
        }
    }

    #[oai(path = "/plot", method = "put")]
    async fn replace_instance(
        &self,
        instance: Json<String>,
        auth: Auth,
    ) -> ReplaceInstanceResult {
        let domain: Instance = if let Ok(str) = instance.0.try_into() {
            str
        } else {
            return ReplaceInstanceResult::InvalidDomain;
        };
        if let Err(err) = self
            .store
            .edit_plot(auth.plot().plot_id, &domain)
            .await
            .expect("store ops shouldn't fail")
        {
            match err {
                PlotEditError::InvalidDomain => ReplaceInstanceResult::InvalidDomain,
                PlotEditError::PlotNotFound => ReplaceInstanceResult::PlotNotFound,
            }
        } else {
            ReplaceInstanceResult::Success
        }
    }

    #[oai(path = "/key", method = "post")]
    async fn create_api_key(&self, auth: PlotAuth) -> Json<String> {
        let key = self
            .store
            .create_key(auth.0.plot_id)
            .await
            .expect("store ops shouldn't fail");
        Json(key)
    }
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
    /// Domain is not another active dftools server
    #[oai(status = 400)]
    InvalidDomain,
    /// Success
    #[oai(status = 200)]
    Success,
}

#[derive(ApiResponse)]
enum RegisterResult {
    /// Try again until mojang servers cooperate
    #[oai(status = 500)]
    CannotFetchUuid,
    /// Domain is not another active dftools server
    #[oai(status = 400)]
    InvalidDomain,
    /// Plot already registered
    #[oai(status = 409)]
    PlotAlreadyExists,
    /// Success
    #[oai(status = 200)]
    Success,
}

#[derive(ApiResponse)]
enum PlotFetchResult {
    /// Success
    #[oai(status = 200)]
    Ok(Json<Option<String>>),
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
