//! OAuth 2.1 Authorization Server for MCP resource servers.
//!
//! Provides RFC 8414 / RFC 9728 metadata, dynamic client registration, Google OIDC owner
//! authentication, and bearer-token middleware for nesting under an Axum host (e.g. skill-master).

pub mod config;
pub mod oauth;

pub use config::McpAuthConfig;