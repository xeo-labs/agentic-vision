//! JavaScript bundle analyzer for API endpoint discovery.
//!
//! Fetches JavaScript bundle files referenced in HTML `<script src="...">` tags
//! and analyzes them for API endpoints using regex-based pattern matching. This
//! is a best-effort enrichment layer — if bundles are too large or minified beyond
//! recognition, it degrades gracefully by returning an empty result set.

use crate::acquisition::action_discovery::{self, HttpAction};
use crate::acquisition::http_client::HttpClient;
use regex::Regex;

/// Maximum number of scripts to fetch and analyze (time budget cap).
const MAX_SCRIPTS: usize = 5;

/// Maximum script size in bytes (5 MB). Scripts larger than this are skipped.
const MAX_SCRIPT_SIZE: usize = 5 * 1024 * 1024;

/// Fetch JavaScript bundles referenced in HTML and analyze them for API endpoints.
///
/// Extracts `<script src="...">` URLs from the provided HTML, fetches same-origin
/// scripts via HTTP GET (capped at 5 scripts, skipping any over 5 MB), and runs
/// regex-based API endpoint discovery on each bundle. Results are deduplicated
/// by URL before returning.
pub async fn fetch_and_analyze_scripts(
    html: &str,
    base_url: &str,
    client: &HttpClient,
) -> Vec<HttpAction> {
    let script_urls = extract_script_urls(html, base_url);
    if script_urls.is_empty() {
        return Vec::new();
    }

    // Cap at MAX_SCRIPTS to stay within time budget
    let urls_to_fetch: Vec<String> = script_urls.into_iter().take(MAX_SCRIPTS).collect();

    // Fetch all scripts in parallel
    let responses = client.get_many(&urls_to_fetch, MAX_SCRIPTS, 10_000).await;

    let mut all_actions: Vec<HttpAction> = Vec::new();

    for resp in responses.into_iter().flatten() {
        // Skip non-200 responses
        if resp.status != 200 {
            continue;
        }

        // Skip scripts that exceed the size limit
        if resp.body.len() > MAX_SCRIPT_SIZE {
            continue;
        }

        let actions = action_discovery::discover_actions_from_js(&resp.body, base_url);
        all_actions.extend(actions);
    }

    // Deduplicate by label (which encodes method + path).
    all_actions.sort_by(|a, b| a.label.cmp(&b.label));
    all_actions.dedup_by(|a, b| a.label == b.label);

    all_actions
}

/// Extract all `<script src="...">` URLs from HTML and resolve them against the base URL.
///
/// Filters out:
/// - Scripts without a `src` attribute (inline scripts)
/// - Known analytics and tracking scripts (Google Analytics, GTM, Facebook, etc.)
/// - Known CDN library URLs (cdnjs, unpkg, jsdelivr)
/// - Cross-origin scripts (different domain than base URL)
pub fn extract_script_urls(html: &str, base_url: &str) -> Vec<String> {
    let re = Regex::new(r#"<script[^>]+src\s*=\s*["']([^"']+)["']"#).expect("valid regex");

    let mut urls = Vec::new();

    for cap in re.captures_iter(html) {
        if let Some(src) = cap.get(1) {
            let raw_url = src.as_str();

            // Resolve relative URLs
            let resolved = resolve_script_url(raw_url, base_url);

            // Skip analytics/CDN scripts
            if is_analytics_or_cdn(&resolved) {
                continue;
            }

            // Same-origin check
            if !is_same_origin(&resolved, base_url) {
                continue;
            }

            urls.push(resolved);
        }
    }

    urls
}

/// Check if a script URL is from the same origin as the base URL.
///
/// Compares the host (domain + port) of both URLs. Returns `false` if either
/// URL cannot be parsed.
fn is_same_origin(script_url: &str, base_url: &str) -> bool {
    let script_parsed = match url::Url::parse(script_url) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let base_parsed = match url::Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return false,
    };

    script_parsed.host_str() == base_parsed.host_str()
}

