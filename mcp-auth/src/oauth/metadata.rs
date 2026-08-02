use rmcp::transport::auth::AuthorizationMetadata;
use serde::Serialize;

use crate::config::McpAuthConfig;

/// RFC 9728 OAuth 2.0 Protected Resource Metadata.
#[derive(Debug, Clone, Serialize)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes_supported: Option<Vec<String>>,
}

impl ProtectedResourceMetadata {
    pub fn from_config(config: &McpAuthConfig) -> Self {
        Self {
            resource: config.resource_url(),
            authorization_servers: vec![config.issuer_url()],
            scopes_supported: Some(vec![config.scope.clone()]),
        }
    }
}

pub fn authorization_metadata(config: &McpAuthConfig) -> AuthorizationMetadata {
    let base = config.issuer_url();
    let mut meta = AuthorizationMetadata::default();

    meta.authorization_endpoint = format!("{base}/oauth/authorize");
    meta.token_endpoint = format!("{base}/oauth/token");
    meta.registration_endpoint = Some(format!("{base}/oauth/register"));
    meta.scopes_supported = Some(vec![config.scope.clone()]);
    meta.response_types_supported = Some(vec!["code".into()]);
    meta.code_challenge_methods_supported = Some(vec!["S256".into()]);
    meta.issuer = Some(base.clone());

    meta.additional_fields.insert(
        "grant_types_supported".into(),
        serde_json::json!(["authorization_code", "refresh_token"]),
    );
    meta.additional_fields.insert(
        "token_endpoint_auth_methods_supported".into(),
        serde_json::json!(["none"]),
    );
    meta.additional_fields.insert(
        "client_id_metadata_document_supported".into(),
        serde_json::json!(true),
    );

    meta
}

/// Build the `WWW-Authenticate` challenge required by MCP authorization discovery.
pub fn www_authenticate_challenge(config: &McpAuthConfig) -> String {
    format!(
        r#"Bearer realm="mcp", resource_metadata="{}", scope="{}""#,
        config.protected_resource_metadata_url(),
        config.scope,
    )
}

/// Returns true when `candidate` matches the configured canonical MCP resource URI.
pub fn resource_matches(config: &McpAuthConfig, candidate: &str) -> bool {
    normalize_resource_uri(candidate)
        == normalize_resource_uri(&config.resource_url())
}

fn normalize_resource_uri(uri: &str) -> String {
    uri.trim_end_matches('/').to_lowercase()
}
