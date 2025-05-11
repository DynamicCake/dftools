use poem_openapi::{payload::PlainText, OpenApi};

use super::PlotAuth;

pub struct BatonApi;

#[OpenApi]
impl BatonApi {
    /// This API returns the currently logged in user.
    #[oai(path = "/hello", method = "get")]
    async fn hello(&self, auth: PlotAuth) -> PlainText<String> {
        PlainText(auth.0.plot_id.to_string())
    }
}
