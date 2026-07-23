//! GitHub badge endpoints, mirroring badgers.space/github/* semantics.

use std::sync::LazyLock;

use axum::extract::{Path, Query};
use axum::http::Response;
use serde_json::Value;

use super::{OwnerRepo, http};
use crate::render::{self, BadgeQuery};

const API: &str = "https://api.github.com";

fn request(url: &str) -> reqwest::RequestBuilder {
    static TOKEN: LazyLock<Option<String>> =
        LazyLock::new(|| std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()));
    let mut rb = http()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(token) = TOKEN.as_deref() {
        rb = rb.bearer_auth(token);
    }
    rb
}

async fn get_json(url: &str) -> Option<Value> {
    super::get_json(request(url)).await
}

#[utoipa::path(
    get,
    path = "/github/release/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "GitHub",
    responses((status = 200, description = "Latest release badge", content_type = "image/svg+xml"))
)]
pub async fn release(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    let tag = get_json(&format!("{API}/repos/{owner}/{repo}/releases/latest"))
        .await
        .and_then(|v| v["tag_name"].as_str().map(str::to_string));
    render::release_badge(&q, tag)
}

/// Count issues (excluding pull requests) via the search API, which returns
/// exact totals in a single request.
async fn issue_count(owner: &str, repo: &str, state: Option<&str>) -> Option<u64> {
    let mut query = format!("repo:{owner}/{repo}+type:issue");
    if let Some(state) = state {
        query.push_str(&format!("+state:{state}"));
    }
    get_json(&format!("{API}/search/issues?q={query}&per_page=1"))
        .await?["total_count"]
        .as_u64()
}

#[utoipa::path(
    get,
    path = "/github/issues/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "GitHub",
    responses((status = 200, description = "Total issues badge", content_type = "image/svg+xml"))
)]
pub async fn issues(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    render::count_badge(&q, "issues", issue_count(&owner, &repo, None).await)
}

#[utoipa::path(
    get,
    path = "/github/open-issues/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "GitHub",
    responses((status = 200, description = "Open issues badge", content_type = "image/svg+xml"))
)]
pub async fn open_issues(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    render::count_badge(&q, "open issues", issue_count(&owner, &repo, Some("open")).await)
}

#[utoipa::path(
    get,
    path = "/github/closed-issues/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "GitHub",
    responses((status = 200, description = "Closed issues badge", content_type = "image/svg+xml"))
)]
pub async fn closed_issues(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    render::count_badge(
        &q,
        "closed issues",
        issue_count(&owner, &repo, Some("closed")).await,
    )
}

/// Combine check-run conclusions the way spacebadgers does: any hard failure
/// wins, neutral/cancelled/skipped are ignored, everything-success is success,
/// anything else (including in-progress runs) is unknown.
fn combined_conclusion(conclusions: &[Option<&str>]) -> (&'static str, &'static str) {
    for bad in ["failure", "timed_out", "action_required"] {
        if conclusions.contains(&Some(bad)) {
            return (bad, "red");
        }
    }
    let ignored = ["neutral", "cancelled", "skipped"];
    let all_success = conclusions
        .iter()
        .filter(|c| !matches!(c, Some(x) if ignored.contains(x)))
        .all(|c| *c == Some("success"));
    if all_success {
        ("success", "green")
    } else {
        ("unknown", "lightgrey")
    }
}

async fn checks_badge(
    owner: &str,
    repo: &str,
    branch: &str,
    check: Option<&str>,
    q: BadgeQuery,
) -> Response<String> {
    let json = get_json(&format!(
        "{API}/repos/{owner}/{repo}/commits/{branch}/check-runs?per_page=100"
    ))
    .await;
    let Some(runs) = json.as_ref().and_then(|j| j["check_runs"].as_array()) else {
        return render::error_badge(&q, "github");
    };
    let conclusions: Vec<Option<&str>> = runs
        .iter()
        .filter(|run| {
            check.is_none_or(|c| {
                run["name"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(c))
            })
        })
        .map(|run| run["conclusion"].as_str())
        .collect();
    if check.is_some() && conclusions.is_empty() {
        return render::error_badge(&q, "github");
    }
    let (status, color) = combined_conclusion(&conclusions);
    render::badge(&q, check.unwrap_or("checks"), status, Some(color))
}

