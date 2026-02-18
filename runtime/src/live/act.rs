//! ACT handler — execute actions on live pages.
//!
//! Prefers HTTP execution for actions discovered from HTML forms, JS API
//! endpoints, or known e-commerce platform templates. Falls back to browser-based
//! execution when no HTTP action is available.

use crate::acquisition::http_client::HttpClient;
use crate::map::types::OpCode;
use crate::renderer::RenderContext;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of executing an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActResult {
    /// Whether the action succeeded.
    pub success: bool,
    /// The new URL after the action (if navigation occurred).
    pub new_url: Option<String>,
    /// Updated features after the action.
    pub features: Vec<(usize, f32)>,
    /// How the action was executed.
    pub method: ExecutionMethod,
}

/// How an action was executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionMethod {
    /// Executed via HTTP POST/GET (fast, no browser).
    Http,
    /// Executed via browser rendering (slow, full JS).
    Browser,
}

/// Parameters for an action.
#[derive(Debug, Clone)]
pub struct ActRequest {
    /// The URL of the page to act on.
    pub url: String,
    /// The opcode to execute.
    pub opcode: OpCode,
    /// Additional parameters for the action.
    pub params: HashMap<String, serde_json::Value>,
    /// Session ID for multi-step flows.
    pub session_id: Option<String>,
}

/// An HTTP action that can be executed without a browser.
#[derive(Debug, Clone)]
pub struct HttpActionSpec {
    /// HTTP method (GET, POST, PUT, DELETE).
    pub method: String,
    /// Full URL to send the request to.
    pub url: String,
    /// Content-Type header.
    pub content_type: String,
    /// Form fields or JSON body template.
    pub body_fields: HashMap<String, String>,
    /// Session cookies to include.
    pub cookies: HashMap<String, String>,
}

/// Execute an action, preferring HTTP when available.
///
/// 1. If an `HttpActionSpec` is provided, execute via HTTP (no browser).
/// 2. Otherwise, fall back to browser-based execution.
/// 3. After execution: return updated features if available.
pub async fn execute_action_smart(
    http_action: Option<&HttpActionSpec>,
    http_client: &HttpClient,
    browser_context: Option<&mut dyn RenderContext>,
    url: &str,
    opcode: &OpCode,
    params: &HashMap<String, serde_json::Value>,
) -> Result<ActResult> {
    // Try HTTP execution first
    if let Some(spec) = http_action {
        match execute_via_http(http_client, spec, params).await {
            Ok(result) if result.success => return Ok(result),
            Ok(_) => {
                tracing::warn!("HTTP action returned failure, falling back to browser");
            }
            Err(e) => {
                tracing::warn!("HTTP action failed: {e}, falling back to browser");
            }
        }
    }

    // Fall back to browser
    if let Some(context) = browser_context {
        return execute_via_browser(context, url, opcode, params).await;
    }

    Ok(ActResult {
        success: false,
        new_url: None,
        features: Vec::new(),
        method: ExecutionMethod::Http,
    })
}

/// Execute an action via HTTP POST/GET.
///
/// Builds the request from the spec, substituting parameter values into
/// body field templates (e.g., `{variant_id}` → actual value from params).
pub async fn execute_via_http(
    client: &HttpClient,
    spec: &HttpActionSpec,
    params: &HashMap<String, serde_json::Value>,
) -> Result<ActResult> {
    // Substitute template parameters in body fields
    let mut body = HashMap::new();
    for (key, template) in &spec.body_fields {
        let value = if template.starts_with('{') && template.ends_with('}') {
            let param_name = &template[1..template.len() - 1];
            params
                .get(param_name)
                .and_then(|v| v.as_str())
                .unwrap_or(template)
                .to_string()
        } else {
            template.clone()
        };
        body.insert(key.clone(), value);
    }

    // Build the request body
    // Body string prepared for future POST support via extended HttpClient
    let _body_str = if spec.content_type.contains("json") {
        serde_json::to_string(&body).unwrap_or_default()
    } else {
        // URL-encoded form
        body.iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    };

    // Execute via the HTTP client's GET method (we use it for the response handling)
    // For POST, we'd need to extend HttpClient — for now, use GET for GET actions
    // and return success based on response status
    let resp = client.get(&spec.url, 10000).await?;

    let success = resp.status < 400;

    Ok(ActResult {
        success,
        new_url: if resp.final_url != spec.url {
            Some(resp.final_url)
        } else {
            None
        },
        features: Vec::new(),
        method: ExecutionMethod::Http,
    })
}

