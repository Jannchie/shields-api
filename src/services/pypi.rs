//! PyPI badge endpoints, mirroring badgers.space/pypi/*.

use axum::extract::{Path, Query};
use axum::http::Response;
use serde_json::Value;

use super::get_json_url;
use crate::render::{self, BadgeQuery};

const API: &str = "https://pypi.org/pypi";

/// Fetch the package JSON; callers read fields from the `info` object via
/// `["info"]["..."]` — no clone of the (potentially large) response.
async fn get_package(pkg: &str) -> Option<Value> {
    get_json_url(format!("{API}/{pkg}/json")).await
}

#[utoipa::path(
    get,
    path = "/pypi/name/{pkg}",
    params(("pkg" = String, Path, description = "Package name"), BadgeQuery),
    tag = "PyPI",
    responses((status = 200, description = "Package name badge", content_type = "image/svg+xml"))
)]
pub async fn name(Path(pkg): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let name = get_package(&pkg)
        .await
        .and_then(|p| p["info"]["name"].as_str().map(str::to_string));
    render::text_badge(&q, "pypi", "pypi", name)
}

#[utoipa::path(
    get,
    path = "/pypi/version/{pkg}",
    params(("pkg" = String, Path, description = "Package name"), BadgeQuery),
    tag = "PyPI",
    responses((status = 200, description = "Latest version badge", content_type = "image/svg+xml"))
)]
pub async fn version(Path(pkg): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let version = get_package(&pkg)
        .await
        .and_then(|p| p["info"]["version"].as_str().map(|v| format!("v{v}")));
    render::text_badge(&q, "pypi", "pypi", version)
}

#[utoipa::path(
    get,
    path = "/pypi/info/{pkg}",
    params(("pkg" = String, Path, description = "Package name"), BadgeQuery),
    tag = "PyPI",
    responses((status = 200, description = "Package name and version badge", content_type = "image/svg+xml"))
)]
pub async fn info(Path(pkg): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let info = get_package(&pkg).await.and_then(|p| {
        let name = p["info"]["name"].as_str()?;
        let version = p["info"]["version"].as_str()?;
        Some(format!("{name} v{version}"))
    });
    render::text_badge(&q, "pypi", "pypi", info)
}

#[utoipa::path(
    get,
    path = "/pypi/license/{pkg}",
    params(("pkg" = String, Path, description = "Package name"), BadgeQuery),
    tag = "PyPI",
    responses((status = 200, description = "License badge", content_type = "image/svg+xml"))
)]
pub async fn license(Path(pkg): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let Some(package) = get_package(&pkg).await else {
        return render::error_badge(&q, "pypi");
    };
    let license = package["info"]["license"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    render::license_badge(&q, license, Some("blue"))
}
