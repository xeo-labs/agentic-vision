//! MCP protocol handling — JSON-RPC dispatch.

pub mod handler;
pub mod negotiation;
pub mod validator;

pub use handler::ProtocolHandler;
