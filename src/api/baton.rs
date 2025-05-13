use futures::{stream, StreamExt};
use poem_openapi::{
    payload::{Json, PlainText},
    ApiResponse, OpenApi,
};
use tracing::info;

use crate::store::Store;

use super::{PlotAuth, PlotId};

pub struct BatonApi {
    pub store: Store,
}

#[OpenApi]
impl BatonApi {
    /// This API returns the currently logged in user.
    #[oai(path = "/hello", method = "get")]
    async fn hello(&self, auth: PlotAuth) -> PlainText<String> {
        PlainText(auth.0.plot_id.to_string())
    }

    #[oai(path = "/trusted", method = "get")]
    async fn get_trusted(&self, auth: PlotAuth) -> Json<Vec<PlotId>> {
        Json(
            self.store
                .fetch_plot_trust(auth.0.plot_id)
                .await
                .expect("Store ops shouldn't fail"),
        )
    }

    #[oai(path = "/trusted", method = "post")]
    async fn set_trusted(&self, auth: PlotAuth, trusted: Json<Vec<PlotId>>) -> SetTrustedResult {
        async fn plot_not_exists(store: &Store, id: PlotId) -> Option<PlotId> {
            if store.plot_exists(id).await.expect("plot_exists shouldn't fail") {
                None
            } else {
                Some(id)
            }
        }
        let errors = stream::iter(&trusted.0)
            .filter_map(|id| plot_not_exists(&self.store, *id))
            .collect::<Vec<_>>()
            .await;

        if errors.is_empty() {
            // TODO: Implement updating
            info!("Set plot trust to {:?}", trusted.0);
            SetTrustedResult::Success
        } else {
            SetTrustedResult::PlotNotRegistered(Json(errors))
        }
    }
}

impl BatonApi {}

#[derive(ApiResponse)]
enum SetTrustedResult {
    /// Some plots are not registered on this instance.
    /// Register these plots before trying again
    #[oai(status = 400)]
    PlotNotRegistered(Json<Vec<PlotId>>),
    #[oai(status = 200)]
    Success,
}
