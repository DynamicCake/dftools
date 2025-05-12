use ascii_domain::{char_set::ASCII_LOWERCASE, dom::Domain};
use futures::{stream, FutureExt, StreamExt};
use poem_openapi::{
    payload::{Json, PlainText},
    ApiResponse, OpenApi,
};
use redis::{aio::MultiplexedConnection, AsyncCommands};
use sqlx::{query_as, Pool, Postgres};

use super::{instance::InstanceApi, PlotAuth, PlotId};

pub struct BatonApi {
    pub pg: Pool<Postgres>,
    pub redis: MultiplexedConnection,
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
        Json(Self::fetch_plot_trust(auth.0.plot_id, &self.pg, self.redis.clone()).await)
    }

    #[oai(path = "/trusted", method = "post")]
    async fn set_trusted(&self, auth: PlotAuth, trusted: Json<Vec<PlotId>>) -> SetTrustedResult {
        let errors: Vec<_> = stream::iter(trusted.0)
            .filter(|id| {
                let domain = Domain::try_from_bytes("oops".to_string(), &ASCII_LOWERCASE).unwrap();
                // FIXME: Error lol
                InstanceApi::find_plot_instance(
                    &self.pg,
                    self.redis.clone(),
                    *id,
                    &domain
                )
                .map(|it| it.is_some())
            })
            .collect()
            .await;

        if errors.is_empty() {
            SetTrustedResult::Success
        } else {
            SetTrustedResult::PlotNotRegistered(Json(errors))
        }
    }
}

impl BatonApi {
    async fn fetch_plot_trust(
        plot: PlotId,
        pg: &Pool<Postgres>,
        mut redis: MultiplexedConnection,
    ) -> Vec<PlotId> {
        let attempt: Option<Vec<PlotId>> = redis
            .get(format!("plot:{}:baton_trust", plot))
            .await
            .expect("redis shouldnt fail");

        if let Some(trusts) = attempt {
            trusts
        } else {
            struct TrustRow {
                trusted: PlotId,
            }
            let trusts: Vec<PlotId> = query_as!(
                TrustRow,
                "SELECT trusted FROM baton_trust WHERE plot = $1;",
                plot
            )
            .fetch_all(pg)
            .await
            .expect("db shouldn't fail")
            .into_iter()
            .map(|it| it.trusted)
            .collect();

            let _: () = redis
                .set(format!("plot:{}:baton_trust", plot), &trusts)
                .await
                .expect("redis shouldn't fail");
            trusts
        }
    }
}

#[derive(ApiResponse)]
enum SetTrustedResult {
    /// Some plots are not registered on this instance.
    /// Register these plots before trying again
    #[oai(status = 400)]
    PlotNotRegistered(Json<Vec<PlotId>>),
    #[oai(status = 200)]
    Success,
}
