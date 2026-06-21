mod validate;

use std::net::SocketAddr;

use serde::Deserialize;

/// HTTP server and OAuth settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpServerConfig {
    /// Bind address, e.g. "0.0.0.0:9191"
    #[serde(deserialize_with = "validate::deserialize_addr")]
    pub addr: String,
    /// Public base URL used in OAuth metadata (no trailing slash), e.g. "http://127.0.0.1:9191"
    #[serde(deserialize_with = "validate::deserialize_public_url")]
    pub public_url: String,
    /// Username shown on the OAuth consent page (single-owner server).
    #[serde(deserialize_with = "validate::deserialize_username")]
    pub username: String,
    /// Password checked on the OAuth consent page.
    #[serde(deserialize_with = "validate::deserialize_password")]
    pub password: String,
    /// Access-token lifetime in seconds (default: 3600).
    #[serde(
        default = "validate::default_token_ttl",
        deserialize_with = "validate::deserialize_token_ttl_secs"
    )]
    pub token_ttl_secs: u64,
    /// OAuth scope advertised to clients (default: "mcp").
    #[serde(
        default = "validate::default_scope",
        deserialize_with = "validate::deserialize_scope"
    )]
    pub scope: String,
    /// Allowed browser origins for Streamable HTTP Origin validation.
    #[serde(
        default,
        deserialize_with = "validate::deserialize_allowed_origins"
    )]
    pub allowed_origins: Vec<String>,
}

impl McpServerConfig {
    pub fn socket_addr(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        self.addr.parse()
    }

    pub fn token_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.token_ttl_secs)
    }

    /// OAuth Authorization Server issuer URL (public base, no trailing slash).
    pub fn issuer_url(&self) -> String {
        self.public_url.clone()
    }

    /// Canonical MCP resource URI (RFC 8707 / RFC 9728).
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

    /// Host values accepted in inbound `Host` headers for Streamable HTTP.
    pub fn allowed_hosts(&self) -> Vec<String> {
        let mut hosts = Vec::new();
        if let Ok(url) = url::Url::parse(&self.public_url) {
            let mut authority = url.host_str().unwrap_or_default().to_string();
            if let Some(port) = url.port() {
                authority.push(':');
                authority.push_str(&port.to_string());
            }
            hosts.push(authority);
        }
        if let Some((host, port)) = self.addr.rsplit_once(':') {
            if host != "0.0.0.0" {
                hosts.push(format!("{host}:{port}"));
                hosts.push(host.to_string());
            }
        }
        hosts.push("localhost".into());
        hosts.push("127.0.0.1".into());
        hosts.sort();
        hosts.dedup();
        hosts
    }
}