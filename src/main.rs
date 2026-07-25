mod fetch;
mod render;
mod services;

use axum::{Json, Router, extract::Query, http::StatusCode, response::Response, routing::get};
use render::BadgeQuery;
use serde::{Deserialize, Serialize};
use services::{codeberg, crates_io, github, npm, pypi, statik};
use tracing::{debug, error, info};
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_scalar::{Scalar, Servable};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ShieldsSchema {
    /// Always the number 1
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    /// The left text, or the empty string to omit the left side of the badge
    label: String,
    /// Can't be empty. The right text
    message: String,
    /// Default: lightgrey. The right color
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    /// Default: grey. The left color
    #[serde(rename = "labelColor", skip_serializing_if = "Option::is_none")]
    label_color: Option<String>,
    /// Default: false. true to color the badge red unless `color` says otherwise
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
    /// One of the simple-icons slugs
    #[serde(rename = "namedLogo", skip_serializing_if = "Option::is_none")]
    named_logo: Option<String>,
    /// An SVG string containing a custom logo
    #[serde(rename = "logoSvg", skip_serializing_if = "Option::is_none")]
    logo_svg: Option<String>,
    /// Same meaning as the query string
    #[serde(rename = "logoColor", skip_serializing_if = "Option::is_none")]
    logo_color: Option<String>,
    /// Default: flat. The default template to use
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<String>,
}

impl ShieldsSchema {
    /// Render the badge this document describes. Query parameters win over the
    /// document, the same precedence every other badge route uses.
    fn to_badge_svg(&self, q: &BadgeQuery) -> String {
        let mut badge = render::builder_for_style(q.style.as_deref().or(self.style.as_deref()));

        badge
            .label(q.label.as_deref().unwrap_or(&self.label))
            .message(&self.message);

        // isError only decides the fallback: an explicit color still wins, which
        // is what shields.io does with an error badge that names its own color.
        let error_color = self.is_error.unwrap_or(false).then_some("red");
        if let Some(color) = q.color.as_deref().or(self.color.as_deref()).or(error_color) {
            badge.message_color(color);
        }

        if let Some(label_color) = q.label_color.as_deref().or(self.label_color.as_deref()) {
            badge.label_color(label_color);
        }

        // Priority: query logo > logoSvg > namedLogo. The shields crate detects
        // a leading "<svg" and embeds it as a base64 data URI, so custom markup
        // needs no special handling here.
        let logo = q
            .icon
            .as_deref()
            .or(self.logo_svg.as_deref())
            .or(self.named_logo.as_deref());
        if let Some(logo) = logo {
            badge.logo(logo);
        }

        if let Some(logo_color) = q.logo_color.as_deref().or(self.logo_color.as_deref()) {
            badge.logo_color(logo_color);
        }

        badge.build()
    }
}

#[derive(Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
struct EndpointParams {
    /// URL returning a shields.io endpoint JSON document
    url: String,
}

