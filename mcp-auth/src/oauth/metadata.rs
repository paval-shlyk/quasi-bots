use serde::Serialize;

use crate::config::McpAuthConfig;
use crate::oauth::openid_config::OpenIdConfiguration;

/// RFC 9728 OAuth 2.0 Protected Resource Metadata.
#[derive(Debug, Clone, Serialize)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes_supported: Option<Vec<String>>,
}

/// RFC 8414 OAuth 2.0 Authorization Server Metadata advertised to MCP clients.
///
/// Endpoints and `issuer` come from the external AS (Zitadel OIDC discovery).
/// MCP-required values (`S256`, public-client `none`, RS scopes) are overlaid
/// so agents that only read this document still get a usable registration
/// and PKCE configuration.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuthorizationServerMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub introspection_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes_supported: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_types_supported: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_types_supported: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

impl AuthorizationServerMetadata {
    /// Build RFC 8414 metadata from cached OIDC discovery + RS policy.
    pub fn from_openid(
        oidc: &OpenIdConfiguration,
        config: &McpAuthConfig,
    ) -> Self {
        Self {
            issuer: oidc.issuer.clone(),
            authorization_endpoint: oidc.authorization_endpoint.clone(),
            token_endpoint: oidc.token_endpoint.clone(),
            registration_endpoint: oidc.registration_endpoint.clone(),
            jwks_uri: oidc.jwks_uri.clone(),
            introspection_endpoint: Some(oidc.introspection_endpoint.clone()),
            scopes_supported: Some(config.supported_scopes.clone()),
            response_types_supported: Some(with_required(
                oidc.response_types_supported.clone().unwrap_or_default(),
                &["code"],
            )),
            grant_types_supported: Some(with_required(
                oidc.grant_types_supported.clone().unwrap_or_default(),
                &["authorization_code", "refresh_token"],
            )),
            code_challenge_methods_supported: Some(with_required(
                oidc.code_challenge_methods_supported
                    .clone()
                    .unwrap_or_default(),
                &["S256"],
            )),
            token_endpoint_auth_methods_supported: Some(with_required(
                oidc.token_endpoint_auth_methods_supported
                    .clone()
                    .unwrap_or_default(),
                &["none"],
            )),
        }
    }
}

fn with_required(mut items: Vec<String>, required: &[&str]) -> Vec<String> {
    for need in required {
        if !items.iter().any(|have| have == need) {
            items.push((*need).to_string());
        }
    }
    items
}

impl ProtectedResourceMetadata {
    pub fn from_config(config: &McpAuthConfig) -> Self {
        Self {
            resource: config.resource_url(),
            authorization_servers: vec![config.trusted_issuer().to_string()],
            scopes_supported: Some(config.supported_scopes.clone()),
        }
    }
}

/// Build the `WWW-Authenticate` challenge required by MCP authorization discovery.
pub fn www_authenticate_challenge(config: &McpAuthConfig) -> String {
    format!(
        r#"Bearer realm="mcp", resource_metadata="{}", scope="{}""#,
        config.protected_resource_metadata_url(),
        config.supported_scopes_param(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> McpAuthConfig {
        toml::from_str(
            r#"
public_url = "http://127.0.0.1:8080"
authorization_server = "https://auth.example.com/realms/mcp"
supported_scopes = ["mcp"]
introspection_client_id = "rs-api-client"
"#,
        )
        .unwrap()
    }

    #[test]
    fn prm_points_at_external_as() {
        let meta = ProtectedResourceMetadata::from_config(&sample_config());
        assert_eq!(meta.resource, "http://127.0.0.1:8080/mcp");
        assert_eq!(
            meta.authorization_servers,
            vec!["https://auth.example.com/realms/mcp"]
        );
        assert_eq!(meta.scopes_supported.as_ref().unwrap(), &vec!["mcp"]);
    }

    #[test]
    fn challenge_includes_resource_metadata() {
        let c = www_authenticate_challenge(&sample_config());
        assert!(c.contains("resource_metadata="));
        assert!(c.contains("/.well-known/oauth-protected-resource/mcp"));
        assert!(c.contains(r#"scope="mcp""#));
    }

    fn discovered() -> OpenIdConfiguration {
        OpenIdConfiguration {
            issuer: "https://auth.example.com/realms/mcp".into(),
            authorization_endpoint:
                "https://auth.example.com/oauth/v2/authorize".into(),
            token_endpoint: "https://auth.example.com/oauth/v2/token".into(),
            introspection_endpoint:
                "https://auth.example.com/oauth/v2/introspect".into(),
            registration_endpoint: Some(
                "https://auth.example.com/oauth/v2/register".into(),
            ),
            jwks_uri: Some("https://auth.example.com/oauth/v2/keys".into()),
            scopes_supported: Some(vec!["openid".into(), "profile".into()]),
            response_types_supported: Some(vec!["id_token".into()]),
            grant_types_supported: Some(vec!["authorization_code".into()]),
            code_challenge_methods_supported: Some(vec!["plain".into()]),
            token_endpoint_auth_methods_supported: Some(vec![
                "client_secret_basic".into(),
            ]),
        }
    }

    #[test]
    fn rfc8414_keeps_external_issuer_and_endpoints() {
        let meta = AuthorizationServerMetadata::from_openid(
            &discovered(),
            &sample_config(),
        );
        assert_eq!(meta.issuer, "https://auth.example.com/realms/mcp");
        assert_eq!(
            meta.authorization_endpoint,
            "https://auth.example.com/oauth/v2/authorize"
        );
        assert_eq!(
            meta.token_endpoint,
            "https://auth.example.com/oauth/v2/token"
        );
        assert_eq!(
            meta.registration_endpoint.as_deref(),
            Some("https://auth.example.com/oauth/v2/register")
        );
        assert_eq!(
            meta.introspection_endpoint.as_deref(),
            Some("https://auth.example.com/oauth/v2/introspect")
        );
    }

    #[test]
    fn rfc8414_advertises_rs_scopes_not_as_catalog() {
        let meta = AuthorizationServerMetadata::from_openid(
            &discovered(),
            &sample_config(),
        );
        assert_eq!(meta.scopes_supported, Some(vec!["mcp".into()]));
    }

    #[test]
    fn rfc8414_overlays_mcp_required_values() {
        let meta = AuthorizationServerMetadata::from_openid(
            &discovered(),
            &sample_config(),
        );
        assert_eq!(
            meta.response_types_supported,
            Some(vec!["id_token".into(), "code".into()])
        );
        assert_eq!(
            meta.grant_types_supported,
            Some(vec!["authorization_code".into(), "refresh_token".into()])
        );
        assert_eq!(
            meta.code_challenge_methods_supported,
            Some(vec!["plain".into(), "S256".into()])
        );
        assert_eq!(
            meta.token_endpoint_auth_methods_supported,
            Some(vec!["client_secret_basic".into(), "none".into()])
        );
    }

    #[test]
    fn rfc8414_omits_registration_endpoint_when_as_does_not_advertise_it() {
        let mut oidc = discovered();
        oidc.registration_endpoint = None;
        let meta =
            AuthorizationServerMetadata::from_openid(&oidc, &sample_config());
        assert!(meta.registration_endpoint.is_none());
        let json = serde_json::to_value(&meta).unwrap();
        assert!(json.get("registration_endpoint").is_none());
    }
}
