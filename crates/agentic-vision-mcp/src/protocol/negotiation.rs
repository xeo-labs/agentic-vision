//! MCP capability negotiation during initialization.

use crate::types::{
    ClientCapabilities, InitializeParams, InitializeResult, McpResult, MCP_VERSION,
};

/// Stored client capabilities after negotiation.
#[derive(Debug, Clone, Default)]
pub struct NegotiatedCapabilities {
    pub client: ClientCapabilities,
    pub initialized: bool,
}

impl NegotiatedCapabilities {
    pub fn negotiate(&mut self, params: InitializeParams) -> McpResult<InitializeResult> {
        if params.protocol_version != MCP_VERSION {
            tracing::warn!(
                "Client requested protocol version {}, server supports {}. Proceeding with server version.",
                params.protocol_version,
                MCP_VERSION
            );
        }

        self.client = params.capabilities;

        tracing::info!(
            "Initialized with client: {} v{}",
            params.client_info.name,
            params.client_info.version
        );

        Ok(InitializeResult::default_result())
    }

    pub fn mark_initialized(&mut self) -> McpResult<()> {
        self.initialized = true;
        tracing::info!("MCP handshake complete");
        Ok(())
    }
}
