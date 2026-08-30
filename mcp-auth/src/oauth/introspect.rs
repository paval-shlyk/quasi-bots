//! RFC 7662 OAuth 2.0 Token Introspection for opaque Bearer access tokens.
//!
//! # Normative references
//!
//! - [RFC 7662](https://datatracker.ietf.org/doc/html/rfc7662) — Token Introspection
//! - [RFC 6750](https://datatracker.ietf.org/doc/html/rfc6750) — Bearer Token Usage
//! - [MCP Authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use crate::{McpAuthConfig, oauth::openid_config::CachedOpenIdConfig};

#[derive(Debug, Clone)]
pub struct ValidatedToken {
    pub sub: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("OpenID configuration is not available")]
    OpenIdConfigUnavailable,
    #[error("introspection request failed: {0}")]
    Transport(String),
    #[error("token inactive or revoked")]
    Inactive,
    #[error("token claims invalid: {0}")]
    Invalid(String),
    #[error("subject not allowlisted")]
    SubjectNotAllowed,
    #[error("required scope missing")]
    InsufficientScope,
    #[error("required role missing")]
    InsufficientRole,
}

/// RFC 7662 introspection response (subset + flatten for vendor fields).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // fields retained for serde completeness / future policy
pub struct IntrospectionResponse {
    pub active: bool,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub exp: Option<i64>,
    #[serde(default)]
    pub iat: Option<i64>,
    #[serde(default)]
    pub nbf: Option<i64>,
    #[serde(default)]
    pub sub: Option<String>,
    /// String or array of strings (AS-dependent).
    #[serde(default)]
    pub aud: Option<Value>,
    #[serde(default)]
    pub iss: Option<String>,
    #[serde(default)]
    pub jti: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Validates opaque Bearer tokens via the AS introspection endpoint.
pub struct TokenValidator<'a> {
    pub openid_config: CachedOpenIdConfig,
    pub mcp_auth_config: &'a McpAuthConfig,
    pub client: &'a reqwest::Client,

    pub client_id: &'a str,
    pub client_secret: &'a str,
}

impl TokenValidator<'_> {
    /// Validate a Bearer access token (opaque) via RFC 7662 introspection.
    pub async fn validate(
        &self,
        token: &str,
    ) -> Result<ValidatedToken, AuthError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(AuthError::Invalid("empty bearer token".into()));
        }

        let endpoint = self
            .openid_config
            .read()
            .await
            .as_ref()
            .map(|c| c.introspection_endpoint.clone())
            .ok_or(AuthError::OpenIdConfigUnavailable)?;

        let resp = self.introspect(&endpoint, token).await?;
        tracing::debug!("Introspection response: {resp:?}");
        apply_introspection_claims(&resp, self.mcp_auth_config)
    }

    async fn introspect(
        &self,
        introspection_endpoint: &str,
        token: &str,
    ) -> Result<IntrospectionResponse, AuthError> {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("token", token)
            .append_pair("token_type_hint", "access_token")
            .finish();

        let resp = self
            .client
            .post(introspection_endpoint)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .basic_auth(self.client_id, Some(self.client_secret))
            .body(body)
            .send()
            .await
            .map_err(|e| AuthError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AuthError::Transport(format!(
                "introspection HTTP {}",
                resp.status()
            )));
        }

        resp.json::<IntrospectionResponse>()
            .await
            .map_err(|e| AuthError::Transport(format!("invalid JSON: {e}")))
    }
}

/// Apply RS authorization policy to an introspection response.
pub fn apply_introspection_claims(
    response: &IntrospectionResponse,
    config: &McpAuthConfig,
) -> Result<ValidatedToken, AuthError> {
    if !response.active {
        tracing::warn!(
            iss = ?response.iss,
            client_id = ?response.client_id,
            "introspection returned active=false"
        );
        return Err(AuthError::Inactive);
    }

    let Some(iss) = response.iss.as_ref() else {
        return Err(AuthError::Invalid("missing issuer field".into()));
    };

    let iss = iss.trim_end_matches('/');
    let expected = config.authorization_server.trim_end_matches('/');

    if iss != expected {
        return Err(AuthError::Invalid(format!(
            "iss mismatch: got {iss}, expected {expected}"
        )));
    }

    let sub =
        response
            .sub
            .clone()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AuthError::Invalid("introspection response missing sub".into())
            })?;

    if !subject_allowed(&config.allowed_subs, &sub) {
        return Err(AuthError::SubjectNotAllowed);
    }

    if !scope_satisfies(
        response.scope.as_deref(),
        &config.required_introspection_scopes,
    ) {
        return Err(AuthError::InsufficientScope);
    }

    if !roles_satisfy(&response.extra, &config.required_roles) {
        return Err(AuthError::InsufficientRole);
    }

    Ok(ValidatedToken { sub })
}

fn subject_allowed(allowed_subs: &[String], sub: &str) -> bool {
    allowed_subs.is_empty() || allowed_subs.iter().any(|s| s == sub)
}

