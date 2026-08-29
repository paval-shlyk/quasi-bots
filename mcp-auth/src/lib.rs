//! OAuth 2.1 **resource server** helpers for MCP hosts.
//!
//! Provides RFC 9728 protected-resource metadata, JWKS-backed JWT validation,
//! and bearer middleware for nesting under an Axum host (e.g. skill-master).
//! Authorization (login, consent, token issuance) is delegated to an external
//! authorization server such as Keycloak or Zitadel.

pub mod config;
pub mod oauth;

pub use config::McpAuthConfig;
