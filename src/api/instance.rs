use ascii_domain::dom::Domain;
use poem_openapi::{
    param::Query,
    payload::{Json, PlainText},
    ApiResponse, Object, OpenApi,
};
use sha2::{Digest, Sha256};

use crate::{store::{
    instance::{PlotEditError, RegisterError},
    Store,
}, DOMAIN_SET};

use super::{PlotAuth, PlotId};

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

    #[oai(path = "/plot", method = "get")]
    async fn get_plot_instance(&self, id: Query<PlotId>) -> PlotFetchResult {
        if let Some(plot) = self
            .store
            .find_plot(id.0)
            .await
            .expect("Store ops shouldn't fail")
        {
            PlotFetchResult::Ok(Json(plot.instance.map(|it| it.to_string())))
        } else {
            PlotFetchResult::NotFound
        }
    }

    #[oai(path = "/plot", method = "post")]
    async fn register(&self, instance: Json<Instance>, auth: PlotAuth) -> RegisterResult {
        let id = auth.0.plot_id;
        let uuid = if let Some(id) = self
            .store
            .get_uuid(auth.0.owner)
            .await
            .expect("Store ops shouldn't fail")
        {
            id
        } else {
            return RegisterResult::CannotFetchUuid;
        };

        let domain = if let Some(str) = instance.instance.clone() {
            if let Ok(it) = Domain::try_from_bytes(str, &DOMAIN_SET) {
                Some(it)
            } else {
                return RegisterResult::InvalidDomain;
            }
        } else {
            None
        };
        match self
            .store
            .register_plot(id, uuid, domain.as_ref())
            .await
            .expect("store shouldn't fail")
        {
            Ok(_) => RegisterResult::Success,
            Err(err) => match err {
                RegisterError::InvalidDomain => RegisterResult::InvalidDomain,
                RegisterError::PlotTaken => RegisterResult::PlotAlreadyExists,
            },
        }
    }

    #[oai(path = "/plot", method = "put")]
    async fn replace_instance(
        &self,
        instance: Json<Instance>,
        auth: PlotAuth,
    ) -> ReplaceInstanceResult {
        let id = auth.0.plot_id;
        let domain = if let Some(str) = instance.instance.clone() {
            if let Ok(it) = Domain::try_from_bytes(str, &DOMAIN_SET) {
                Some(it)
            } else {
                return ReplaceInstanceResult::InvalidDomain;
            }
        } else {
            None
        };
        if let Err(err) = self
            .store
            .edit_plot(id, domain.as_ref())
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

#[derive(Object)]
struct Instance {
    instance: Option<String>,
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
