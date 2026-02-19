//! Transport layer for MCP communication.

pub mod framing;
pub mod stdio;

pub use stdio::StdioTransport;
