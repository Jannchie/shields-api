//! Hardened fetching for the caller-supplied `/endpoint` URL.
//!
//! `/endpoint` accepts an arbitrary URL, the same way shields.io's endpoint
//! badge does — that part is the feature. But it also means this service makes
//! HTTP requests on behalf of anyone who can reach it, so this path is stricter
//! than [`crate::services::http`]:
//!
//! - only `http` and `https`
//! - loopback, private, link-local and other non-public addresses are refused,
//!   checked both for literal-IP URLs and again after DNS resolution, so a
//!   hostname that resolves to 127.0.0.1 is caught too
//! - redirects are not followed, since the redirect target would otherwise skip
//!   both checks above
//! - the response body is capped
//!
//! None of this is configurable. These are safety defaults, not deployment
//! knobs: an environment variable here would only be useful for turning them
//! off. The service integrations in `services::*` keep using the plain client,
//! since they only ever talk to their own hardcoded upstreams.

use std::fmt;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, LazyLock};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::{Url, redirect};

use crate::services::client_builder;

/// Upper bound on the response body. A shields.io endpoint document is a few
/// hundred bytes, so this is generous while still bounding per-request memory.
const MAX_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum FetchError {
    InvalidUrl,
    UnsupportedScheme,
    MissingHost,
    /// The URL names a non-public address directly. A *hostname* that resolves
    /// to one is refused inside [`PublicOnlyResolver`] instead, and surfaces as
    /// [`FetchError::Upstream`] because hyper wraps the resolver's error.
    BlockedAddress,
    TooLarge,
    Upstream(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl => f.write_str("could not parse the URL"),
            Self::UnsupportedScheme => f.write_str("only http and https URLs are supported"),
            Self::MissingHost => f.write_str("the URL has no host"),
            Self::BlockedAddress => {
                f.write_str("the URL points at a non-public address, which is not allowed")
            }
            Self::TooLarge => write!(f, "the response exceeded {MAX_BODY_BYTES} bytes"),
            Self::Upstream(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for FetchError {}

/// Whether `ip` is outside the public internet and must not be fetched.
fn is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local() // 169.254.0.0/16, where cloud metadata lives
                || v4.is_documentation()
                || a == 0 // 0.0.0.0/8, "this network", which includes 0.0.0.0
                || a >= 224 // 224.0.0.0/3: multicast, reserved, and broadcast
                || (a == 100 && (64..128).contains(&b)) // 100.64.0.0/10, carrier-grade NAT
        }
        IpAddr::V6(v6) => {
            // ::ffff:127.0.0.1 is loopback, but Ipv6Addr::is_loopback() says no,
            // so mapped addresses have to be judged in their IPv4 form.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked(IpAddr::V4(v4));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || v6.is_unique_local() // fc00::/7
                || v6.is_unicast_link_local() // fe80::/10
        }
    }
}

/// A resolver that drops non-public addresses from every DNS answer.
///
/// Checking the hostname before connecting would not be enough on its own: the
/// name could resolve to a public address at check time and a private one at
/// connect time (DNS rebinding). Filtering inside the resolver means the
/// addresses the connector actually dials are the ones that were vetted.
struct PublicOnlyResolver;

impl Resolve for PublicOnlyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            // Port 0 is a placeholder; reqwest overwrites it with the real port.
            // Collected rather than left lazy: the iterator lookup_host returns
            // borrows `name`, while `Addrs` has to be 'static.
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .filter(|addr| !is_blocked(addr.ip()))
                .collect();

            // hyper wraps whatever is returned here into its own connect error,
            // so this text is the only trace of why the connection never began.
            if addrs.is_empty() {
                return Err(Box::new(io::Error::other(
                    "every address for this host is non-public",
                )) as _);
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

fn client() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        client_builder()
            .redirect(redirect::Policy::none())
            .dns_resolver(Arc::new(PublicOnlyResolver))
            .build()
            .expect("failed to build guarded HTTP client")
    });
    &CLIENT
}

