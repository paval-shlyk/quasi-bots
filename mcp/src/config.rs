use serde::Deserialize;

/// HTTP server and OAuth settings.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    /// Bind address, e.g. "0.0.0.0:9191"
    pub addr: String,
    /// Username shown on the OAuth consent page (single-owner server).
    pub username: String,
    /// Password checked on the OAuth consent page.
    pub password: String,
    /// Access-token lifetime in seconds (default: 3600).
    #[serde(default = "default_token_ttl")]
    pub token_ttl_secs: u64,
}

impl McpServerConfig {
    pub fn token_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.token_ttl_secs)
    }
}

fn default_token_ttl() -> u64 {
    3600
}