fn scope_satisfies(granted: Option<&str>, required: &[String]) -> bool {
    if required.is_empty() {
        return true;
    }
    let Some(granted) = granted else {
        return false;
    };
    let granted: Vec<&str> = granted.split_whitespace().collect();
    required
        .iter()
        .all(|need| granted.iter().any(|have| *have == need))
}

/// Zitadel puts project roles in vendor claims, not in `scope`.
fn roles_satisfy(extra: &HashMap<String, Value>, required: &[String]) -> bool {
    if required.is_empty() {
        return true;
    }

    let granted = granted_zitadel_roles(extra);
    required
        .iter()
        .all(|need| granted.iter().any(|have| *have == need))
}

fn granted_zitadel_roles(extra: &HashMap<String, Value>) -> Vec<&str> {
    extra
        .iter()
        .filter(|(key, _)| is_zitadel_roles_claim(key))
        .filter_map(|(_, value)| value.as_object())
        .flat_map(|roles| roles.keys().map(String::as_str))
        .collect()
}

fn is_zitadel_roles_claim(key: &str) -> bool {
    if key == "urn:zitadel:iam:org:project:roles" {
        return true;
    }

    let Some(rest) = key.strip_prefix("urn:zitadel:iam:org:project:") else {
        return false;
    };

    let Some(project_id) = rest.strip_suffix(":roles") else {
        return false;
    };
    !project_id.is_empty() && !project_id.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_config() -> McpAuthConfig {
        toml::from_str(
            r#"
public_url = "http://127.0.0.1:8080"
authorization_server = "https://auth.example.com"
supported_scopes = ["mcp"]
required_introspection_scopes = [
    "openid",
    "offline_access",
    "urn:zitadel:iam:org:project:id:my_client_id:aud",
]
introspection_client_id = "rs-api-client"
"#,
        )
        .unwrap()
    }

    fn active_response() -> IntrospectionResponse {
        serde_json::from_value(json!({
            "active": true,
            "sub": "user-1",
            "iss": "https://auth.example.com",
            "aud": "http://127.0.0.1:8080/mcp",
            "scope": "openid urn:zitadel:iam:org:project:id:my_client_id:aud offline_access",
            "exp": 9999999999_i64
        }))
        .unwrap()
    }

    #[test]
    fn accepts_valid_active_token() {
        let v =
            apply_introspection_claims(&active_response(), &sample_config())
                .unwrap();
        assert_eq!(v.sub, "user-1");
    }

    #[test]
    fn accepts_without_mcp_scope() {
        assert!(
            apply_introspection_claims(&active_response(), &sample_config())
                .is_ok()
        );
    }

    #[test]
    fn rejects_inactive() {
        let mut r = active_response();
        r.active = false;
        let err = apply_introspection_claims(&r, &sample_config()).unwrap_err();
        assert!(matches!(err, AuthError::Inactive));
    }

    #[test]
    fn rejects_missing_project_audience_scope() {
        let mut r = active_response();
        r.scope = Some("openid offline_access".into());
        let err = apply_introspection_claims(&r, &sample_config()).unwrap_err();
        assert!(matches!(err, AuthError::InsufficientScope));
    }

    #[test]
    fn rejects_disallowed_sub() {
        let mut config = sample_config();
        config.allowed_subs = vec!["other".into()];
        let err = apply_introspection_claims(&active_response(), &config)
            .unwrap_err();
        assert!(matches!(err, AuthError::SubjectNotAllowed));
    }

    #[test]
    fn rejects_issuer_mismatch() {
        let mut config = sample_config();
        config.authorization_server = "https://other.example.com".into();
        let err = apply_introspection_claims(&active_response(), &config)
            .unwrap_err();
        assert!(matches!(err, AuthError::Invalid(_)));
    }

    #[test]
    fn accepts_required_zitadel_role() {
        let mut config = sample_config();
        config.required_roles = vec!["mcp".into()];
        let r = serde_json::from_value(json!({
            "active": true,
            "sub": "user-1",
            "iss": "https://auth.example.com",
            "scope": "openid urn:zitadel:iam:org:project:id:my_client_id:aud offline_access",
            "urn:zitadel:iam:org:project:roles": { "mcp": { "1": "org" } }
        }))
        .unwrap();
        assert!(apply_introspection_claims(&r, &config).is_ok());
    }

    #[test]
    fn accepts_required_role_on_project_claim() {
        let mut config = sample_config();
        config.required_roles = vec!["mcp".into()];
        let mut r = active_response();
        r.extra.insert(
            "urn:zitadel:iam:org:project:my_client_id:roles".into(),
            json!({ "mcp": { "1": "org" } }),
        );
        assert!(apply_introspection_claims(&r, &config).is_ok());
    }

    #[test]
    fn rejects_missing_required_role() {
        let mut config = sample_config();
        config.required_roles = vec!["mcp".into()];
        let err = apply_introspection_claims(&active_response(), &config)
            .unwrap_err();
        assert!(matches!(err, AuthError::InsufficientRole));
    }
}
