//! Discovers executable HTTP actions from HTML forms, JavaScript patterns,
//! and known e-commerce platforms.
//!
//! This module complements the pattern engine by going deeper into action
//! discovery: instead of just classifying buttons by label, it extracts the
//! full HTTP-level details (URL, method, fields, body templates) needed to
//! *execute* the action without a browser.
//!
//! Three discovery strategies are layered:
//!
//! 1. **HTML form parsing** — walks every `<form>` tag, resolves action URLs
//!    against the base, and extracts all fields including hidden CSRF tokens.
//! 2. **JavaScript API scanning** — regex-scans JS source for `fetch()`,
//!    `axios`, `$.ajax`, and bare `/api/` string literals.
//! 3. **Platform detection** — recognises Shopify, WooCommerce, Magento, and
//!    BigCommerce from fingerprints in the HTML/JS and loads pre-built action
//!    templates from an embedded JSON file.
//!
//! All public entry points are **synchronous**. Callers should wrap in
//! `tokio::task::spawn_blocking` when integrating with the async runtime.

use crate::map::types::OpCode;
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

// ── Compile-time platform configuration ─────────────────────────────────────

/// Raw JSON content of the platform action templates, embedded at compile time.
const PLATFORM_ACTIONS_JSON: &str = include_str!("platform_actions.json");

// ── Public types ────────────────────────────────────────────────────────────

/// A single field inside an HTML form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    /// The `name` attribute of the field.
    pub name: String,
    /// The `type` attribute (e.g., `"text"`, `"hidden"`, `"email"`).
    pub field_type: String,
    /// Pre-filled `value` attribute, if any.
    pub value: Option<String>,
    /// Whether the field has the `required` attribute.
    pub required: bool,
}

/// An action that can be executed via HTTP (no browser needed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpAction {
    /// OpCode `(category, action)` for the binary map spec.
    pub opcode: OpCode,
    /// Human-readable label (e.g., "Add to Cart", "Login").
    pub label: String,
    /// Where this action was discovered.
    pub source: ActionSource,
    /// Confidence that this action is correctly identified, in `[0.0, 1.0]`.
    pub confidence: f32,
}

/// Where an [`HttpAction`] was discovered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionSource {
    /// Discovered from an HTML `<form>` element.
    Form {
        /// Resolved form action URL.
        action_url: String,
        /// HTTP method (`GET` or `POST`).
        method: String,
        /// Content type (e.g., `application/x-www-form-urlencoded`).
        content_type: String,
        /// All fields inside the form.
        fields: Vec<FormField>,
    },
    /// Discovered from JavaScript API calls.
    Api {
        /// The API endpoint URL.
        url: String,
        /// HTTP method (`GET`, `POST`, `PUT`, `DELETE`).
        method: String,
        /// Optional body template extracted from JS source.
        body_template: Option<String>,
    },
    /// Discovered from a known e-commerce platform.
    Platform {
        /// Platform name (e.g., `"shopify"`, `"woocommerce"`).
        platform: String,
        /// Action type from the platform template (e.g., `"add_to_cart"`).
        action_type: String,
    },
}

/// The detected e-commerce platform for a site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedPlatform {
    /// Shopify storefront.
    Shopify,
    /// WooCommerce (WordPress).
    WooCommerce,
    /// Adobe Commerce / Magento.
    Magento,
    /// BigCommerce.
    BigCommerce,
    /// No recognised platform.
    Unknown,
}

// ── Internal JSON deserialization types ─────────────────────────────────────

#[derive(Debug, Deserialize)]
struct PlatformConfig {
    indicators: PlatformIndicators,
    actions: Vec<PlatformActionTemplate>,
}

