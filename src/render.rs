//! Shared badge rendering for the spacebadgers-compatible endpoints.

use axum::http::Response;
use serde::Deserialize;
use shields::{BadgeStyle, builder::Badge};
use utoipa::IntoParams;

pub const DEFAULT_CACHE_SECONDS: u32 = 300;

/// Query parameters accepted by every badge endpoint, mirroring the
/// spacebadgers worker. Unknown parameters (scale, corner_radius, ...) are
/// silently ignored.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BadgeQuery {
    /// Override the badge label (left side text)
    pub label: Option<String>,
    /// Status color: shields.io named color, hex, or CSS color
    pub color: Option<String>,
    /// Label color: shields.io named color, hex, or CSS color
    #[serde(alias = "labelColor")]
    pub label_color: Option<String>,
    /// Badge style: flat (default), plastic, flat-square, social, for-the-badge
    pub style: Option<String>,
    /// Logo: one of the simple-icons slugs
    #[serde(alias = "logo")]
    pub icon: Option<String>,
    /// Logo color
    #[serde(alias = "logoColor", alias = "iconColor", alias = "icon_color")]
    pub logo_color: Option<String>,
    /// Cache duration in seconds (default: 300)
    pub cache: Option<String>,
}

impl BadgeQuery {
    fn cache_seconds(&self) -> u32 {
        self.cache
            .as_deref()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CACHE_SECONDS)
    }
}

/// Map a spacebadgers/shields.io style string to a badge builder.
pub fn builder_for_style(style: Option<&str>) -> shields::builder::BadgeBuilder<'static> {
    match style {
        Some("plastic") => Badge::style(BadgeStyle::Plastic),
        Some("flat-square") => Badge::style(BadgeStyle::FlatSquare),
        Some("social") => Badge::style(BadgeStyle::Social),
        Some("for-the-badge") => Badge::style(BadgeStyle::ForTheBadge),
        _ => Badge::style(BadgeStyle::Flat),
    }
}

pub fn svg_response(svg: String, cache_control: &str) -> Response<String> {
    Response::builder()
        .status(200)
        .header("content-type", "image/svg+xml")
        .header("cache-control", cache_control)
        .body(svg)
        .unwrap()
}

/// Render a badge. `default_label`, `status` and `default_color` come from the
/// endpoint; query parameters override them, matching spacebadgers behavior.
/// Color strings are passed through as-is: shields.rs resolves named colors,
/// aliases, hex, and CSS colors itself.
pub fn badge(
    q: &BadgeQuery,
    default_label: &str,
    status: &str,
    default_color: Option<&str>,
) -> Response<String> {
    let label = q.label.as_deref().unwrap_or(default_label);

    let mut builder = builder_for_style(q.style.as_deref());
    builder.label(label).message(status);

    if let Some(color) = q.color.as_deref().or(default_color) {
        builder.message_color(color);
    }
    if let Some(label_color) = &q.label_color {
        builder.label_color(label_color);
    }
    if let Some(icon) = &q.icon {
        builder.logo(icon);
    }
    if let Some(logo_color) = &q.logo_color {
        builder.logo_color(logo_color);
    }

    svg_response(
        builder.build(),
        &format!(
            "public, max-age={}, no-transform, must-revalidate",
            q.cache_seconds()
        ),
    )
}

/// Badge for count endpoints, with spacebadgers' `None` fallback when the
/// upstream count is unavailable.
pub fn count_badge(q: &BadgeQuery, label: &str, count: Option<u64>) -> Response<String> {
    match count {
        Some(n) => badge(q, label, &n.to_string(), None),
        None => badge(q, label, "None", None),
    }
}

/// Badge for plain text endpoints, falling back to the `<subsystem> | error`
/// badge when the upstream value is unavailable.
pub fn text_badge(
    q: &BadgeQuery,
    label: &str,
    subsystem: &str,
    value: Option<String>,
) -> Response<String> {
    match value {
        Some(value) => badge(q, label, &value, None),
        None => error_badge(q, subsystem),
    }
}

/// Release badge with spacebadgers' `None` / yellow fallback when the upstream
/// has no release.
pub fn release_badge(q: &BadgeQuery, name: Option<String>) -> Response<String> {
    match name {
        Some(name) => badge(q, "release", &name, Some("blue")),
        None => badge(q, "release", "None", Some("yellow")),
    }
}

/// License badge with the shared `unknown` / lightgrey fallback. `color` is
/// the status color when a license is found (services differ here).
pub fn license_badge(
    q: &BadgeQuery,
    license: Option<String>,
    color: Option<&str>,
) -> Response<String> {
    match license {
        Some(license) => badge(q, "license", &license, color),
        None => badge(q, "license", "unknown", Some("lightgrey")),
    }
}

/// The gray `<subsystem> | error` badge spacebadgers serves on upstream failures.
/// Uses lightgrey (the shields.io "inactive" color) so the status side stays
/// distinguishable from the dark label side.
pub fn error_badge(q: &BadgeQuery, subsystem: &str) -> Response<String> {
    badge(q, subsystem, "error", Some("lightgrey"))
}

/// Format a number like `Intl.NumberFormat('en-US', { notation: 'compact',
/// maximumFractionDigits: 1 })`, which spacebadgers uses for download counts.
pub fn compact_number(n: u64) -> String {
    const UNITS: [(f64, &str); 4] = [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")];
    for (div, suffix) in UNITS {
        if (n as f64) >= div {
            let value = format!("{:.1}", (n as f64 / div * 10.0).round() / 10.0);
            return format!("{}{suffix}", value.strip_suffix(".0").unwrap_or(&value));
        }
    }
    n.to_string()
}

#[cfg(test)]
mod tests {
    use super::compact_number;

    #[test]
    fn compact_numbers() {
        assert_eq!(compact_number(0), "0");
        assert_eq!(compact_number(999), "999");
        assert_eq!(compact_number(1234), "1.2K");
        assert_eq!(compact_number(12345), "12.3K");
        assert_eq!(compact_number(1_000_000), "1M");
        assert_eq!(compact_number(1_234_567), "1.2M");
        assert_eq!(compact_number(9_876_543_210), "9.9B");
    }
}
