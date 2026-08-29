//! JWT access-token validation via OIDC discovery and JWKS.
//!
//! # Normative references
//!
//! - [OpenID Connect Discovery 1.0](https://openid.net/specs/openid-connect-discovery-1_0.html)
//! - [RFC 7517](https://datatracker.ietf.org/doc/html/rfc7517) — JSON Web Key (JWK)
//! - [RFC 7519](https://datatracker.ietf.org/doc/html/rfc7519) — JSON Web Token (JWT)
//! - [RFC 8414](https://datatracker.ietf.org/doc/html/rfc8414) — Authorization Server Metadata
//! - [RFC 8707](https://datatracker.ietf.org/doc/html/rfc8707) — Resource Indicators
//! - [MCP Authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)

use std::sync::Arc;
use std::time::Duration;

use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet,
};
use serde::Deserialize;
use tokio::sync::RwLock;
use tokio::time::MissedTickBehavior;

use crate::config::McpAuthConfig;
use crate::oauth::discovery::fetch_jwks;

const CLOCK_SKEW_SECS: u64 = 60;

#[derive(Debug, Clone, Deserialize)]
struct AccessTokenClaims {
    sub: String,
    #[serde(default)]
    scope: Option<String>,
    /// Some ASes emit space-separated scopes as an array claim.
    #[serde(default)]
    scp: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ValidatedToken {
    pub sub: String,
}

#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    #[error("token is not a JWT or is malformed: {0}")]
    Malformed(String),
    #[error("token signature or claims invalid: {0}")]
    Invalid(String),
    #[error("subject not allowlisted")]
    SubjectNotAllowed,
    #[error("required scope missing")]
    InsufficientScope,
    #[error("JWKS unavailable: {0}")]
    Jwks(String),
}

pub type CachedJwkSet = Arc<RwLock<Option<JwkSet>>>;

pub fn spawn_jwks_refresh_task(
    client: reqwest::Client,
    trusted_issuer: String,
    jwks: CachedJwkSet,
    refresh_period: Duration,
) -> tokio::task::JoinHandle<()> {
    assert!(refresh_period.as_millis() > 0);

    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(refresh_period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        interval.tick().await; // first tick completes immediately

        loop {
            interval.tick().await;

            let maybe_fresh_jwks = fetch_jwks(&client, &trusted_issuer)
                .await
                .inspect_err(|e| {
                    tracing::warn!("Failed to refresh jwks: {e}");
                })
                .ok();

            *jwks.write().await = maybe_fresh_jwks;
        }
    })
}

pub async fn new_cached_jwks(
    config: McpAuthConfig,
) -> anyhow::Result<CachedJwkSet> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let trusted_issuer = &config.authorization_server;

    let fresh_jwks = fetch_jwks(&client, trusted_issuer)
        .await
        .inspect_err(|e| tracing::warn!("Failed to load JWKS on start up: {e}"))
        .ok();

    let jwks = Arc::new(RwLock::new(fresh_jwks));

    let _refresh_task = spawn_jwks_refresh_task(
        client,
        trusted_issuer.clone(),
        jwks.clone(),
        Duration::from_secs(60),
    );

    Ok(jwks)
}

/// Validates JWT access tokens issued by the configured authorization server.
pub struct JwtValidator<'a> {
    pub jwks: CachedJwkSet,

    pub expected_issuer: &'a str,
    pub expected_audience: &'a str,
    pub expected_scope: &'a str,
}

