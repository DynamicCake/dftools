use futures::{stream, StreamExt};
use poem_openapi::{param::Query, payload::Json, ApiResponse, OpenApi};

use crate::{dfjson::DfJson, store::Store};

use super::{auth::Auth, PlotId};

pub struct BatonApi {
    pub store: Store,
}

#[OpenApi]
impl BatonApi {
    /// List trusted plots that can set transfer
    #[oai(path = "/trusted", method = "get")]
    async fn get_trusted(&self, auth: Auth) -> Json<Vec<PlotId>> {
        Json(
            self.store
                .fetch_plot_trust(auth.plot().plot_id)
                .await
                .expect("Store ops shouldn't fail"),
        )
    }

    /// Replace all trusted plots
    #[oai(path = "/trusted", method = "post")]
    async fn set_trusted(&self, auth: Auth, trusted: Json<Vec<PlotId>>) -> SetTrustedResult {
        async fn plot_not_exists(store: &Store, id: PlotId) -> Option<PlotId> {
            if store
                .plot_exists(id)
                .await
                .expect("plot_exists shouldn't fail")
            {
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
            if let Err(_err) = self
                .store
                .set_plot_trust(auth.plot().plot_id, trusted.0)
                .await
                .expect("Store ops shouldn't fail")
            {
                return SetTrustedResult::PlotNotFound;
            }
            SetTrustedResult::Success
        } else {
            SetTrustedResult::OtherPlotNotRegistered(Json(errors))
        }
    }

    /// TODO: Finish making this function lol
    #[oai(path = "/transfer", method = "post")]
    async fn transfer(&self, dest: Query<PlotId>) -> SetTransferResult {
        let found = if let Some(it) = self
            .store
            .get_plot(dest.0)
            .await
            .expect("Get plot shouldn't fail")
        {
            it
        } else {
            return SetTransferResult::PlotNotFound;
        };
        // let str: Option<String> = found.instance.into();
        SetTransferResult::Ok
    }

    /*
    {
        "plot_origin": 41808, // The plot id that sent the transfer
        "time_set": 1743544800, // The time the plot claimed to send the transfer
        "data": { // Payload (DFJSON)
            "id": "str",
            "val": "Hello world!"
        }
        */

    /// [EXT] Set transfer to a plot managed by this instance
    #[oai(path = "/send/transfer", method = "get")]
    async fn transfer_recv(
        &self,
        plot_id: Query<PlotId>,
        payload: Json<DfJson>,
    ) -> TransferSendResult {
        let plot_id = plot_id.0;
        let plot = self
            .store
            .get_plot(plot_id)
            .await
            .expect("store ops shouldn't fail");

        self.store
            .set_transfer(plot_id, payload.0)
            .await
            .expect("store ops shouldn't fail");
        TransferSendResult::Ok
    }
}

#[derive(ApiResponse)]
enum TransferSendResult {
    #[oai(status = 409)]
    PlotInstanceInconsistency(Json<Option<String>>),
    #[oai(status = 404)]
    PlotNotFound,
    #[oai(status = 200)]
    Ok,
}

#[derive(ApiResponse)]
enum SetTransferResult {
    /// Plot not found
    #[oai(status = 404)]
    PlotNotFound,
    /// Ok
    #[oai(status = 200)]
    Ok,
}

#[derive(ApiResponse)]
enum SetTrustedResult {
    #[oai(status = 404)]
    PlotNotFound,
    /// Some plots are not registered on this instance.
    /// Register these plots before trying again
    #[oai(status = 409)]
    OtherPlotNotRegistered(Json<Vec<PlotId>>),
    #[oai(status = 200)]
    Success,
}
