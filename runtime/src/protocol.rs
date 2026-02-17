//! Protocol message parsing and formatting for the Cortex socket protocol.
//!
//! Messages are newline-delimited JSON over Unix domain socket.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Protocol methods supported by Cortex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    Handshake,
    Map,
    Query,
    Pathfind,
    Refresh,
    Act,
    Watch,
    Perceive,
    Status,
}

impl Method {
    /// Parse a method name string into a Method enum.
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "handshake" => Ok(Self::Handshake),
            "map" => Ok(Self::Map),
            "query" => Ok(Self::Query),
            "pathfind" => Ok(Self::Pathfind),
            "refresh" => Ok(Self::Refresh),
            "act" => Ok(Self::Act),
            "watch" => Ok(Self::Watch),
            "perceive" => Ok(Self::Perceive),
            "status" => Ok(Self::Status),
            _ => bail!("unknown method: {s}"),
        }
    }
}

/// A parsed protocol request.
#[derive(Debug)]
pub struct Request {
    pub id: String,
    pub method: Method,
    pub params: Value,
}

/// Parse a JSON request line into (id, method, params).
pub fn parse_request(json: &str) -> Result<Request> {
    let v: Value = serde_json::from_str(json)?;

    let id = v
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let method_str = v
        .get("method")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'method' field"))?;

    let method = Method::from_str(method_str)?;

    let params = v.get("params").cloned().unwrap_or(Value::Object(Default::default()));

    Ok(Request { id, method, params })
}

/// Format a successful response as JSON string (newline-terminated).
pub fn format_response(id: &str, result: Value) -> String {
    let resp = serde_json::json!({
        "id": id,
        "result": result,
    });
    format!("{}\n", resp)
}

/// Format an error response as JSON string (newline-terminated).
pub fn format_error(id: &str, code: &str, message: &str) -> String {
    let resp = serde_json::json!({
        "id": id,
        "error": {
            "code": code,
            "message": message,
        },
    });
    format!("{}\n", resp)
}

/// Handshake response.
#[derive(Debug, Serialize, Deserialize)]
pub struct HandshakeResult {
    pub server_version: String,
    pub protocol_version: u16,
    pub compatible: bool,
}

/// Status response.
#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResult {
    pub version: String,
    pub uptime_s: u64,
    pub maps_cached: u32,
    pub pool: PoolStatus,
    pub cache_mb: u32,
}

/// Pool status info.
#[derive(Debug, Serialize, Deserialize)]
pub struct PoolStatus {
    pub active: u32,
    pub max: u32,
    pub memory_mb: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_handshake_request() {
        let json = r#"{"id": "r1", "method": "handshake", "params": {"client_version": "0.1.0", "protocol_version": 1}}"#;
        let req = parse_request(json).unwrap();
        assert_eq!(req.id, "r1");
        assert_eq!(req.method, Method::Handshake);
    }

    #[test]
    fn test_parse_status_request() {
        let json = r#"{"id": "s1", "method": "status", "params": {}}"#;
        let req = parse_request(json).unwrap();
        assert_eq!(req.id, "s1");
        assert_eq!(req.method, Method::Status);
    }

    #[test]
    fn test_parse_unknown_method() {
        let json = r#"{"id": "x", "method": "foobar", "params": {}}"#;
        assert!(parse_request(json).is_err());
    }

    #[test]
    fn test_format_response() {
        let resp = format_response("r1", serde_json::json!({"ok": true}));
        let parsed: Value = serde_json::from_str(resp.trim()).unwrap();
        assert_eq!(parsed["id"], "r1");
        assert_eq!(parsed["result"]["ok"], true);
    }

    #[test]
    fn test_format_error() {
        let resp = format_error("r2", "E_INVALID_METHOD", "unknown method");
        let parsed: Value = serde_json::from_str(resp.trim()).unwrap();
        assert_eq!(parsed["id"], "r2");
        assert_eq!(parsed["error"]["code"], "E_INVALID_METHOD");
    }
}