#[utoipa::path(
    get,
    path = "/github/checks/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "GitHub",
    responses((status = 200, description = "Combined checks badge for the default branch", content_type = "image/svg+xml"))
)]
pub async fn checks(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    // HEAD resolves to the default branch, saving the extra repo lookup.
    checks_badge(&owner, &repo, "HEAD", None, q).await
}

#[utoipa::path(
    get,
    path = "/github/checks/{owner}/{repo}/{branch}",
    params(
        ("owner" = String, Path, description = "Repository owner"),
        ("repo" = String, Path, description = "Repository name"),
        ("branch" = String, Path, description = "Branch name"),
        BadgeQuery,
    ),
    tag = "GitHub",
    responses((status = 200, description = "Combined checks badge for a branch", content_type = "image/svg+xml"))
)]
pub async fn checks_branch(
    Path((owner, repo, branch)): Path<(String, String, String)>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    checks_badge(&owner, &repo, &branch, None, q).await
}

#[utoipa::path(
    get,
    path = "/github/checks/{owner}/{repo}/{branch}/{check}",
    params(
        ("owner" = String, Path, description = "Repository owner"),
        ("repo" = String, Path, description = "Repository name"),
        ("branch" = String, Path, description = "Branch name"),
        ("check" = String, Path, description = "Check name (case-insensitive)"),
        BadgeQuery,
    ),
    tag = "GitHub",
    responses((status = 200, description = "Status badge for a specific check", content_type = "image/svg+xml"))
)]
pub async fn checks_specific(
    Path((owner, repo, branch, check)): Path<(String, String, String, String)>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    checks_badge(&owner, &repo, &branch, Some(&check), q).await
}

#[utoipa::path(
    get,
    path = "/github/contributors/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "GitHub",
    responses((status = 200, description = "Contributor count badge", content_type = "image/svg+xml"))
)]
pub async fn contributors(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    // per_page=1 + the rel="last" page number in the Link header gives the
    // exact contributor count without paging through the whole list.
    let resp = request(&format!("{API}/repos/{owner}/{repo}/contributors?per_page=1"))
        .send()
        .await
        .ok()
        .filter(|r| r.status().is_success());
    let Some(resp) = resp else {
        return render::error_badge(&q, "github");
    };
    let last_page = last_page_from_link_header(
        resp.headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default(),
    );
    let count = match last_page {
        Some(n) => n,
        // No Link header: the whole list fits on one page.
        None => match resp.json::<Vec<Value>>().await {
            Ok(items) => items.len() as u64,
            Err(_) => return render::error_badge(&q, "github"),
        },
    };
    render::badge(&q, "contributors", &count.to_string(), None)
}

fn last_page_from_link_header(link: &str) -> Option<u64> {
    link.split(',')
        .find(|part| part.contains("rel=\"last\""))
        .and_then(|part| {
            let url = part.split('<').nth(1)?.split('>').next()?;
            url.split(['?', '&'])
                .find_map(|kv| kv.strip_prefix("page="))
                .and_then(|v| v.parse().ok())
        })
}

#[utoipa::path(
    get,
    path = "/github/license/{owner}/{repo}",
    params(OwnerRepo, BadgeQuery),
    tag = "GitHub",
    responses((status = 200, description = "License badge", content_type = "image/svg+xml"))
)]
pub async fn license(
    Path(OwnerRepo { owner, repo }): Path<OwnerRepo>,
    Query(q): Query<BadgeQuery>,
) -> Response<String> {
    let name = get_json(&format!("{API}/repos/{owner}/{repo}/license"))
        .await
        .and_then(|v| {
            v["license"]["spdx_id"]
                .as_str()
                .filter(|s| *s != "NOASSERTION")
                .or_else(|| v["license"]["name"].as_str())
                .map(str::to_string)
        });
    render::license_badge(&q, name, Some("blue"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_header_parsing() {
        let link = r#"<https://api.github.com/repositories/1/contributors?per_page=1&page=2>; rel="next", <https://api.github.com/repositories/1/contributors?per_page=1&page=113>; rel="last""#;
        assert_eq!(last_page_from_link_header(link), Some(113));
        assert_eq!(last_page_from_link_header(""), None);
    }

    #[test]
    fn combined_conclusions() {
        let c = combined_conclusion;
        assert_eq!(c(&[Some("success"), Some("failure")]), ("failure", "red"));
        assert_eq!(c(&[Some("success"), Some("skipped")]), ("success", "green"));
        assert_eq!(c(&[Some("success"), None]), ("unknown", "lightgrey"));
        assert_eq!(c(&[]), ("success", "green"));
    }
}