#[utoipa::path(
    get,
    path = "/endpoint",
    params(EndpointParams, BadgeQuery),
    tag = "Badge",
    responses(
        (status = 200, description = "Badge SVG", content_type = "image/svg+xml"),
        (status = 400, description = "Bad Request")
    )
)]
async fn endpoint_badge(
    Query(params): Query<EndpointParams>,
    Query(q): Query<BadgeQuery>,
) -> Result<Response<String>, StatusCode> {
    info!("Badge request - URL: {}, params: {:?}", params.url, q);

    let json_data = fetch::fetch_json(&params.url).await.map_err(|e| {
        error!("Failed to fetch JSON data: {}", e);
        StatusCode::BAD_REQUEST
    })?;
    // Debug level: the body is caller-controlled and would otherwise flood the
    // journal on every request.
    debug!("API response content: {}", json_data);

    let schema: ShieldsSchema = serde_json::from_value(json_data).map_err(|e| {
        error!("Response is not a valid Shields.io schema format: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    Ok(render::svg_response(
        schema.to_badge_svg(&q),
        &q.cache_control(),
    ))
}

#[derive(Serialize, ToSchema)]
struct DocsInfo {
    /// Interactive API documentation (Scalar UI)
    ui: &'static str,
    /// OpenAPI specification in JSON format
    openapi: &'static str,
}

#[derive(Serialize, ToSchema)]
struct ApiInfo {
    name: &'static str,
    /// API version
    version: &'static str,
    /// Version of the shields.rs crate used for badge rendering
    shields_version: &'static str,
    docs: DocsInfo,
}

#[utoipa::path(
    get,
    path = "/",
    tag = "Meta",
    responses((status = 200, description = "API version and documentation index", body = ApiInfo))
)]
async fn root() -> Json<ApiInfo> {
    Json(ApiInfo {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        shields_version: env!("SHIELDS_CRATE_VERSION"),
        docs: DocsInfo {
            ui: "/docs",
            openapi: "/openapi.json",
        },
    })
}

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::endpoint_badge,
        crate::root,
        statik::badge,
        statik::badge_with_color,
        github::release,
        github::issues,
        github::open_issues,
        github::closed_issues,
        github::checks,
        github::checks_branch,
        github::checks_specific,
        github::contributors,
        github::license,
        codeberg::release,
        codeberg::issues,
        codeberg::open_issues,
        codeberg::closed_issues,
        codeberg::stars,
        crates_io::name,
        crates_io::version,
        crates_io::info,
        crates_io::downloads,
        crates_io::downloads_latest,
        npm::name,
        npm::version,
        npm::license,
        npm::types,
        pypi::name,
        pypi::version,
        pypi::info,
        pypi::license,
    ),
    components(schemas(EndpointParams, ApiInfo, DocsInfo)),
    info(
        title = "Shields API",
        description = "API for generating shield badges compatible with shields.io"
    )
)]
struct ApiDoc;

