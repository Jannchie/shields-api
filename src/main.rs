mod render;
mod services;

use axum::{Json, Router, extract::Query, http::StatusCode, response::Response, routing::get};
use services::{codeberg, crates_io, github, npm, pypi, statik};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
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
    /// Default: false. true to treat this as an error badge
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
    /// Make icons adaptively resize by setting auto
    #[serde(rename = "logoSize", skip_serializing_if = "Option::is_none")]
    logo_size: Option<String>,
    /// Default: flat. The default template to use
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<String>,
}

impl ShieldsSchema {
    fn to_badge_svg(&self) -> String {
        let mut badge = render::builder_for_style(self.style.as_deref());

        badge.label(&self.label).message(&self.message);

        if let Some(label_color) = &self.label_color {
            badge.label_color(label_color);
        }

        if let Some(message_color) = &self.color {
            badge.message_color(message_color);
        }

        // Priority: logoSvg > namedLogo
        if let Some(logo_svg) = &self.logo_svg {
            // For custom SVG, we need to handle it differently
            // The shields crate might not support custom SVG directly in the builder
            // We'll pass it as logo parameter anyway and let the library handle it
            badge.logo(logo_svg);
        } else if let Some(named_logo) = &self.named_logo {
            badge.logo(named_logo);
        }

        if let Some(logo_color) = &self.logo_color {
            badge.logo_color(logo_color);
        }

        badge.build()
    }
}

#[derive(Deserialize, ToSchema, IntoParams)]
struct EndpointParams {
    /// API endpoint URL that returns JSON data
    url: String,
    /// JSONPath query to extract value from response (e.g., "version" or "data.count")
    query: Option<String>,
    /// Text to show on the left side of the badge
    label: Option<String>,
    /// Badge color (red, green, blue, yellow, orange, lightgrey, or hex color)
    color: Option<String>,
    /// Badge style (flat, plastic, flat-square, social, for-the-badge)
    style: Option<String>,
}

#[utoipa::path(
    get,
    path = "/endpoint",
    params(EndpointParams),
    tag = "Badge",
    responses(
        (status = 200, description = "Badge SVG", content_type = "image/svg+xml"),
        (status = 400, description = "Bad Request")
    )
)]
async fn endpoint_badge(
    Query(params): Query<EndpointParams>,
) -> Result<Response<String>, StatusCode> {
    info!(
        "Badge request - URL: {}, Query: {:?}, Label: {:?}, Color: {:?}, Style: {:?}",
        params.url, params.query, params.label, params.color, params.style
    );

    let json_data = fetch_json_data(&params.url).await.map_err(|e| {
        error!("Failed to fetch JSON data: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    // Check if the response is already a Shields.io schema
    if let Ok(shields_schema) = serde_json::from_value::<ShieldsSchema>(json_data.clone()) {
        info!("Response is already Shields.io schema format");

        // Apply query parameters to override schema values
        let mut final_schema = shields_schema;
        if let Some(label) = &params.label {
            final_schema.label = label.clone();
        }
        if let Some(color) = &params.color {
            final_schema.color = Some(color.clone());
        }
        if let Some(style) = &params.style {
            final_schema.style = Some(style.clone());
        }

        info!(
            "Generating badge from schema - Label: '{}', Message: '{}', Color: '{:?}', Style: '{:?}', NamedLogo: '{:?}', LogoSvg: '{:?}'",
            final_schema.label,
            final_schema.message,
            final_schema.color,
            final_schema.style,
            final_schema.named_logo,
            final_schema
                .logo_svg
                .as_deref()
                .map(|s| &s[..50.min(s.len())])
        );

        let svg_content = final_schema.to_badge_svg();

        info!("Badge generated successfully");

        return Ok(render::svg_response(
            svg_content,
            "no-cache, no-store, must-revalidate",
        ));
    }

    // If not a Shields.io schema, report an error
    error!("Response is not a valid Shields.io schema format");
    Err(StatusCode::BAD_REQUEST)
}

async fn fetch_json_data(url: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    info!("Fetching data from URL: {}", url);
    let response = services::http().get(url).send().await?;
    let status = response.status();
    info!("HTTP response status: {}", status);

    let json: serde_json::Value = response.json().await?;
    info!(
        "API response content: {}",
        serde_json::to_string_pretty(&json)?
    );
    Ok(json)
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
    let base_url =
        std::env::var("BASE_URL").unwrap_or_else(|_| format!("http://localhost:{port}"));
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
        .route("/badge/{label}/{status}/{color}", get(statik::badge_with_color))
        .route("/github/release/{owner}/{repo}", get(github::release))
        .route("/github/issues/{owner}/{repo}", get(github::issues))
        .route("/github/open-issues/{owner}/{repo}", get(github::open_issues))
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
