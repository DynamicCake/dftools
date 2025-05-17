use reqwest::StatusCode;

use crate::instance::{Instance, InstanceDomain};

use super::Store;

impl Store {
    /// Do not just blindly ? this function
    pub async fn vibe_check(&self, instance: Instance) -> Result<bool, reqwest::Error> {
        let str: Option<&String> = todo!();
        if let Some(domain) = str {
            let res = self
                .client
                .get(format!("https://{}/instance/v0/ping", domain))
                .send()
                .await?;
            Ok(res.status() == StatusCode::NO_CONTENT)
        } else {
            Ok(true)
        }
    }
    pub async fn send() {}
}
