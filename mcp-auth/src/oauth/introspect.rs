//! RFC 7662 OAuth 2.0 Token Introspection for opaque Bearer access tokens.
//!
//! # Normative references
//!
//! - [RFC 7662](https://datatracker.ietf.org/doc/html/rfc7662) — Token Introspection
//! - [RFC 6750](https://datatracker.ietf.org/doc/html/rfc6750) — Bearer Token Usage
//! - [RFC 8707](https://datatracker.ietf.org/doc/html/rfc8707) — Resource Indicators
//! - [MCP Authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use crate::oauth::openid_config::CachedOpenIdConfig;

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
    pub client: &'a reqwest::Client,

    pub client_id: &'a str,
    pub client_secret: &'a str,

    pub expected_issuer: &'a str,
    pub expected_audience: &'a str,
    pub expected_scope: &'a str,
    pub allowed_subs: &'a [String],
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
        apply_introspection_claims(
            &resp,
            self.expected_issuer,
            self.expected_audience,
            self.expected_scope,
            self.allowed_subs,
        )
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
    expected_issuer: &str,
    expected_audience: &str,
    expected_scope: &str,
    allowed_subs: &[String],
) -> Result<ValidatedToken, AuthError> {
    if !response.active {
        return Err(AuthError::Inactive);
    }

    let Some(iss) = response.iss.as_ref() else {
        return Err(AuthError::Invalid("missing issuer field".into()));
    };

    let iss = iss.trim_end_matches('/');
    let expected = expected_issuer.trim_end_matches('/');

    if iss != expected {
        return Err(AuthError::Invalid(format!(
            "iss mismatch: got {iss}, expected {expected}"
        )));
    }

    // Require audience to include our MCP resource URI.
    match &response.aud {
        None => {
            return Err(AuthError::Invalid(
                "introspection response missing aud".into(),
            ));
        }
        Some(aud) if !audience_includes(aud, expected_audience) => {
            return Err(AuthError::Invalid(format!(
                "aud does not include resource {expected_audience}; got {aud}"
            )));
        }
        Some(_) => {}
    }

    let sub =
        response
            .sub
            .clone()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AuthError::Invalid("introspection response missing sub".into())
            })?;

    if !subject_allowed(allowed_subs, &sub) {
        return Err(AuthError::SubjectNotAllowed);
    }

    if !scope_satisfies(response.scope.as_deref(), expected_scope) {
        return Err(AuthError::InsufficientScope);
    }

    Ok(ValidatedToken { sub })
}

fn audience_includes(aud: &Value, expected: &str) -> bool {
    match aud {
        Value::String(s) => s == expected,
        Value::Array(items) => items
            .iter()
            .any(|v| v.as_str().map(|s| s == expected).unwrap_or(false)),
        _ => false,
    }
}

fn subject_allowed(allowed_subs: &[String], sub: &str) -> bool {
    allowed_subs.is_empty() || allowed_subs.iter().any(|s| s == sub)
}

fn scope_satisfies(granted: Option<&str>, required: &str) -> bool {
    let required: Vec<&str> = required
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .collect();
    if required.is_empty() {
        return true;
    }
    let Some(granted) = granted else {
        // No scope claim → trust AS.
        return true;
    };
    let granted: Vec<&str> = granted.split_whitespace().collect();
    if granted.is_empty() {
        return true;
    }
    required
        .iter()
        .all(|need| granted.iter().any(|have| have == need))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn active_response() -> IntrospectionResponse {
        serde_json::from_value(json!({
            "active": true,
            "sub": "user-1",
            "iss": "https://auth.example.com",
            "aud": "http://127.0.0.1:8080/mcp",
            "scope": "openid mcp",
            "exp": 9999999999_i64
        }))
        .unwrap()
    }

    #[test]
    fn accepts_valid_active_token() {
        let v = apply_introspection_claims(
            &active_response(),
            "https://auth.example.com",
            "http://127.0.0.1:8080/mcp",
            "mcp",
            &[],
        )
        .unwrap();
        assert_eq!(v.sub, "user-1");
    }

    #[test]
    fn rejects_inactive() {
        let mut r = active_response();
        r.active = false;
        let err = apply_introspection_claims(
            &r,
            "https://auth.example.com",
            "http://127.0.0.1:8080/mcp",
            "mcp",
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, AuthError::Inactive));
    }

    #[test]
    fn rejects_wrong_audience() {
        let mut r = active_response();
        r.aud = Some(json!("other-client"));
        let err = apply_introspection_claims(
            &r,
            "https://auth.example.com",
            "http://127.0.0.1:8080/mcp",
            "mcp",
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, AuthError::Invalid(_)));
    }

    #[test]
    fn accepts_audience_array_containing_resource() {
        let mut r = active_response();
        r.aud = Some(json!(["client-id", "http://127.0.0.1:8080/mcp"]));
        assert!(
            apply_introspection_claims(
                &r,
                "https://auth.example.com",
                "http://127.0.0.1:8080/mcp",
                "mcp",
                &[],
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_missing_audience() {
        let mut r = active_response();
        r.aud = None;
        assert!(
            apply_introspection_claims(
                &r,
                "https://auth.example.com",
                "http://127.0.0.1:8080/mcp",
                "mcp",
                &[],
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_missing_scope_segment() {
        let mut r = active_response();
        r.scope = Some("openid".into());
        let err = apply_introspection_claims(
            &r,
            "https://auth.example.com",
            "http://127.0.0.1:8080/mcp",
            "mcp",
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, AuthError::InsufficientScope));
    }

    #[test]
    fn rejects_disallowed_sub() {
        let err = apply_introspection_claims(
            &active_response(),
            "https://auth.example.com",
            "http://127.0.0.1:8080/mcp",
            "mcp",
            &[String::from("other")],
        )
        .unwrap_err();
        assert!(matches!(err, AuthError::SubjectNotAllowed));
    }

    #[test]
    fn rejects_issuer_mismatch() {
        let err = apply_introspection_claims(
            &active_response(),
            "https://other.example.com",
            "http://127.0.0.1:8080/mcp",
            "mcp",
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, AuthError::Invalid(_)));
    }
}