#[derive(Debug, Deserialize)]
struct PlatformIndicators {
    js_patterns: Vec<String>,
    html_patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PlatformActionTemplate {
    label: String,
    opcode: PlatformOpCode,
    action_type: String,
    #[allow(dead_code)]
    url_template: String,
    #[allow(dead_code)]
    method: String,
    confidence: f32,
}

#[derive(Debug, Deserialize)]
struct PlatformOpCode {
    category: u8,
    action: u8,
}

type PlatformRegistry = std::collections::HashMap<String, PlatformConfig>;

/// Parse and cache the embedded platform action templates.
fn platform_registry() -> &'static PlatformRegistry {
    static REGISTRY: OnceLock<PlatformRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| serde_json::from_str(PLATFORM_ACTIONS_JSON).unwrap_or_default())
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Discover HTTP actions from HTML forms and interactive elements.
///
/// Parses every `<form>` tag in the document, resolves action URLs relative to
/// `base_url`, and extracts all input fields (including hidden ones such as
/// CSRF tokens and product IDs). Buttons associated with forms are also
/// recognised.
///
/// # Arguments
///
/// * `html` - Raw HTML source of the page.
/// * `base_url` - Base URL for resolving relative action URLs.
///
/// # Returns
///
/// A vector of [`HttpAction`] items, one per form discovered.
pub fn discover_actions_from_html(html: &str, base_url: &str) -> Vec<HttpAction> {
    let document = Html::parse_document(html);
    let mut actions = Vec::new();

    // Parse all <form> elements.
    let form_sel = match Selector::parse("form") {
        Ok(s) => s,
        Err(_) => return actions,
    };
    let field_sel =
        Selector::parse("input, select, textarea").expect("field selector is valid");
    let button_sel =
        Selector::parse("button, input[type=\"submit\"]").expect("button selector is valid");

    for form in document.select(&form_sel) {
        let action_raw = form.value().attr("action").unwrap_or("");
        let action_url = resolve_url(base_url, action_raw);
        let method = form
            .value()
            .attr("method")
            .unwrap_or("GET")
            .to_uppercase();
        let enctype = form
            .value()
            .attr("enctype")
            .unwrap_or("application/x-www-form-urlencoded")
            .to_string();

        let mut fields = Vec::new();
        for field in form.select(&field_sel) {
            let name = field.value().attr("name").unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let field_type = field
                .value()
                .attr("type")
                .unwrap_or(field.value().name())
                .to_string();
            let value = field.value().attr("value").map(String::from);
            let required = field.value().attr("required").is_some();

            fields.push(FormField {
                name,
                field_type,
                value,
                required,
            });
        }

        // Derive label from submit button text or value.
        let submit_label = form
            .select(&button_sel)
            .next()
            .map(|el| {
                el.value()
                    .attr("value")
                    .map(String::from)
                    .unwrap_or_else(|| element_text(&el))
            })
            .filter(|s| !s.is_empty());

        let label = submit_label.unwrap_or_else(|| format!("Form \u{2192} {action_url}"));
        let opcode = classify_form_opcode(&label, &action_url);

        actions.push(HttpAction {
            opcode,
            label,
            source: ActionSource::Form {
                action_url,
                method,
                content_type: enctype,
                fields,
            },
            confidence: 0.90,
        });
    }

    actions
}

