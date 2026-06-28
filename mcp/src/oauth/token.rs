//! OAuth token types for the MCP authorization server.
//!
//! # Normative references
//!
//! - [RFC 6749](https://datatracker.ietf.org/doc/html/rfc6749) — OAuth 2.0 Authorization Framework
//!   - [§3.3 Scope](https://datatracker.ietf.org/doc/html/rfc6749#section-3.3)
//!   - [§5.1 Access Token Response](https://datatracker.ietf.org/doc/html/rfc6749#section-5.1)
//!   - [§5.2 Error Response](https://datatracker.ietf.org/doc/html/rfc6749#section-5.2)
//! - [RFC 6750](https://datatracker.ietf.org/doc/html/rfc6750) — Bearer Token Usage (`token_type`)
//! - [RFC 7636](https://datatracker.ietf.org/doc/html/rfc7636) — PKCE (authorization code exchange)
//! - [RFC 8707](https://datatracker.ietf.org/doc/html/rfc8707) — Resource Indicators (`resource` parameter)
//! - [RFC 9728](https://datatracker.ietf.org/doc/html/rfc9728) — Protected Resource Metadata
//! - [MCP Authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)
//! - [SEP-2468](https://modelcontextprotocol.io/seps/2468-recommend-issuer-claim-for-auth.md) — optional `iss` in auth responses

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;

/// In-memory token record. Never serialized to HTTP clients.
#[derive(Clone, Debug)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: String,
    pub issued_at: DateTime<Utc>,
    pub ttl_secs: u64,
    pub scope: Option<String>,
    pub issuer: Option<String>,
    /// Google OIDC `sub` of the owner who approved access.
    pub owner_sub: Option<String>,
}

impl StoredToken {
    pub fn is_expired(&self) -> bool {
        Utc::now()
            .signed_duration_since(self.issued_at)
            .num_seconds()
            >= self.ttl_secs as i64
    }

    /// Build an [RFC 6749 §5.1](https://datatracker.ietf.org/doc/html/rfc6749#section-5.1) response.
    pub fn to_response(&self) -> TokenResponse {
        TokenResponse {
            access_token: self.access_token.clone(),
            token_type: "Bearer".to_string(),
            expires_in: self.ttl_secs,
            refresh_token: Some(self.refresh_token.clone()),
            scope: self.scope.clone(),
            iss: self.issuer.clone(),
        }
    }
}

/// OAuth 2.0 access token response sent from `POST /oauth/token`.
///
/// Uses `expires_in` (seconds) per RFC 6749 — not a JWT-style `exp` claim.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// MCP SEP-2468 optional issuer identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
}

pub fn new_stored_token(
    ttl: Duration,
    scope: Option<String>,
    issuer: Option<String>,
) -> StoredToken {
    StoredToken {
        access_token: format!("mcp-{}", uuid::Uuid::new_v4()),
        refresh_token: format!("refresh-{}", uuid::Uuid::new_v4()),
        issued_at: Utc::now(),
        ttl_secs: ttl.as_secs(),
        scope,
        issuer,
        owner_sub: None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn token_response_matches_rfc6749_shape() {
        let stored = new_stored_token(
            Duration::from_secs(120),
            Some("mcp".into()),
            Some("http://127.0.0.1:9191".into()),
        );
        let json = serde_json::to_value(stored.to_response()).unwrap();
        let obj = json.as_object().unwrap();

        assert!(obj.contains_key("access_token"));
        assert!(obj.contains_key("token_type"));
        assert!(obj.contains_key("expires_in"));
        assert!(obj.contains_key("refresh_token"));
        assert!(!obj.contains_key("exp"));
        assert!(!obj.contains_key("iat"));
        assert_eq!(obj["token_type"], "Bearer");
        assert_eq!(obj["expires_in"], 120);
    }
}

