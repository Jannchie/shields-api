use axum::{Router, extract::Query, http::StatusCode, response::Response, routing::get};
use serde::{Deserialize, Serialize};
use shields::{BadgeStyle, builder::Badge};
use tokio;
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

fn create_badge_with_style(style: Option<&str>) -> shields::builder::BadgeBuilder {
    match style {
        Some("plastic") => Badge::style(BadgeStyle::Plastic),
        Some("flat-square") => Badge::style(BadgeStyle::FlatSquare),
        Some("social") => Badge::style(BadgeStyle::Social),
        Some("for-the-badge") => Badge::style(BadgeStyle::ForTheBadge),
        _ => Badge::style(BadgeStyle::Flat),
    }
}

impl ShieldsSchema {
    fn to_badge_svg(&self) -> String {
        let mut badge = create_badge_with_style(self.style.as_deref());

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

        badge.build().to_string()
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

        return Ok(Response::builder()
            .status(200)
            .header("content-type", "image/svg+xml")
            .body(svg_content)
            .unwrap());
    }

    // If not a Shields.io schema, report an error
    error!("Response is not a valid Shields.io schema format");
    Err(StatusCode::BAD_REQUEST)
}

async fn fetch_json_data(url: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    info!("Fetching data from URL: {}", url);
    let response = reqwest::get(url).await?;
    let status = response.status();
    info!("HTTP response status: {}", status);

    let json: serde_json::Value = response.json().await?;
    info!(
        "API response content: {}",
        serde_json::to_string_pretty(&json)?
    );
    Ok(json)
}

#[derive(OpenApi)]
#[openapi(
    paths(crate::endpoint_badge),
    components(schemas(EndpointParams)),
    info(
        title = "Shields API",
        version = "0.1.0",
        description = "API for generating shield badges compatible with shields.io"
    ),
    servers(
        (url = "http://localhost:1581", description = "Local development server"),
    )
)]
struct ApiDoc;

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

    let app = Router::new()
        .route("/endpoint", get(endpoint_badge))
        .route("/", get(|| async { "Shields API Server" }))
        .merge(Scalar::with_url("/docs", ApiDoc::openapi()));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:1581").await.unwrap();

    info!("Server running on http://0.0.0.0:1581");
    info!("API documentation available at http://0.0.0.0:1581/docs");
    axum::serve(listener, app).await.unwrap();
}
