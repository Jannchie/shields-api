use std::sync::LazyLock;
use std::time::Duration;

use serde::Deserialize;
use utoipa::IntoParams;

pub mod codeberg;
pub mod crates_io;
pub mod github;
pub mod npm;
pub mod pypi;
pub mod statik;

/// Shared HTTP client for all upstream integrations.
pub fn http() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .user_agent(concat!(
                "shields-api/",
                env!("CARGO_PKG_VERSION"),
                " (badge service)"
            ))
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client")
    });
    &CLIENT
}

/// Send a prepared request and parse the JSON body; `None` on any transport,
/// status, or parse failure. Shared by every integration.
pub async fn get_json(rb: reqwest::RequestBuilder) -> Option<serde_json::Value> {
    let resp = rb.send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json().await.ok()
}

/// `get_json` for unauthenticated endpoints that only need a URL.
pub async fn get_json_url(url: impl reqwest::IntoUrl) -> Option<serde_json::Value> {
    get_json(http().get(url)).await
}

/// The `{owner}/{repo}` path parameters shared by all forge endpoints.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub struct OwnerRepo {
    /// Repository owner
    pub owner: String,
    /// Repository name
    pub repo: String,
}