impl JwtValidator<'_> {
    /// Validate a Bearer access token (JWT).
    pub async fn validate(
        &self,
        token: &str,
    ) -> Result<ValidatedToken, JwtError> {
        let header = decode_header(token)
            .map_err(|e| JwtError::Malformed(e.to_string()))?;

        let kid = header.kid.as_deref().ok_or_else(|| {
            JwtError::Malformed("JWT header missing kid".into())
        })?;

        let decoding_key = self.lookup_key(kid).await?;

        let mut validation = Validation::new(header.alg);
        validation.leeway = CLOCK_SKEW_SECS;
        validation.set_issuer(&[self.expected_issuer]);
        validation.set_audience(&[self.expected_audience]);

        // Accept common AS algorithms; header.alg already selected the Validation default.
        validation.algorithms = vec![
            Algorithm::RS256,
            Algorithm::RS384,
            Algorithm::RS512,
            Algorithm::ES256,
            Algorithm::ES384,
            Algorithm::PS256,
            Algorithm::PS384,
            Algorithm::PS512,
        ];

        let data =
            decode::<AccessTokenClaims>(token, &decoding_key, &validation)
                .map_err(|e| JwtError::Invalid(e.to_string()))?;

        let claims = data.claims;

        if !scope_satisfies(&claims, self.expected_scope) {
            return Err(JwtError::InsufficientScope);
        }

        Ok(ValidatedToken { sub: claims.sub })
    }

    async fn lookup_key(&self, kid: &str) -> Result<DecodingKey, JwtError> {
        let cache = self.jwks.read().await;

        let Some(cache) = cache.as_ref() else {
            return Err(JwtError::Jwks("JWKS is not loaded".to_string()));
        };

        let Some(jwk) = cache.find(kid) else {
            return Err(JwtError::Invalid(format!(
                "no JWK found for kid={kid}"
            )));
        };

        DecodingKey::from_jwk(jwk)
            .map_err(|e| JwtError::Jwks(format!("unsupported JWK: {e}")))
    }
}

/// Scope check: if the token carries scope/scp claims, require `required` to be present.
/// Tokens without any scope claim are accepted (AS is trusted for authorization).
fn scope_satisfies(claims: &AccessTokenClaims, required: &str) -> bool {
    let mut granted = Vec::new();
    if let Some(scope) = &claims.scope {
        granted.extend(scope.split_whitespace().map(|s| s.to_string()));
    }
    if let Some(scp) = &claims.scp {
        granted.extend(scp.iter().cloned());
    }
    if granted.is_empty() {
        return true;
    }
    granted.iter().any(|s| s == required)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde::Serialize;

    #[test]
    fn scope_check_absent_ok() {
        let claims = AccessTokenClaims {
            sub: "u".into(),
            scope: None,
            scp: None,
        };
        assert!(scope_satisfies(&claims, "mcp"));
    }

    #[test]
    fn scope_check_present() {
        let claims = AccessTokenClaims {
            sub: "u".into(),
            scope: Some("openid mcp".into()),
            scp: None,
        };
        assert!(scope_satisfies(&claims, "mcp"));
        assert!(!scope_satisfies(&claims, "admin"));
    }

    #[derive(Serialize)]
    struct TestClaims {
        sub: String,
        iss: String,
        aud: String,
        exp: usize,
        iat: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    }

    fn test_rsa_keys() -> (EncodingKey, DecodingKey, String) {
        // Tiny fixed RSA key for unit tests only.
        let private_pem =
            include_str!("../../tests/fixtures/test_rsa_private.pem");
        let public_pem =
            include_str!("../../tests/fixtures/test_rsa_public.pem");
        let enc = EncodingKey::from_rsa_pem(private_pem.as_bytes()).unwrap();
        let dec = DecodingKey::from_rsa_pem(public_pem.as_bytes()).unwrap();
        (enc, dec, "test-kid".into())
    }

    #[test]
    fn encode_decode_roundtrip_with_aud() {
        let (enc, dec, kid) = test_rsa_keys();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;
        let claims = TestClaims {
            sub: "user-1".into(),
            iss: "https://auth.example.com/realms/mcp".into(),
            aud: "http://127.0.0.1:8080/mcp".into(),
            exp: now + 3600,
            iat: now,
            scope: Some("mcp".into()),
        };
        let token = encode(&header, &claims, &enc).unwrap();
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&["https://auth.example.com/realms/mcp"]);
        validation.set_audience(&["http://127.0.0.1:8080/mcp"]);
        let data =
            decode::<AccessTokenClaims>(&token, &dec, &validation).unwrap();
        assert_eq!(data.claims.sub, "user-1");
        assert!(scope_satisfies(&data.claims, "mcp"));
    }

    #[test]
    fn rejects_wrong_audience() {
        let (enc, dec, kid) = test_rsa_keys();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;
        let claims = TestClaims {
            sub: "user-1".into(),
            iss: "https://auth.example.com/realms/mcp".into(),
            aud: "https://other.example.com".into(),
            exp: now + 3600,
            iat: now,
            scope: None,
        };
        let token = encode(&header, &claims, &enc).unwrap();
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&["https://auth.example.com/realms/mcp"]);
        validation.set_audience(&["http://127.0.0.1:8080/mcp"]);
        assert!(
            decode::<AccessTokenClaims>(&token, &dec, &validation).is_err()
        );
    }
}
