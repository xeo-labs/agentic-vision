//! Transport layer for MCP communication.

pub mod framing;
#[cfg(feature = "sse")]
pub mod sse;
pub mod stdio;

#[cfg(feature = "sse")]
pub use sse::SseTransport;
pub use stdio::StdioTransport;
