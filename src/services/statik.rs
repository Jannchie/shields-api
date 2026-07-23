//! Static badge endpoints (spacebadgers worker routes).

use axum::extract::{Path, Query};
use axum::http::Response;

use crate::render::{self, BadgeQuery};

#[utoipa::path(
    get,
    path = "/badge/{label}/{status}",
    params(
        ("label" = String, Path, description = "Badge label (left side text)"),
        ("status" = String, Path, description = "Badge status (right side text)"),
        BadgeQuery,
    ),
    tag = "Static Badge",
    responses((status = 200, description = "Badge SVG", content_type = "image/svg+xml"))
)]
pub async fn badge(
    Path((label, status)): Path<(String, String)>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    render::badge(&q, &label, &status, None)
}

#[utoipa::path(
    get,
    path = "/badge/{label}/{status}/{color}",
    params(
        ("label" = String, Path, description = "Badge label (left side text)"),
        ("status" = String, Path, description = "Badge status (right side text)"),
        ("color" = String, Path, description = "Status color: named color, hex, or CSS color"),
        BadgeQuery,
    ),
    tag = "Static Badge",
    responses((status = 200, description = "Badge SVG", content_type = "image/svg+xml"))
)]
pub async fn badge_with_color(
    Path((label, status, color)): Path<(String, String, String)>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    render::badge(&q, &label, &status, Some(&color))
}
