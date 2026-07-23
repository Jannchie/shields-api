//! crates.io badge endpoints, mirroring badgers.space/crates/*.

use axum::extract::{Path, Query};
use axum::http::Response;
use serde_json::Value;

use super::get_json_url;
use crate::render::{self, BadgeQuery, compact_number};

const API: &str = "https://crates.io/api/v1";

/// Latest non-yanked version, most recently updated first (spacebadgers order).
fn latest_version(versions: &Value) -> Option<&Value> {
    versions
        .as_array()?
        .iter()
        .filter(|v| !v["yanked"].as_bool().unwrap_or(false))
        // RFC 3339 timestamps compare correctly as strings
        .max_by_key(|v| v["updated_at"].as_str().unwrap_or_default())
}

#[utoipa::path(
    get,
    path = "/crates/name/{crate}",
    params(
        ("crate" = String, Path, description = "Crate name"),
        BadgeQuery,
    ),
    tag = "crates.io",
    responses((status = 200, description = "Crate name badge", content_type = "image/svg+xml"))
)]
pub async fn name(Path(krate): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let name = get_json_url(format!("{API}/crates/{krate}"))
        .await
        .and_then(|v| v["crate"]["name"].as_str().map(str::to_string));
    render::text_badge(&q, "crates.io", "crates.io", name)
}

#[utoipa::path(
    get,
    path = "/crates/version/{crate}",
    params(
        ("crate" = String, Path, description = "Crate name"),
        BadgeQuery,
    ),
    tag = "crates.io",
    responses((status = 200, description = "Latest version badge", content_type = "image/svg+xml"))
)]
pub async fn version(Path(krate): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let num = get_json_url(format!("{API}/crates/{krate}/versions"))
        .await
        .and_then(|v| {
            latest_version(&v["versions"]).and_then(|v| v["num"].as_str().map(str::to_string))
        });
    render::text_badge(&q, "crates.io", "crates.io", num.map(|n| format!("v{n}")))
}

#[utoipa::path(
    get,
    path = "/crates/info/{crate}",
    params(
        ("crate" = String, Path, description = "Crate name"),
        BadgeQuery,
    ),
    tag = "crates.io",
    responses((status = 200, description = "Crate name and version badge", content_type = "image/svg+xml"))
)]
pub async fn info(Path(krate): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let info = get_json_url(format!("{API}/crates/{krate}")).await.and_then(|v| {
        let name = v["crate"]["name"].as_str()?;
        let num = latest_version(&v["versions"])?["num"].as_str()?;
        Some(format!("{name} v{num}"))
    });
    render::text_badge(&q, "crates.io", "crates.io", info)
}

#[utoipa::path(
    get,
    path = "/crates/downloads/{crate}",
    params(
        ("crate" = String, Path, description = "Crate name"),
        BadgeQuery,
    ),
    tag = "crates.io",
    responses((status = 200, description = "Total downloads badge", content_type = "image/svg+xml"))
)]
pub async fn downloads(Path(krate): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let count = get_json_url(format!("{API}/crates/{krate}"))
        .await
        .and_then(|v| v["crate"]["downloads"].as_u64());
    render::text_badge(&q, "downloads", "crates.io", count.map(compact_number))
}

#[utoipa::path(
    get,
    path = "/crates/downloads/{crate}/latest",
    params(
        ("crate" = String, Path, description = "Crate name"),
        BadgeQuery,
    ),
    tag = "crates.io",
    responses((status = 200, description = "Latest version downloads badge", content_type = "image/svg+xml"))
)]
pub async fn downloads_latest(
    Path(krate): Path<String>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    let count = get_json_url(format!("{API}/crates/{krate}/versions"))
        .await
        .and_then(|v| latest_version(&v["versions"]).and_then(|v| v["downloads"].as_u64()));
    render::text_badge(
        &q,
        "downloads",
        "crates.io",
        count.map(|n| format!("{} latest version", compact_number(n))),
    )
}
