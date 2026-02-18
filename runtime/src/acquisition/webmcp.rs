//! WebMCP tool discovery and execution.
//!
//! [WebMCP](https://webmcp.org) is a protocol where websites expose structured
//! tool definitions to AI agents via `navigator.modelContext`. When a site
//! supports WebMCP, the tools are explicitly defined with typed parameters
//! and return values — the highest-reliability execution path.
//!
//! WebMCP requires a browser context (the tools live in the page's JS), but
//! unlike DOM scraping, the tool contract is explicit and machine-readable.
//!
//! Discovery is **opportunistic**: Cortex checks for WebMCP only when a browser
//! context is already open (Layer 3 fallback or ACT). It never opens a browser
//! just to check for WebMCP.

use crate::renderer::RenderContext;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A tool exposed by a web page via the WebMCP protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebMcpTool {
    /// Tool name (e.g., `"searchProducts"`, `"addToCart"`).
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's parameters.
    pub parameters: serde_json::Value,
    /// JSON Schema describing the tool's return value, if any.
    pub returns: Option<serde_json::Value>,
}

/// Result of executing a WebMCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebMcpResult {
    /// Whether the tool execution succeeded.
    pub success: bool,
    /// The tool's return value.
    pub value: serde_json::Value,
    /// Error message, if any.
    pub error: Option<String>,
}

/// Discover WebMCP tools available on the current page.
///
/// Executes JavaScript in the page context to check for the WebMCP API
/// (`navigator.modelContext`) and enumerate available tools.
///
/// Returns an empty vector if WebMCP is not supported on this page.
///
/// # Arguments
///
/// * `context` - An active browser render context with a loaded page.
pub async fn discover_webmcp_tools(context: &dyn RenderContext) -> Vec<WebMcpTool> {
    let js = r#"
    (async () => {
        try {
            if (!navigator.modelContext) return null;
            const mc = navigator.modelContext;
            if (typeof mc.getTools !== 'function') return null;
            const tools = await mc.getTools();
            if (!Array.isArray(tools) || tools.length === 0) return null;
            return JSON.stringify(tools.map(t => ({
                name: t.name || '',
                description: t.description || '',
                parameters: t.parameters || t.inputSchema || {},
                returns: t.returns || t.outputSchema || null,
            })));
        } catch(e) {
            return null;
        }
    })()
    "#;

    let result = match context.execute_js(js).await {
        Ok(val) => val,
        Err(_) => return Vec::new(),
    };

    let json_str = match result.as_str() {
        Some(s) => s,
        None => return Vec::new(),
    };

    match serde_json::from_str::<Vec<WebMcpTool>>(json_str) {
        Ok(tools) => {
            if !tools.is_empty() {
                tracing::info!("WebMCP: discovered {} tools on page", tools.len());
            }
            tools
        }
        Err(_) => Vec::new(),
    }
}

/// Execute a WebMCP tool on the current page.
///
/// Calls the tool via `navigator.modelContext.callTool()` with the given
/// parameters. The tool runs in the page's JavaScript context.
///
/// # Arguments
///
/// * `context` - An active browser render context with a loaded page.
/// * `tool_name` - The name of the tool to execute.
/// * `params` - Parameters to pass to the tool (must match the tool's schema).
pub async fn execute_webmcp_tool(
    context: &dyn RenderContext,
    tool_name: &str,
    params: &serde_json::Value,
) -> Result<WebMcpResult> {
    let params_json = serde_json::to_string(params)?;

    // Sanitize tool_name for safe JS injection
    let safe_name: String = tool_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect();

    let js = format!(
        r#"
        (async () => {{
            try {{
                if (!navigator.modelContext) {{
                    return JSON.stringify({{ success: false, value: null, error: "WebMCP not available" }});
                }}
                const mc = navigator.modelContext;
                if (typeof mc.callTool !== 'function') {{
                    return JSON.stringify({{ success: false, value: null, error: "callTool not available" }});
                }}
                const result = await mc.callTool("{name}", {params});
                return JSON.stringify({{ success: true, value: result, error: null }});
            }} catch(e) {{
                return JSON.stringify({{ success: false, value: null, error: e.message || String(e) }});
            }}
        }})()
        "#,
        name = safe_name,
        params = params_json,
    );

    let result = context.execute_js(&js).await?;
    let json_str = result
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("WebMCP tool returned non-string result"))?;

    let mcp_result: WebMcpResult = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("failed to parse WebMCP result: {e}"))?;

    if mcp_result.success {
        tracing::info!("WebMCP: tool '{}' executed successfully", safe_name);
    } else {
        tracing::warn!(
            "WebMCP: tool '{}' failed: {}",
            safe_name,
            mcp_result.error.as_deref().unwrap_or("unknown error")
        );
    }

    Ok(mcp_result)
}