/// Build the OpenAPI document with the server URL taken from the
/// BASE_URL environment variable (falls back to the local address).
fn build_openapi(port: u16) -> utoipa::openapi::OpenApi {
    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| format!("http://localhost:{port}"));
    let mut doc = ApiDoc::openapi();
    doc.servers = Some(vec![utoipa::openapi::Server::new(base_url)]);
    doc
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "shields_api=info,info".into()),
        )
        .init();

    info!("Starting Shields API Server");

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1581);
    let openapi = build_openapi(port);

    let app = Router::new()
        .route("/endpoint", get(endpoint_badge))
        .route("/", get(root))
        .route("/badge/{label}/{status}", get(statik::badge))
        .route(
            "/badge/{label}/{status}/{color}",
            get(statik::badge_with_color),
        )
        .route("/github/release/{owner}/{repo}", get(github::release))
        .route("/github/issues/{owner}/{repo}", get(github::issues))
        .route(
            "/github/open-issues/{owner}/{repo}",
            get(github::open_issues),
        )
        .route(
            "/github/closed-issues/{owner}/{repo}",
            get(github::closed_issues),
        )
        .route("/github/checks/{owner}/{repo}", get(github::checks))
        .route(
            "/github/checks/{owner}/{repo}/{branch}",
            get(github::checks_branch),
        )
        .route(
            "/github/checks/{owner}/{repo}/{branch}/{check}",
            get(github::checks_specific),
        )
        .route(
            "/github/contributors/{owner}/{repo}",
            get(github::contributors),
        )
        .route("/github/license/{owner}/{repo}", get(github::license))
        .route("/codeberg/release/{owner}/{repo}", get(codeberg::release))
        .route("/codeberg/issues/{owner}/{repo}", get(codeberg::issues))
        .route(
            "/codeberg/open-issues/{owner}/{repo}",
            get(codeberg::open_issues),
        )
        .route(
            "/codeberg/closed-issues/{owner}/{repo}",
            get(codeberg::closed_issues),
        )
        .route("/codeberg/stars/{owner}/{repo}", get(codeberg::stars))
        .route("/crates/name/{crate}", get(crates_io::name))
        .route("/crates/version/{crate}", get(crates_io::version))
        .route("/crates/info/{crate}", get(crates_io::info))
        .route("/crates/downloads/{crate}", get(crates_io::downloads))
        .route(
            "/crates/downloads/{crate}/latest",
            get(crates_io::downloads_latest),
        )
        .route("/npm/name/{*pkg}", get(npm::name))
        .route("/npm/version/{*pkg}", get(npm::version))
        .route("/npm/license/{*pkg}", get(npm::license))
        .route("/npm/types/{*pkg}", get(npm::types))
        .route("/pypi/name/{pkg}", get(pypi::name))
        .route("/pypi/version/{pkg}", get(pypi::version))
        .route("/pypi/info/{pkg}", get(pypi::info))
        .route("/pypi/license/{pkg}", get(pypi::license))
        .route("/openapi.json", {
            let doc = openapi.clone();
            get(move || async move { Json(doc) })
        })
        .merge(Scalar::with_url("/docs", openapi));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap();

    info!("Server running on http://0.0.0.0:{port}");
    info!("API documentation available at http://0.0.0.0:{port}/docs");
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(json: serde_json::Value) -> ShieldsSchema {
        serde_json::from_value(json).unwrap()
    }

    fn minimal() -> ShieldsSchema {
        schema(serde_json::json!({
            "schemaVersion": 1, "label": "build", "message": "passing"
        }))
    }

    #[test]
    fn query_overrides_the_document() {
        let doc = schema(serde_json::json!({
            "schemaVersion": 1, "label": "build", "message": "passing", "color": "green"
        }));
        let q = BadgeQuery {
            label: Some("ci".into()),
            color: Some("blue".into()),
            ..Default::default()
        };
        let svg = doc.to_badge_svg(&q);
        assert!(svg.contains("ci"), "query label should win");
        assert!(!svg.contains("build"), "document label should be replaced");
    }

    #[test]
    fn document_is_used_when_the_query_is_silent() {
        let svg = minimal().to_badge_svg(&BadgeQuery::default());
        assert!(svg.contains("build") && svg.contains("passing"));
    }

    #[test]
    fn is_error_colors_the_badge_red() {
        let err = schema(serde_json::json!({
            "schemaVersion": 1, "label": "build", "message": "failing", "isError": true
        }));
        let error_svg = err.to_badge_svg(&BadgeQuery::default());
        let plain_svg = minimal().to_badge_svg(&BadgeQuery::default());
        assert_ne!(
            error_svg.contains("#e05d44"),
            plain_svg.contains("#e05d44"),
            "isError should change the message color"
        );
        assert!(error_svg.contains("#e05d44"), "isError should render red");
    }

    #[test]
    fn an_explicit_color_beats_is_error() {
        let doc = schema(serde_json::json!({
            "schemaVersion": 1, "label": "build", "message": "failing",
            "isError": true, "color": "blue"
        }));
        assert!(!doc.to_badge_svg(&BadgeQuery::default()).contains("#e05d44"));
    }

    /// The document may carry fields this service does not render (logoSize was
    /// one). Unknown keys must not turn a usable document into a 400.
    #[test]
    fn unknown_document_fields_are_ignored() {
        let doc = schema(serde_json::json!({
            "schemaVersion": 1, "label": "build", "message": "passing",
            "logoSize": "auto", "cacheSeconds": 3600
        }));
        assert!(doc.to_badge_svg(&BadgeQuery::default()).contains("passing"));
    }

    /// A caller-supplied logoSvg used to be sliced at byte 50 for a log line,
    /// which panicked when byte 50 landed inside a character.
    #[test]
    fn multibyte_logo_svg_does_not_panic() {
        let doc = schema(serde_json::json!({
            "schemaVersion": 1, "label": "build", "message": "passing",
            "logoSvg": "x".repeat(48) + "日本語のロゴ"
        }));
        doc.to_badge_svg(&BadgeQuery::default());
    }
}