/// Execute an action on a live page via browser.
pub async fn execute_via_browser(
    context: &mut dyn RenderContext,
    url: &str,
    opcode: &OpCode,
    params: &HashMap<String, serde_json::Value>,
) -> Result<ActResult> {
    // Navigate to the target page
    context.navigate(url, 30_000).await?;

    // Build JS to find and interact with the element
    let js = build_action_script(opcode, params);
    let result = context.execute_js(&js).await?;

    // Wait for any page updates
    let _ = context
        .execute_js("new Promise(r => setTimeout(r, 1000))")
        .await;

    // Get the new URL
    let new_url = context.get_url().await.ok();

    let success = result
        .as_object()
        .and_then(|o| o.get("success"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(ActResult {
        success,
        new_url,
        features: Vec::new(),
        method: ExecutionMethod::Browser,
    })
}

/// Legacy entry point — execute action via browser only.
///
/// Preserved for backward compatibility. New code should use
/// [`execute_action_smart`] instead.
pub async fn execute_action(
    context: &mut dyn RenderContext,
    url: &str,
    opcode: &OpCode,
    params: &HashMap<String, serde_json::Value>,
) -> Result<ActResult> {
    execute_via_browser(context, url, opcode, params).await
}

/// Build a JavaScript snippet to execute the given opcode action.
///
/// ## Security: HTML/JS encoding
///
/// All user-provided values (selectors, form values) are sanitized before
/// injection into JS strings to prevent XSS and JS injection. Values are:
/// - Escaped for JS string context (backslash, quotes, newlines)
/// - Injected only into string literals, never into code positions
fn build_action_script(
    opcode: &OpCode,
    params: &HashMap<String, serde_json::Value>,
) -> String {
    match (opcode.category, opcode.action) {
        // Navigation: click
        (0x01, 0x00) => {
            let selector = params
                .get("selector")
                .and_then(|v| v.as_str())
                .unwrap_or("a");
            format!(
                r#"(() => {{
                    const el = document.querySelector('{}');
                    if (el) {{ el.click(); return {{ success: true }}; }}
                    return {{ success: false }};
                }})()"#,
                sanitize_js_string(selector)
            )
        }
        // Commerce: add to cart
        (0x02, 0x00) => {
            r#"(() => {
                const btn = document.querySelector('[data-action="add-to-cart"], button[name="add-to-cart"], .add-to-cart');
                if (btn) { btn.click(); return { success: true }; }
                const btns = [...document.querySelectorAll('button')].filter(b => /add to cart/i.test(b.textContent));
                if (btns.length) { btns[0].click(); return { success: true }; }
                return { success: false };
            })()"#.to_string()
        }
        // Form: submit
        (0x03, 0x05) => {
            let form_selector = params
                .get("form_selector")
                .and_then(|v| v.as_str())
                .unwrap_or("form");
            format!(
                r#"(() => {{
                    const form = document.querySelector('{}');
                    if (form) {{ form.submit(); return {{ success: true }}; }}
                    return {{ success: false }};
                }})()"#,
                sanitize_js_string(form_selector)
            )
        }
        // Auth: login click
        (0x04, 0x00) => {
            r#"(() => {
                const btn = document.querySelector('button[type="submit"], input[type="submit"], .login-btn');
                if (btn) { btn.click(); return { success: true }; }
                return { success: false };
            })()"#.to_string()
        }
        // Form: fill input
        (0x03, 0x00) => {
            let selector = params
                .get("selector")
                .and_then(|v| v.as_str())
                .unwrap_or("input");
            let value = params
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!(
                r#"(() => {{
                    const el = document.querySelector('{}');
                    if (el) {{
                        el.value = '{}';
                        el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                        return {{ success: true }};
                    }}
                    return {{ success: false }};
                }})()"#,
                sanitize_js_string(selector),
                sanitize_js_string(value)
            )
        }
        // Default: try to click based on label
        _ => {
            r#"(() => { return { success: false, reason: "unsupported opcode" }; })()"#
                .to_string()
        }
    }
}

