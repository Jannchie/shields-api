//! Codeberg (Gitea/Forgejo) badge endpoints, mirroring badgers.space/codeberg/*.

use std::sync::LazyLock;

use axum::extract::{Path, Query};
use axum::http::Response;
use serde_json::Value;

use super::{OwnerRepo, http};
use crate::render::{self, BadgeQuery};

const API: &str = "https://codeberg.org/api/v1";

fn request(url: &str) -> reqwest::RequestBuilder {
    static TOKEN: LazyLock<Option<String>> = LazyLock::new(|| {
        std::env::var("CODEBERG_TOKEN").ok().filter(|t| !t.is_empty())
    });
    let mut rb = http().get(url);
    if let Some(token) = TOKEN.as_deref() {
        rb = rb.header("Authorization", format!("token {token}"));
    }
    rb
}

async fn get_json(url: &str) -> Option<Value> {
    super::get_json(request(url)).await
}

#[utoipa::path(
    get,
    path = "/codeberg/release/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "Codeberg",
    responses((status = 200, description = "Latest release badge", content_type = "image/svg+xml"))
)]
pub async fn release(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    // spacebadgers shows the shorter of tag_name and release name.
    let name = get_json(&format!("{API}/repos/{owner}/{repo}/releases/latest"))
        .await
        .and_then(|v| {
            let tag = v["tag_name"].as_str().unwrap_or_default();
            let name = v["name"].as_str().unwrap_or_default();
            let shortest = [tag, name]
                .into_iter()
                .filter(|s| !s.is_empty())
                .min_by_key(|s| s.len())?;
            Some(shortest.to_string())
        });
    render::release_badge(&q, name)
}

async fn issue_count(owner: &str, repo: &str, state: &str) -> Option<u64> {
    let resp = request(&format!(
        "{API}/repos/{owner}/{repo}/issues?type=issues&state={state}&limit=1"
    ))
    .send()
    .await
    .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.headers()
        .get("x-total-count")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
}

#[utoipa::path(
    get,
    path = "/codeberg/issues/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "Codeberg",
    responses((status = 200, description = "Total issues badge", content_type = "image/svg+xml"))
)]
pub async fn issues(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    render::count_badge(&q, "issues", issue_count(&owner, &repo, "all").await)
}

#[utoipa::path(
    get,
    path = "/codeberg/open-issues/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "Codeberg",
    responses((status = 200, description = "Open issues badge", content_type = "image/svg+xml"))
)]
pub async fn open_issues(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    render::count_badge(&q, "open issues", issue_count(&owner, &repo, "open").await)
}

#[utoipa::path(
    get,
    path = "/codeberg/closed-issues/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "Codeberg",
    responses((status = 200, description = "Closed issues badge", content_type = "image/svg+xml"))
)]
pub async fn closed_issues(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    render::count_badge(
        &q,
        "closed issues",
        issue_count(&owner, &repo, "closed").await,
    )
}

#[utoipa::path(
    get,
    path = "/codeberg/stars/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "Codeberg",
    responses((status = 200, description = "Star count badge", content_type = "image/svg+xml"))
)]
pub async fn stars(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    let stars = get_json(&format!("{API}/repos/{owner}/{repo}"))
        .await
        .and_then(|v| v["stars_count"].as_u64());
    match stars {
        Some(n) => {
            let color = if n > 0 { "blue" } else { "yellow" };
            render::badge(&q, "stars", &n.to_string(), Some(color))
        }
        None => render::badge(&q, "stars", "None", Some("yellow")),
    }
}
