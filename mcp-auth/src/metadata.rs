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
            authorization_servers: vec![config.issuer().to_string()],
            scopes_supported: Some(vec![config.scope.clone()]),
        }
    }
}

/// Build the `WWW-Authenticate` challenge required by MCP authorization discovery.
pub fn www_authenticate_challenge(config: &McpAuthConfig) -> String {
    format!(
        r#"Bearer realm="mcp", resource_metadata="{}", scope="{}""#,
        config.protected_resource_metadata_url(),
        config.scope,
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
scope = "mcp"
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
}