/// Check if a page likely supports WebMCP by scanning its HTML source.
///
/// This is a fast heuristic check -- it looks for WebMCP-related script tags
/// or meta tags. Definitive detection requires `discover_webmcp_tools()` in
/// a browser context.
pub fn might_have_webmcp(html: &str) -> bool {
    html.contains("modelContext")
        || html.contains("webmcp")
        || html.contains("model-context")
        || html.contains("navigator.ai")
}

/// Find a matching WebMCP tool by name from a list of discovered tools.
pub fn find_tool_by_name<'a>(tools: &'a [WebMcpTool], name: &str) -> Option<&'a WebMcpTool> {
    tools.iter().find(|t| t.name == name)
}

/// Find WebMCP tools that match an opcode category.
///
/// Maps opcode categories to likely WebMCP tool name patterns:
/// - `0x01` (navigation) -> "search", "navigate", "find"
/// - `0x02` (commerce) -> "addToCart", "removeFromCart", "checkout"
/// - `0x03` (form) -> "submit", "fill", "validate"
/// - `0x04` (auth) -> "login", "register", "logout"
pub fn find_tools_for_opcode(tools: &[WebMcpTool], category: u8) -> Vec<&WebMcpTool> {
    let patterns: &[&str] = match category {
        0x01 => &["search", "navigate", "find", "filter", "sort"],
        0x02 => &[
            "cart", "add", "remove", "checkout", "buy", "purchase", "wishlist",
        ],
        0x03 => &["submit", "fill", "validate", "form"],
        0x04 => &["login", "register", "logout", "auth", "signin", "signup"],
        0x05 => &["play", "pause", "stop", "media", "download"],
        0x06 => &["share", "like", "comment", "follow", "subscribe"],
        _ => &[],
    };

    tools
        .iter()
        .filter(|tool| {
            let name_lower = tool.name.to_lowercase();
            let desc_lower = tool.description.to_lowercase();
            patterns
                .iter()
                .any(|p| name_lower.contains(p) || desc_lower.contains(p))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_might_have_webmcp() {
        assert!(might_have_webmcp(
            r#"<script>navigator.modelContext.getTools()</script>"#
        ));
        assert!(might_have_webmcp(r#"<meta name="webmcp" content="v1">"#));
        assert!(!might_have_webmcp(r#"<html><body>Hello</body></html>"#));
    }

    #[test]
    fn test_find_tool_by_name() {
        let tools = vec![
            WebMcpTool {
                name: "searchProducts".to_string(),
                description: "Search for products".to_string(),
                parameters: serde_json::json!({"type": "object"}),
                returns: None,
            },
            WebMcpTool {
                name: "addToCart".to_string(),
                description: "Add item to cart".to_string(),
                parameters: serde_json::json!({"type": "object"}),
                returns: None,
            },
        ];

        assert!(find_tool_by_name(&tools, "searchProducts").is_some());
        assert!(find_tool_by_name(&tools, "addToCart").is_some());
        assert!(find_tool_by_name(&tools, "nonexistent").is_none());
    }

    #[test]
    fn test_find_tools_for_opcode() {
        let tools = vec![
            WebMcpTool {
                name: "searchProducts".to_string(),
                description: "Search the product catalog".to_string(),
                parameters: serde_json::json!({}),
                returns: None,
            },
            WebMcpTool {
                name: "addToCart".to_string(),
                description: "Add item to shopping cart".to_string(),
                parameters: serde_json::json!({}),
                returns: None,
            },
            WebMcpTool {
                name: "loginUser".to_string(),
                description: "Log in a user".to_string(),
                parameters: serde_json::json!({}),
                returns: None,
            },
        ];

        let nav_tools = find_tools_for_opcode(&tools, 0x01);
        assert_eq!(nav_tools.len(), 1);
        assert_eq!(nav_tools[0].name, "searchProducts");

        let commerce_tools = find_tools_for_opcode(&tools, 0x02);
        assert_eq!(commerce_tools.len(), 1);
        assert_eq!(commerce_tools[0].name, "addToCart");

        let auth_tools = find_tools_for_opcode(&tools, 0x04);
        assert_eq!(auth_tools.len(), 1);
        assert_eq!(auth_tools[0].name, "loginUser");
    }

    #[test]
    fn test_webmcp_result_deserialization() {
        let json = r#"{"success": true, "value": {"items": 3}, "error": null}"#;
        let result: WebMcpResult = serde_json::from_str(json).unwrap();
        assert!(result.success);
        assert_eq!(result.value["items"], 3);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_webmcp_tool_deserialization() {
        let json = r#"[
            {"name": "search", "description": "Search products", "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}, "returns": null}
        ]"#;
        let tools: Vec<WebMcpTool> = serde_json::from_str(json).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "search");
    }
}