/// Reject a URL that names a non-public address directly. Literal IPs never
/// reach [`PublicOnlyResolver`], because there is nothing to resolve.
fn check_literal_host(url: &Url) -> Result<(), FetchError> {
    let host = url.host_str().ok_or(FetchError::MissingHost)?;
    // host_str keeps the brackets around IPv6 literals, and IpAddr can't parse
    // those. Brackets are not legal anywhere else in a host, so this is safe.
    let host = host.trim_matches(['[', ']']);

    match host.parse::<IpAddr>() {
        Ok(ip) if is_blocked(ip) => Err(FetchError::BlockedAddress),
        // Not an IP literal: PublicOnlyResolver vets it at connect time.
        _ => Ok(()),
    }
}

/// Fetch and parse JSON from a caller-supplied URL, enforcing the guards above.
pub async fn fetch_json(raw_url: &str) -> Result<serde_json::Value, FetchError> {
    let url: Url = raw_url.parse().map_err(|_| FetchError::InvalidUrl)?;

    if !matches!(url.scheme(), "http" | "https") {
        return Err(FetchError::UnsupportedScheme);
    }
    check_literal_host(&url)?;

    let mut resp = client()
        .get(url)
        .send()
        .await
        .map_err(|e| FetchError::Upstream(format!("request failed: {e}")))?;

    // With redirects disabled a 3xx arrives as-is, and is rejected here.
    if !resp.status().is_success() {
        return Err(FetchError::Upstream(format!(
            "upstream returned {}",
            resp.status()
        )));
    }

    // Streamed rather than buffered whole, so an oversized body is abandoned
    // partway instead of being read into memory first. Content-Length is not
    // consulted: it can disagree with what the server actually sends.
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| FetchError::Upstream(format!("read failed: {e}")))?
    {
        if body.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(FetchError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }

    serde_json::from_slice(&body).map_err(|e| FetchError::Upstream(format!("invalid JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(s: &str) -> bool {
        is_blocked(s.parse().unwrap())
    }

    /// Whether the pre-flight literal-host check refuses `url`.
    fn literal_blocked(url: &str) -> bool {
        let parsed: Url = url.parse().unwrap();
        matches!(check_literal_host(&parsed), Err(FetchError::BlockedAddress))
    }

    #[test]
    fn blocks_non_public_v4() {
        for ip in [
            "127.0.0.1",
            "127.1.2.3",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "0.0.0.0",
            "100.64.0.1", // CGNAT
            "255.255.255.255",
            "240.0.0.1",
        ] {
            assert!(blocked(ip), "{ip} should be blocked");
        }
    }

    #[test]
    fn allows_public_v4() {
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "140.82.121.4",
            "172.32.0.1",
            "100.128.0.1",
        ] {
            assert!(!blocked(ip), "{ip} should be allowed");
        }
    }

    #[test]
    fn blocks_non_public_v6() {
        for ip in ["::1", "::", "fc00::1", "fd12:3456::1", "fe80::1", "ff02::1"] {
            assert!(blocked(ip), "{ip} should be blocked");
        }
    }

    #[test]
    fn allows_public_v6() {
        for ip in ["2606:4700:4700::1111", "2001:4860:4860::8888"] {
            assert!(!blocked(ip), "{ip} should be allowed");
        }
    }

    #[test]
    fn blocks_v4_mapped_loopback() {
        // The form that slips through a naive Ipv6Addr::is_loopback() check.
        assert!(blocked("::ffff:127.0.0.1"));
        assert!(blocked("::ffff:10.0.0.1"));
        assert!(!blocked("::ffff:8.8.8.8"));
    }

    #[test]
    fn rejects_literal_loopback_urls() {
        for url in [
            "http://127.0.0.1:5432/",
            "http://[::1]:9090/metrics",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::ffff:127.0.0.1]/",
        ] {
            assert!(literal_blocked(url), "{url} should be blocked");
        }
    }

    #[test]
    fn accepts_public_urls() {
        for url in ["https://api.github.com/repos/a/b", "http://8.8.8.8/x"] {
            assert!(!literal_blocked(url), "{url} should be allowed");
        }
    }

    #[tokio::test]
    async fn rejects_non_http_schemes() {
        assert!(matches!(
            fetch_json("file:///etc/passwd").await,
            Err(FetchError::UnsupportedScheme)
        ));
    }

    #[tokio::test]
    async fn rejects_loopback_before_connecting() {
        assert!(matches!(
            fetch_json("http://127.0.0.1:1/").await,
            Err(FetchError::BlockedAddress)
        ));
    }
}
