pub mod validate;

use serde::Deserialize;

/// MCP resource-server auth configuration.
///
/// The host is an OAuth 2.1 **resource server**. Access tokens are opaque
/// Bearer tokens validated via RFC 7662 introspection against an external
/// authorization server.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpAuthConfig {
    /// Public base URL used in protected-resource metadata (no trailing slash),
    /// e.g. `"http://127.0.0.1:8080"`.
    #[serde(deserialize_with = "validate::deserialize_public_url")]
    pub public_url: String,

    /// External authorization server issuer URL.
    /// Used in RFC 9728 `authorization_servers` and OIDC discovery
    /// (`introspection_endpoint`).
    #[serde(deserialize_with = "validate::deserialize_authorization_server")]
    pub authorization_server: String,

    /// OAuth scope advertised to clients.
    #[serde(
        default = "validate::default_scope",
        deserialize_with = "validate::deserialize_scope"
    )]
    pub scope: String,

    /// Optional token `sub` allowlist. Empty = any subject with an active token.
    #[serde(default, deserialize_with = "validate::deserialize_allowed_subs")]
    pub allowed_subs: Vec<String>,

    /// Allowed browser origins for Streamable HTTP Origin validation.
    #[serde(
        default,
        deserialize_with = "validate::deserialize_allowed_origins"
    )]
    pub allowed_origins: Vec<String>,

    /// Zitadel/Keycloak **API** application client id used by this RS to call
    /// the introspection endpoint (client_secret_basic).
    #[serde(deserialize_with = "validate::deserialize_introspection_client_id")]
    pub introspection_client_id: String,

    /// Introspection API client secret.
    ///
    /// TOML value wins when non-empty; otherwise filled from
    /// `INTROSPECTION_CLIENT_SECRET` at deserialize time (`None` if unset).
    #[serde(
        default = "validate::introspection_client_secret_from_env",
        deserialize_with = "validate::deserialize_introspection_client_secret"
    )]
    pub introspection_client_secret: Option<String>,
}

impl McpAuthConfig {
    /// Canonical MCP resource URI (RFC 8707 / RFC 9728) — expected introspection `aud`.
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

    pub fn trusted_issuer(&self) -> &str {
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
