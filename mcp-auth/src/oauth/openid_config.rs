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

/// Subset of OpenID Provider Metadata needed by this resource server
/// and by RFC 8414 authorization-server metadata advertised to MCP clients.
#[derive(Debug, Clone)]
pub struct OpenIdConfiguration {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub introspection_endpoint: String,
    pub registration_endpoint: Option<String>,
    pub jwks_uri: Option<String>,
    pub scopes_supported: Option<Vec<String>>,
    pub response_types_supported: Option<Vec<String>>,
    pub grant_types_supported: Option<Vec<String>>,
    pub code_challenge_methods_supported: Option<Vec<String>>,
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

/// Raw OIDC discovery document (subset).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OpenIdConfigurationDocument {
    pub issuer: String,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub introspection_endpoint: Option<String>,
    pub registration_endpoint: Option<String>,
    pub jwks_uri: Option<String>,
    pub scopes_supported: Option<Vec<String>>,
    pub response_types_supported: Option<Vec<String>>,
    pub grant_types_supported: Option<Vec<String>>,
    pub code_challenge_methods_supported: Option<Vec<String>>,
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

fn optional_url(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn required_url(
    value: Option<&str>,
    field: &str,
) -> Result<String, DiscoveryError> {
    optional_url(value).ok_or_else(|| {
        format!("OIDC discovery document missing {field}").into()
    })
}

/// Validate a parsed OIDC discovery document against `trusted_issuer`.
pub(crate) fn openid_configuration_from_document(
    document: OpenIdConfigurationDocument,
    trusted_issuer: &str,
) -> Result<OpenIdConfiguration, DiscoveryError> {
    let discovered = document.issuer.trim_end_matches('/');
    let trusted = trusted_issuer.trim_end_matches('/');
    if discovered != trusted {
        return Err(format!(
            "OIDC issuer mismatch: discovery returned {discovered}, expected {trusted}"
        )
        .into());
    }

    Ok(OpenIdConfiguration {
        issuer: discovered.to_string(),
        authorization_endpoint: required_url(
            document.authorization_endpoint.as_deref(),
            "authorization_endpoint",
        )?,
        token_endpoint: required_url(
            document.token_endpoint.as_deref(),
            "token_endpoint",
        )?,
        introspection_endpoint: required_url(
            document.introspection_endpoint.as_deref(),
            "introspection_endpoint",
        )?,
        registration_endpoint: optional_url(
            document.registration_endpoint.as_deref(),
        ),
        jwks_uri: optional_url(document.jwks_uri.as_deref()),
        scopes_supported: document.scopes_supported,
        response_types_supported: document.response_types_supported,
        grant_types_supported: document.grant_types_supported,
        code_challenge_methods_supported: document
            .code_challenge_methods_supported,
        token_endpoint_auth_methods_supported: document
            .token_endpoint_auth_methods_supported,
    })
}

/// Fetch and validate OpenID Provider Metadata for `trusted_issuer`.
pub async fn fetch_openid_configuration(
    client: &reqwest::Client,
    trusted_issuer: &str,
) -> Result<OpenIdConfiguration, DiscoveryError> {
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

    openid_configuration_from_document(document, trusted_issuer)
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
            issuer = %cfg.issuer,
            authorization_endpoint = %cfg.authorization_endpoint,
            token_endpoint = %cfg.token_endpoint,
            introspection_endpoint = %cfg.introspection_endpoint,
            registration_endpoint = ?cfg.registration_endpoint,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> OpenIdConfigurationDocument {
        OpenIdConfigurationDocument {
            issuer: "https://auth.example.com".into(),
            authorization_endpoint: Some(
                "https://auth.example.com/oauth/v2/authorize".into(),
            ),
            token_endpoint: Some(
                "https://auth.example.com/oauth/v2/token".into(),
            ),
            introspection_endpoint: Some(
                "https://auth.example.com/oauth/v2/introspect".into(),
            ),
            registration_endpoint: Some(
                "https://auth.example.com/oauth/v2/register".into(),
            ),
            jwks_uri: Some("https://auth.example.com/oauth/v2/keys".into()),
            scopes_supported: Some(vec!["openid".into()]),
            response_types_supported: Some(vec!["code".into()]),
            grant_types_supported: Some(vec!["authorization_code".into()]),
            code_challenge_methods_supported: Some(vec!["S256".into()]),
            token_endpoint_auth_methods_supported: Some(vec![
                "client_secret_basic".into(),
            ]),
        }
    }

    #[test]
    fn accepts_matching_issuer_and_required_endpoints() {
        let cfg = openid_configuration_from_document(
            document(),
            "https://auth.example.com/",
        )
        .unwrap();
        assert_eq!(cfg.issuer, "https://auth.example.com");
        assert_eq!(
            cfg.registration_endpoint.as_deref(),
            Some("https://auth.example.com/oauth/v2/register")
        );
    }

    #[test]
    fn rejects_issuer_mismatch() {
        let err = openid_configuration_from_document(
            document(),
            "https://other.example.com",
        )
        .unwrap_err();
        assert!(err.to_string().contains("issuer mismatch"));
    }

    #[test]
    fn rejects_missing_authorization_endpoint() {
        let mut doc = document();
        doc.authorization_endpoint = Some("  ".into());
        let err =
            openid_configuration_from_document(doc, "https://auth.example.com")
                .unwrap_err();
        assert!(err.to_string().contains("authorization_endpoint"));
    }
}