/// Discover HTTP actions from JavaScript source code.
///
/// Uses regex patterns to find API calls made with `fetch()`, `axios`,
/// `$.ajax`, and bare `/api/` string literals in the JS source.
///
/// # Arguments
///
/// * `js_source` - Raw JavaScript source text.
/// * `base_url` - Base URL for resolving relative API paths.
///
/// # Returns
///
/// A vector of [`HttpAction`] items, one per discovered API endpoint.
pub fn discover_actions_from_js(js_source: &str, base_url: &str) -> Vec<HttpAction> {
    let mut actions = Vec::new();
    let mut seen_urls: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Pattern 1: fetch("url", {method: "METHOD"})
    let fetch_re = Regex::new(
        r#"fetch\(\s*['"]([^'"]+)['"]\s*,\s*\{[^}]*method\s*:\s*['"](\w+)['"]"#,
    )
    .expect("fetch regex is valid");

    for caps in fetch_re.captures_iter(js_source) {
        let url_raw = caps.get(1).map_or("", |m| m.as_str());
        let method = caps
            .get(2)
            .map_or("GET", |m| m.as_str())
            .to_uppercase();
        let url = resolve_url(base_url, url_raw);
        if seen_urls.insert(format!("{method}:{url}")) {
            actions.push(HttpAction {
                opcode: classify_api_opcode(&url, &method),
                label: format!("{method} {url_raw}"),
                source: ActionSource::Api {
                    url,
                    method,
                    body_template: None,
                },
                confidence: 0.80,
            });
        }
    }

    // Pattern 1b: fetch("url") -- no options, defaults to GET
    let fetch_simple_re =
        Regex::new(r#"fetch\(\s*['"]([^'"]+)['"]\s*\)"#).expect("fetch simple regex is valid");

    for caps in fetch_simple_re.captures_iter(js_source) {
        let url_raw = caps.get(1).map_or("", |m| m.as_str());
        let method = "GET".to_string();
        let url = resolve_url(base_url, url_raw);
        if seen_urls.insert(format!("{method}:{url}")) {
            actions.push(HttpAction {
                opcode: classify_api_opcode(&url, &method),
                label: format!("GET {url_raw}"),
                source: ActionSource::Api {
                    url,
                    method,
                    body_template: None,
                },
                confidence: 0.70,
            });
        }
    }

    // Pattern 2: axios.get|post|put|delete("url")
    let axios_re = Regex::new(
        r#"axios\.(get|post|put|delete|patch)\(\s*['"]([^'"]+)['"]"#,
    )
    .expect("axios regex is valid");

    for caps in axios_re.captures_iter(js_source) {
        let method = caps
            .get(1)
            .map_or("GET", |m| m.as_str())
            .to_uppercase();
        let url_raw = caps.get(2).map_or("", |m| m.as_str());
        let url = resolve_url(base_url, url_raw);
        if seen_urls.insert(format!("{method}:{url}")) {
            actions.push(HttpAction {
                opcode: classify_api_opcode(&url, &method),
                label: format!("{method} {url_raw}"),
                source: ActionSource::Api {
                    url,
                    method,
                    body_template: None,
                },
                confidence: 0.80,
            });
        }
    }

    // Pattern 3: $.ajax({url: "...", type: "..."})
    let ajax_block_re =
        Regex::new(r#"\$\.ajax\(\s*\{([^}]*)\}"#).expect("ajax block regex is valid");
    let inner_url_re =
        Regex::new(r#"url\s*:\s*['"]([^'"]+)['"]"#).expect("inner url regex is valid");
    let inner_type_re =
        Regex::new(r#"type\s*:\s*['"](\w+)['"]"#).expect("inner type regex is valid");

    for caps in ajax_block_re.captures_iter(js_source) {
        let block = caps.get(1).map_or("", |m| m.as_str());
        if let Some(url_caps) = inner_url_re.captures(block) {
            let url_raw = url_caps.get(1).map_or("", |m| m.as_str());
            let method = inner_type_re
                .captures(block)
                .and_then(|c| c.get(1))
                .map_or("GET", |m| m.as_str())
                .to_uppercase();
            let url = resolve_url(base_url, url_raw);
            if seen_urls.insert(format!("{method}:{url}")) {
                actions.push(HttpAction {
                    opcode: classify_api_opcode(&url, &method),
                    label: format!("{method} {url_raw}"),
                    source: ActionSource::Api {
                        url,
                        method,
                        body_template: None,
                    },
                    confidence: 0.75,
                });
            }
        }
    }

    // Pattern 4: bare /api/ path string literals
    let api_path_re =
        Regex::new(r#"['"](/api/[^'"]+)['"]"#).expect("api path regex is valid");

    for caps in api_path_re.captures_iter(js_source) {
        let url_raw = caps.get(1).map_or("", |m| m.as_str());
        let url = resolve_url(base_url, url_raw);
        let method = "GET".to_string();
        if seen_urls.insert(format!("{method}:{url}")) {
            actions.push(HttpAction {
                opcode: classify_api_opcode(&url, &method),
                label: format!("API {url_raw}"),
                source: ActionSource::Api {
                    url,
                    method,
                    body_template: None,
                },
                confidence: 0.60,
            });
        }
    }

    actions
}

/// Detect the e-commerce platform and return platform-specific actions.
///
/// Checks for Shopify, WooCommerce, Magento, and BigCommerce fingerprints
/// in both HTML content and JavaScript. When a platform is detected, loads
/// pre-built action templates from the embedded `platform_actions.json`.
///
/// # Arguments
///
/// * `domain` - The domain being mapped (used for URL resolution).
/// * `page_html` - Raw HTML source of the page.
///
/// # Returns
///
/// A vector of [`HttpAction`] items from the detected platform's template.
/// Returns an empty vector if no platform is detected.
pub fn discover_actions_from_platform(_domain: &str, page_html: &str) -> Vec<HttpAction> {
    let platform = detect_platform(page_html);
    if platform == DetectedPlatform::Unknown {
        return Vec::new();
    }

    let platform_key = match platform {
        DetectedPlatform::Shopify => "shopify",
        DetectedPlatform::WooCommerce => "woocommerce",
        DetectedPlatform::Magento => "magento",
        DetectedPlatform::BigCommerce => "bigcommerce",
        DetectedPlatform::Unknown => return Vec::new(),
    };

    let registry = platform_registry();
    let config = match registry.get(platform_key) {
        Some(c) => c,
        None => return Vec::new(),
    };

    config
        .actions
        .iter()
        .map(|tmpl| HttpAction {
            opcode: OpCode::new(tmpl.opcode.category, tmpl.opcode.action),
            label: tmpl.label.clone(),
            source: ActionSource::Platform {
                platform: platform_key.to_string(),
                action_type: tmpl.action_type.clone(),
            },
            confidence: tmpl.confidence,
        })
        .collect()
}

/// Detect the e-commerce platform from HTML content.
///
/// Checks for known fingerprints in the page source: JavaScript global
/// objects, CDN URLs, CSS class names, and plugin paths.
///
/// # Arguments
///
/// * `html` - Raw HTML source of the page.
///
/// # Returns
///
/// The [`DetectedPlatform`] variant for the first matching platform,
/// or [`DetectedPlatform::Unknown`] if none matched.
pub fn detect_platform(html: &str) -> DetectedPlatform {
    let registry = platform_registry();

    // Check each platform in a deterministic order.
    let platform_order = ["shopify", "woocommerce", "magento", "bigcommerce"];

    for &key in &platform_order {
        if let Some(config) = registry.get(key) {
            let matched = config
                .indicators
                .js_patterns
                .iter()
                .any(|pat| html.contains(pat.as_str()))
                || config
                    .indicators
                    .html_patterns
                    .iter()
                    .any(|pat| html.contains(pat.as_str()));

            if matched {
                return match key {
                    "shopify" => DetectedPlatform::Shopify,
                    "woocommerce" => DetectedPlatform::WooCommerce,
                    "magento" => DetectedPlatform::Magento,
                    "bigcommerce" => DetectedPlatform::BigCommerce,
                    _ => DetectedPlatform::Unknown,
                };
            }
        }
    }

    DetectedPlatform::Unknown
}

// ── Private helpers ─────────────────────────────────────────────────────────

/// Collect all visible text content from an element, trimmed and
/// whitespace-collapsed.
fn element_text(el: &scraper::ElementRef<'_>) -> String {
    el.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(base_url: &str, relative: &str) -> String {
    if relative.is_empty() {
        return base_url.to_string();
    }
    if relative.starts_with("http://") || relative.starts_with("https://") {
        return relative.to_string();
    }
    if let Ok(base) = url::Url::parse(base_url) {
        if let Ok(resolved) = base.join(relative) {
            return resolved.to_string();
        }
    }
    relative.to_string()
}

/// Classify a form into an OpCode based on its submit label and action URL.
fn classify_form_opcode(label: &str, action_url: &str) -> OpCode {
    let label_lower = label.to_lowercase();
    let url_lower = action_url.to_lowercase();

    if label_lower.contains("add to cart")
        || label_lower.contains("add to bag")
        || url_lower.contains("/cart/add")
    {
        return OpCode::new(0x02, 0x00); // commerce: add_to_cart
    }
    if label_lower.contains("search") || url_lower.contains("/search") {
        return OpCode::new(0x00, 0x01); // nav: search
    }
    if label_lower.contains("log in")
        || label_lower.contains("login")
        || label_lower.contains("sign in")
        || url_lower.contains("/login")
        || url_lower.contains("/signin")
    {
        return OpCode::new(0x04, 0x01); // form: login
    }
    if label_lower.contains("subscribe") || label_lower.contains("newsletter") {
        return OpCode::new(0x04, 0x03); // form: subscribe
    }
    if label_lower.contains("register")
        || label_lower.contains("sign up")
        || label_lower.contains("signup")
    {
        return OpCode::new(0x04, 0x02); // form: register
    }

    // Default: generic form submit
    OpCode::new(0x04, 0x00)
}

/// Classify an API endpoint into an OpCode based on URL path and method.
fn classify_api_opcode(url: &str, method: &str) -> OpCode {
    let url_lower = url.to_lowercase();

    if url_lower.contains("/cart") || url_lower.contains("/basket") {
        return match method {
            "POST" => OpCode::new(0x02, 0x00),   // commerce: add_to_cart
            "DELETE" => OpCode::new(0x02, 0x01),  // commerce: remove_from_cart
            "PUT" | "PATCH" => OpCode::new(0x02, 0x02), // commerce: update_cart
            _ => OpCode::new(0x02, 0x06),         // commerce: view_cart
        };
    }
    if url_lower.contains("/search") || url_lower.contains("/query") {
        return OpCode::new(0x00, 0x01); // nav: search
    }
    if url_lower.contains("/auth")
        || url_lower.contains("/login")
        || url_lower.contains("/session")
    {
        return OpCode::new(0x04, 0x01); // form: login
    }

    // Default: generic API call
    OpCode::new(0x06, 0x00) // data: api_call
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_form_actions() {
        let html = r#"
        <html><body>
            <form action="/search" method="GET">
                <input type="text" name="q" required />
                <input type="hidden" name="ref" value="nav_search" />
                <button type="submit">Search</button>
            </form>
        </body></html>
        "#;

        let actions = discover_actions_from_html(html, "https://example.com");
        assert_eq!(actions.len(), 1);

        let action = &actions[0];
        if let ActionSource::Form { fields, .. } = &action.source {
            assert_eq!(fields.len(), 2);

            let q_field = fields.iter().find(|f| f.name == "q").unwrap();
            assert_eq!(q_field.field_type, "text");
            assert!(q_field.required);

            let ref_field = fields.iter().find(|f| f.name == "ref").unwrap();
            assert_eq!(ref_field.field_type, "hidden");
            assert_eq!(ref_field.value.as_deref(), Some("nav_search"));
        } else {
            panic!("expected ActionSource::Form");
        }
    }

    #[test]
    fn test_discover_form_post_action() {
        let html = r#"
        <html><body>
            <form action="/login" method="POST">
                <input type="text" name="username" />
                <input type="password" name="password" />
                <button type="submit">Log In</button>
            </form>
        </body></html>
        "#;

        let actions = discover_actions_from_html(html, "https://example.com");
        assert_eq!(actions.len(), 1);

        let action = &actions[0];
        if let ActionSource::Form {
            action_url,
            method,
            ..
        } = &action.source
        {
            assert_eq!(action_url, "https://example.com/login");
            assert_eq!(method, "POST");
        } else {
            panic!("expected ActionSource::Form");
        }
    }

    #[test]
    fn test_discover_js_fetch() {
        let js = r#"
            async function addItem(id) {
                const resp = await fetch('/api/cart', {method: 'POST', body: JSON.stringify({id})});
                return resp.json();
            }
        "#;

        let actions = discover_actions_from_js(js, "https://shop.example.com");
        assert!(!actions.is_empty());

        let cart_action = actions
            .iter()
            .find(|a| {
                matches!(
                    &a.source,
                    ActionSource::Api { url, .. } if url.contains("/api/cart")
                )
            })
            .expect("should find /api/cart action");

        if let ActionSource::Api { method, .. } = &cart_action.source {
            assert_eq!(method, "POST");
        } else {
            panic!("expected ActionSource::Api");
        }
    }

    #[test]
    fn test_discover_js_axios() {
        let js = r#"
            axios.post('/api/items', { name: 'widget' });
            axios.get('/api/items/123');
        "#;

        let actions = discover_actions_from_js(js, "https://example.com");
        assert!(actions.len() >= 2);

        let post_action = actions
            .iter()
            .find(|a| {
                matches!(
                    &a.source,
                    ActionSource::Api { url, method, .. }
                        if url.contains("/api/items") && !url.contains("/123") && method == "POST"
                )
            })
            .expect("should find POST /api/items");
        assert!(
            matches!(&post_action.source, ActionSource::Api { method, .. } if method == "POST")
        );

        let get_action = actions
            .iter()
            .find(|a| {
                matches!(
                    &a.source,
                    ActionSource::Api { url, method, .. }
                        if url.contains("/api/items/123") && method == "GET"
                )
            })
            .expect("should find GET /api/items/123");
        assert!(
            matches!(&get_action.source, ActionSource::Api { method, .. } if method == "GET")
        );
    }

    #[test]
    fn test_detect_shopify() {
        let html = r#"
        <html>
        <head>
            <script src="https://cdn.shopify.com/s/files/1/shop.js"></script>
        </head>
        <body>
            <div id="product">Widget</div>
        </body>
        </html>
        "#;

        assert_eq!(detect_platform(html), DetectedPlatform::Shopify);
    }

    #[test]
    fn test_detect_woocommerce() {
        let html = r#"
        <html>
        <body class="woocommerce woocommerce-page">
            <div class="product">
                <a href="/wp-content/plugins/woocommerce/assets/style.css"></a>
            </div>
        </body>
        </html>
        "#;

        assert_eq!(detect_platform(html), DetectedPlatform::WooCommerce);
    }

    #[test]
    fn test_detect_no_platform() {
        let html = r#"
        <html>
        <body>
            <h1>My Personal Blog</h1>
            <p>Nothing to see here.</p>
        </body>
        </html>
        "#;

        assert_eq!(detect_platform(html), DetectedPlatform::Unknown);
    }

    #[test]
    fn test_discover_platform_actions_shopify() {
        let html = r#"
        <html>
        <head>
            <script>
                Shopify.shop = "mystore.myshopify.com";
            </script>
        </head>
        <body>
            <div class="product">Widget</div>
        </body>
        </html>
        "#;

        let actions = discover_actions_from_platform("mystore.myshopify.com", html);
        assert!(!actions.is_empty());

        let add_to_cart = actions
            .iter()
            .find(|a| {
                matches!(
                    &a.source,
                    ActionSource::Platform { action_type, .. } if action_type == "add_to_cart"
                )
            })
            .expect("should find add_to_cart action");

        assert_eq!(add_to_cart.opcode, OpCode::new(0x02, 0x00));
        assert!(add_to_cart.confidence > 0.0);
    }

    #[test]
    fn test_empty_html() {
        let actions = discover_actions_from_html("", "https://example.com");
        assert!(actions.is_empty());
    }

    #[test]
    fn test_form_csrf_token() {
        let html = r#"
        <html><body>
            <form action="/transfer" method="POST">
                <input type="hidden" name="_csrf" value="abc123def456" />
                <input type="hidden" name="_token" value="xyz789" />
                <input type="number" name="amount" required />
                <button type="submit">Submit</button>
            </form>
        </body></html>
        "#;

        let actions = discover_actions_from_html(html, "https://bank.example.com");
        assert_eq!(actions.len(), 1);

        if let ActionSource::Form { fields, .. } = &actions[0].source {
            let csrf = fields
                .iter()
                .find(|f| f.name == "_csrf")
                .expect("should find _csrf field");
            assert_eq!(csrf.field_type, "hidden");
            assert_eq!(csrf.value.as_deref(), Some("abc123def456"));

            let token = fields
                .iter()
                .find(|f| f.name == "_token")
                .expect("should find _token field");
            assert_eq!(token.field_type, "hidden");
            assert_eq!(token.value.as_deref(), Some("xyz789"));
        } else {
            panic!("expected ActionSource::Form");
        }
    }
}
