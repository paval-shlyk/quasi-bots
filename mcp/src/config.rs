mod validate;

use std::net::SocketAddr;

use serde::Deserialize;

/// Google OIDC settings for owner authentication.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoogleAuthConfig {
    /// Google OAuth client ID (Web application).
    #[serde(deserialize_with = "validate::deserialize_google_client_id")]
    pub client_id: String,
    /// Optional client secret in config. Prefer `GOOGLE_CLIENT_SECRET` env in production.
    #[serde(default)]
    pub client_secret: Option<String>,
    /// Allowlisted Google `sub` values (owner accounts).
    #[serde(
        default,
        deserialize_with = "validate::deserialize_allowed_google_subs"
    )]
    pub allowed_google_subs: Vec<String>,
}

/// Pre-approved owner entry (alternative to `allowed_google_subs`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedOwner {
    pub sub: String,
    #[allow(dead_code)]
    pub label: Option<String>,
}

/// Owner authentication settings (loaded from config.toml).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    pub google: GoogleAuthConfig,
    #[serde(default)]
    pub allowed_owners: Vec<AllowedOwner>,
}

impl AuthConfig {
    /// Merged allowlist from `allowed_google_subs` and `allowed_owners`.
    pub fn allowed_subs(&self) -> Vec<String> {
        let mut subs = self.google.allowed_google_subs.clone();
        for owner in &self.allowed_owners {
            if !subs.contains(&owner.sub) {
                subs.push(owner.sub.clone());
            }
        }
        subs
    }

    /// Resolve Google client secret from config or `GOOGLE_CLIENT_SECRET`.
    pub fn resolve_client_secret(&self) -> Option<String> {
        if let Some(secret) = &self.google.client_secret {
            let trimmed = secret.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        std::env::var("GOOGLE_CLIENT_SECRET")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn google_redirect_uri(&self, public_url: &str) -> String {
        format!("{public_url}/oauth/google/callback")
    }

    pub fn google_configured(&self) -> bool {
        !self.google.client_id.starts_with("REPLACE")
            && self.resolve_client_secret().is_some()
    }

    pub fn owner_allowed(&self, sub: &str) -> bool {
        let allowlist = self.allowed_subs();
        allowlist.is_empty() || allowlist.iter().any(|s| s == sub)
    }

    pub fn dev_allowlist_mode(&self) -> bool {
        self.allowed_subs().is_empty()
    }
}

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
    pub auth: AuthConfig,
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
    /// Streamable HTTP session mode. `false` allows stateless per-request tool calls.
    #[serde(default = "validate::default_stateful_mode")]
    pub stateful_mode: bool,
    /// Return `application/json` directly instead of SSE (only when `stateful_mode = false`).
    #[serde(default = "validate::default_json_response")]
    pub json_response: bool,
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
