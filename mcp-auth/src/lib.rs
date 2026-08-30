#![forbid(unsafe_code)]
//! OAuth 2.1 **resource server** helpers for MCP hosts.
//!
//! Provides RFC 9728 protected-resource metadata, RFC 8414 authorization-server
//! metadata (proxied from the external AS), RFC 7662 token introspection for
//! opaque Bearer access tokens, and bearer middleware for nesting under an
//! Axum host (e.g. skill-master). Authorization (login, consent, token issuance)
//! is delegated to an external authorization server such as Zitadel.

pub mod config;
pub mod oauth;

pub use config::McpAuthConfig;
