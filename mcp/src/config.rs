use serde::Deserialize;

/// Top-level configuration loaded from config.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub provider: ProviderConfig,
}

/// HTTP server and OAuth settings.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
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

fn default_token_ttl() -> u64 {
    3600
}

/// DZENGI.com REST API credentials.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub api_secret: String,
}
