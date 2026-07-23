//! npm badge endpoints, mirroring badgers.space/npm/*.
//! Routes use a wildcard segment so both `pkg` and scoped `@org/pkg` names
//! arrive in a single path parameter.

use axum::extract::{Path, Query};
use axum::http::Response;
use serde_json::Value;

use super::get_json_url;
use crate::render::{self, BadgeQuery};

const REGISTRY: &str = "https://registry.npmjs.com";

async fn get_latest(pkg: &str) -> Option<Value> {
    get_json_url(format!("{REGISTRY}/{pkg}/latest")).await
}

fn license_text(data: &Value) -> Option<String> {
    // Older packages publish `license` as an object: { "type": "MIT", ... }
    match &data["license"] {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Object(o) => o.get("type").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

/// The DefinitelyTyped package name for `pkg` (`@scope/name` -> `@types/scope__name`).
fn types_package_name(pkg: &str) -> String {
    match pkg.strip_prefix('@') {
        Some(scoped) => format!("@types/{}", scoped.replace('/', "__")),
        None => format!("@types/{pkg}"),
    }
}

#[utoipa::path(
    get,
    path = "/npm/name/{pkg}",
    params(
        ("pkg" = String, Path, description = "Package name, including the @org/ scope for scoped packages"),
        BadgeQuery,
    ),
    tag = "npm",
    responses((status = 200, description = "Package name badge", content_type = "image/svg+xml"))
)]
pub async fn name(Path(pkg): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let name = get_latest(&pkg)
        .await
        .and_then(|d| d["name"].as_str().map(str::to_string));
    render::text_badge(&q, "npm", "npm", name)
}

#[utoipa::path(
    get,
    path = "/npm/version/{pkg}",
    params(
        ("pkg" = String, Path, description = "Package name, including the @org/ scope for scoped packages"),
        BadgeQuery,
    ),
    tag = "npm",
    responses((status = 200, description = "Latest version badge", content_type = "image/svg+xml"))
)]
pub async fn version(Path(pkg): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let version = get_latest(&pkg)
        .await
        .and_then(|d| d["version"].as_str().map(|v| format!("v{v}")));
    render::text_badge(&q, "npm", "npm", version)
}

#[utoipa::path(
    get,
    path = "/npm/license/{pkg}",
    params(
        ("pkg" = String, Path, description = "Package name, including the @org/ scope for scoped packages"),
        BadgeQuery,
    ),
    tag = "npm",
    responses((status = 200, description = "License badge", content_type = "image/svg+xml"))
)]
pub async fn license(Path(pkg): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    let Some(data) = get_latest(&pkg).await else {
        return render::error_badge(&q, "npm");
    };
    render::license_badge(&q, license_text(&data), None)
}

#[utoipa::path(
    get,
    path = "/npm/types/{pkg}",
    params(
        ("pkg" = String, Path, description = "Package name, including the @org/ scope for scoped packages"),
        BadgeQuery,
    ),
    tag = "npm",
    responses((status = 200, description = "TypeScript types badge", content_type = "image/svg+xml"))
)]
pub async fn types(Path(pkg): Path<String>, Query(q): Query<BadgeQuery>) -> Response<String> {
    // Fetch the package and its @types counterpart concurrently; the extra
    // request is wasted only when the package bundles its own types.
    let types_pkg = types_package_name(&pkg);
    let (data, types_data) = tokio::join!(get_latest(&pkg), get_latest(&types_pkg));
    let Some(data) = data else {
        return render::error_badge(&q, "npm");
    };
    if data["types"].is_string() || data["typings"].is_string() {
        render::badge(&q, "types", "included", Some("blue"))
    } else if types_data.is_some() {
        render::badge(&q, "types", &types_pkg, Some("cyan"))
    } else {
        render::badge(&q, "types", "missing", Some("orange"))
    }
}

#[cfg(test)]
mod tests {
    use super::types_package_name;

    #[test]
    fn types_names() {
        assert_eq!(types_package_name("react"), "@types/react");
        assert_eq!(types_package_name("@babel/core"), "@types/babel__core");
    }
}