/// Check if a URL is a known analytics, tracking, or CDN script that should be skipped.
///
/// Matches against common third-party domains including Google Analytics,
/// Google Tag Manager, Facebook, Hotjar, Segment, cdnjs, unpkg, and jsdelivr.
fn is_analytics_or_cdn(url: &str) -> bool {
    const SKIP_PATTERNS: &[&str] = &[
        "google-analytics.com",
        "googletagmanager.com",
        "googlesyndication.com",
        "googleadservices.com",
        "google.com/recaptcha",
        "gstatic.com",
        "facebook.net",
        "facebook.com/tr",
        "fbcdn.net",
        "connect.facebook.net",
        "hotjar.com",
        "segment.com",
        "segment.io",
        "cdn.segment.com",
        "analytics.",
        "cdnjs.cloudflare.com",
        "unpkg.com",
        "cdn.jsdelivr.net",
        "ajax.googleapis.com",
        "stackpath.bootstrapcdn.com",
        "maxcdn.bootstrapcdn.com",
        "code.jquery.com",
        "newrelic.com",
        "nr-data.net",
        "sentry.io",
        "browser.sentry-cdn.com",
        "fullstory.com",
        "mixpanel.com",
        "heapanalytics.com",
        "clarity.ms",
        "doubleclick.net",
        "quantserve.com",
        "scorecardresearch.com",
        "optimizely.com",
        "crazyegg.com",
        "mouseflow.com",
        "tawk.to",
        "intercom.io",
        "intercomcdn.com",
        "crisp.chat",
        "drift.com",
        "zendesk.com",
    ];

    let lower = url.to_lowercase();
    SKIP_PATTERNS.iter().any(|pattern| lower.contains(pattern))
}

/// Resolve a script `src` attribute value against a base URL.
///
/// Handles absolute URLs (returned as-is), protocol-relative URLs, and
/// relative paths (resolved against the base URL origin).
fn resolve_script_url(src: &str, base_url: &str) -> String {
    // Already absolute
    if src.starts_with("http://") || src.starts_with("https://") {
        return src.to_string();
    }

    // Protocol-relative
    if src.starts_with("//") {
        return format!("https:{src}");
    }

    // Resolve relative URL against base
    if let Ok(base) = url::Url::parse(base_url) {
        if let Ok(resolved) = base.join(src) {
            return resolved.to_string();
        }
    }

    // Fallback: prepend origin
    let base_trimmed = base_url.trim_end_matches('/');
    if src.starts_with('/') {
        if let Ok(parsed) = url::Url::parse(base_trimmed) {
            return format!(
                "{}://{}{}",
                parsed.scheme(),
                parsed.host_str().unwrap_or(""),
                src
            );
        }
    }

    format!("{base_trimmed}/{src}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_script_urls_basic() {
        let html = r#"<html><head><script src="/js/main.js"></script><script src="/js/app.js"></script></head></html>"#;
        let urls = extract_script_urls(html, "https://example.com");
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://example.com/js/main.js".to_string()));
        assert!(urls.contains(&"https://example.com/js/app.js".to_string()));
    }

    #[test]
    fn test_extract_script_urls_filters_cdn() {
        let html = r#"<html><head>
            <script src="https://cdnjs.cloudflare.com/ajax/libs/jquery/3.6.0/jquery.min.js"></script>
            <script src="/js/app.js"></script>
            <script src="https://www.google-analytics.com/analytics.js"></script>
        </head></html>"#;
        let urls = extract_script_urls(html, "https://example.com");
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com/js/app.js");
    }

    #[test]
    fn test_is_same_origin() {
        assert!(is_same_origin(
            "https://example.com/js/app.js",
            "https://example.com/page"
        ));
        assert!(!is_same_origin(
            "https://cdn.example.com/js/app.js",
            "https://example.com/page"
        ));
        assert!(!is_same_origin(
            "https://other.com/js/app.js",
            "https://example.com/page"
        ));
    }

    #[test]
    fn test_is_analytics_or_cdn() {
        assert!(is_analytics_or_cdn(
            "https://www.google-analytics.com/analytics.js"
        ));
        assert!(is_analytics_or_cdn(
            "https://cdnjs.cloudflare.com/ajax/libs/react/18.0.0/react.min.js"
        ));
        assert!(is_analytics_or_cdn(
            "https://www.googletagmanager.com/gtm.js?id=GTM-XXX"
        ));
        assert!(!is_analytics_or_cdn("https://example.com/js/app.js"));
    }

    #[test]
    fn test_extract_empty_html() {
        let urls = extract_script_urls("", "https://example.com");
        assert!(urls.is_empty());
    }

    #[test]
    fn test_extract_inline_scripts_ignored() {
        let html =
            r#"<script>console.log("inline")</script><script src="/app.js"></script>"#;
        let urls = extract_script_urls(html, "https://example.com");
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com/app.js");
    }
}