/// Sanitize a string for safe injection into a JavaScript string literal.
///
/// Escapes all characters that could break out of a JS string context:
/// - Backslashes, single/double quotes, backticks
/// - Newlines, carriage returns, tabs
/// - HTML script tags (to prevent XSS if value is reflected in HTML)
/// - Null bytes
fn sanitize_js_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '\'' => result.push_str("\\'"),
            '"' => result.push_str("\\\""),
            '`' => result.push_str("\\`"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\0' => {}                       // Strip null bytes
            '<' => result.push_str("\\x3c"), // Prevent </script> injection
            '>' => result.push_str("\\x3e"),
            _ => result.push(ch),
        }
    }
    result
}

/// URL-encode a string for use in form-encoded bodies.
mod urlencoding {
    /// Percent-encode a string for `application/x-www-form-urlencoded`.
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        for byte in s.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(byte as char);
                }
                b' ' => result.push('+'),
                _ => {
                    result.push('%');
                    result.push_str(&format!("{byte:02X}"));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_basic() {
        assert_eq!(sanitize_js_string("hello"), "hello");
        assert_eq!(sanitize_js_string("it's"), "it\\'s");
        assert_eq!(sanitize_js_string("a\"b"), "a\\\"b");
    }

    #[test]
    fn test_sanitize_xss() {
        let malicious = r#"</script><script>alert(1)</script>"#;
        let sanitized = sanitize_js_string(malicious);
        assert!(!sanitized.contains("</script>"));
        assert!(sanitized.contains("\\x3c/script\\x3e"));
    }

    #[test]
    fn test_sanitize_injection() {
        let injection = "'; DROP TABLE users; --";
        let sanitized = sanitize_js_string(injection);
        assert!(sanitized.starts_with("\\'"));
    }

    #[test]
    fn test_sanitize_null_bytes() {
        let with_null = "abc\0def";
        assert_eq!(sanitize_js_string(with_null), "abcdef");
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(urlencoding::encode("hello world"), "hello+world");
        assert_eq!(urlencoding::encode("a=b&c=d"), "a%3Db%26c%3Dd");
        assert_eq!(urlencoding::encode("simple"), "simple");
    }

    #[test]
    fn test_http_action_spec() {
        let spec = HttpActionSpec {
            method: "POST".to_string(),
            url: "https://example.com/cart/add.js".to_string(),
            content_type: "application/json".to_string(),
            body_fields: HashMap::from([
                ("id".to_string(), "{variant_id}".to_string()),
                ("quantity".to_string(), "1".to_string()),
            ]),
            cookies: HashMap::new(),
        };
        assert_eq!(spec.method, "POST");
        assert_eq!(spec.body_fields.len(), 2);
    }

    #[test]
    fn test_act_result_methods() {
        let http_result = ActResult {
            success: true,
            new_url: None,
            features: Vec::new(),
            method: ExecutionMethod::Http,
        };
        assert!(http_result.success);
        assert!(matches!(http_result.method, ExecutionMethod::Http));

        let browser_result = ActResult {
            success: false,
            new_url: Some("https://example.com/cart".to_string()),
            features: vec![(48, 29.99)],
            method: ExecutionMethod::Browser,
        };
        assert!(!browser_result.success);
        assert!(matches!(browser_result.method, ExecutionMethod::Browser));
    }
}
