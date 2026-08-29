pub mod validate;

use serde::Deserialize;

/// MCP resource-server auth configuration.
///
/// The host application is an OAuth 2.1 **resource server**. Authorization
/// (login, consent, token minting) is delegated to an external authorization
/// server such as Keycloak or Zitadel.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpAuthConfig {
    /// Public base URL used in protected-resource metadata (no trailing slash),
    /// e.g. `"http://127.0.0.1:8080"`.
    #[serde(deserialize_with = "validate::deserialize_public_url")]
    pub public_url: String,

    /// External authorization server issuer URL.
    /// Used in RFC 9728 `authorization_servers` and for OIDC discovery / JWKS.
    #[serde(deserialize_with = "validate::deserialize_authorization_server")]
    pub authorization_server: String,

    /// OAuth scope advertised to clients.
    #[serde(
        default = "validate::default_scope",
        deserialize_with = "validate::deserialize_scope"
    )]
    pub scope: String,

    /// Optional JWT `sub` allowlist. Empty = any subject with a valid token.
    #[serde(default, deserialize_with = "validate::deserialize_allowed_subs")]
    pub allowed_subs: Vec<String>,

    /// Allowed browser origins for Streamable HTTP Origin validation.
    #[serde(
        default,
        deserialize_with = "validate::deserialize_allowed_origins"
    )]
    pub allowed_origins: Vec<String>,
}

impl McpAuthConfig {
    /// Canonical MCP resource URI (RFC 8707 / RFC 9728) — also the expected JWT `aud`.
    pub fn resource_url(&self) -> String {
        format!("{}/mcp", self.public_url)
    }

    /// RFC 9728 protected-resource metadata document URL (path-scoped).
    pub fn protected_resource_metadata_url(&self) -> String {
        format!(
            "{}/.well-known/oauth-protected-resource/mcp",
            self.public_url
        )
    }

    pub fn issuer(&self) -> &str {
        self.authorization_server.as_str()
    }

    /// Whether the given subject is allowed. Empty allowlist accepts any `sub`.
    pub fn subject_allowed(&self, sub: &str) -> bool {
        self.allowed_subs.is_empty()
            || self.allowed_subs.iter().any(|s| s == sub)
    }

    /// Host values accepted in inbound `Host` headers for Streamable HTTP.
    pub fn allowed_hosts(&self) -> Vec<String> {
        let mut hosts = Vec::new();
        if let Ok(url) = url::Url::parse(&self.public_url)
            && let Some(host) = url.host_str()
        {
            let mut authority = host.to_string();
            if let Some(port) = url.port() {
                authority.push(':');
                authority.push_str(&port.to_string());
            }
            hosts.push(authority);
            hosts.push(host.to_string());
        }
        hosts.push("localhost".into());
        hosts.push("127.0.0.1".into());
        hosts.sort();
        hosts.dedup();
        hosts
    }
}
