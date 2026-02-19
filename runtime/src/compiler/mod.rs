//! Web Compiler — transforms SiteMaps into typed APIs with auto-generated clients.
//!
//! The compiler analyzes a mapped website's structured data, infers typed data models,
//! discovers relationships between models, compiles HTTP actions into typed methods,
//! and generates client code in Python, TypeScript, OpenAPI, GraphQL, and MCP formats.

pub mod actions;
pub mod codegen;
pub mod codegen_graphql;
pub mod codegen_mcp;
pub mod codegen_openapi;
pub mod codegen_python;
pub mod codegen_typescript;
pub mod models;
pub mod relationships;
pub mod schema;
pub mod unifier;
