//! OIDC discovery and cached OpenID Provider configuration.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::RwLock;
use tokio::time::MissedTickBehavior;

use crate::config::McpAuthConfig;

pub type DiscoveryError = Box<dyn std::error::Error + Send + Sync>;

/// Cached OpenID Provider configuration used by the resource server.
pub type CachedOpenIdConfig = Arc<RwLock<Option<OpenIdConfiguration>>>;

/// Subset of OpenID Provider Metadata needed by this resource server.
#[derive(Debug, Clone)]
pub struct OpenIdConfiguration {
    pub introspection_endpoint: String,
}

/// Fetch and validate OpenID Provider Metadata for `trusted_issuer`.
pub async fn fetch_openid_configuration(
    client: &reqwest::Client,
    trusted_issuer: &str,
) -> Result<OpenIdConfiguration, DiscoveryError> {
    #[derive(Debug, Clone, Deserialize)]
    struct OpenIdConfigurationDocument {
        pub issuer: String,
        pub introspection_endpoint: Option<String>,
    }

    let discovery_url =
        format!("{trusted_issuer}/.well-known/openid-configuration");

    let resp =
        client.get(&discovery_url).send().await.map_err(|e| {
            format!("Failed to fetch OpenID configuration: {e}")
        })?;

    let resp = resp
        .error_for_status()
        .map_err(|e| format!("OIDC discovery returned not 2xx: {e}"))?;

    let document: OpenIdConfigurationDocument = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenID configuration: {e}"))?;

    let discovered = document.issuer.trim_end_matches('/');
    let trusted = trusted_issuer.trim_end_matches('/');
    if discovered != trusted {
        return Err(format!(
            "OIDC issuer mismatch: discovery returned {discovered}, expected {trusted}"
        )
        .into());
    }

    let Some(endpoint) = document
        .introspection_endpoint
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Err(
            "OIDC discovery document missing introspection_endpoint".into()
        );
    };

    Ok(OpenIdConfiguration {
        introspection_endpoint: endpoint.to_string(),
    })
}

pub fn spawn_openid_config_refresh_task(
    client: reqwest::Client,
    trusted_issuer: String,
    cache: CachedOpenIdConfig,
    refresh_period: Duration,
) -> tokio::task::JoinHandle<()> {
    assert!(refresh_period.as_millis() > 0);

    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(refresh_period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        interval.tick().await; // first tick completes immediately

        loop {
            interval.tick().await;

            match fetch_openid_configuration(&client, &trusted_issuer).await {
                Ok(fresh) => {
                    *cache.write().await = Some(fresh);
                }
                Err(e) => {
                    // Keep the last good configuration; a transient AS outage
                    // must not wipe discovery and turn every MCP request into 401.
                    tracing::warn!(
                        "Failed to refresh OpenID configuration: {e}"
                    );
                }
            }
        }
    })
}

/// Load initial OpenID configuration and spawn a periodic refresh task.
pub async fn new_cached_openid_configuration(
    config: &McpAuthConfig,
) -> anyhow::Result<CachedOpenIdConfig> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let trusted_issuer = config.trusted_issuer().to_string();

    let initial = fetch_openid_configuration(&client, &trusted_issuer)
        .await
        .inspect_err(|e| {
            tracing::warn!(
                "Failed to load OpenID configuration on startup: {e}"
            )
        })
        .ok();

    if let Some(ref cfg) = initial {
        tracing::info!(
            introspection_endpoint = %cfg.introspection_endpoint,
            "OpenID configuration loaded"
        );
    }

    let cache = Arc::new(RwLock::new(initial));

    let _refresh_task = spawn_openid_config_refresh_task(
        client,
        trusted_issuer,
        cache.clone(),
        Duration::from_secs(60),
    );

    Ok(cache)
}
