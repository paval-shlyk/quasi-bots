use std::net::SocketAddr;

use serde::{Deserialize, Deserializer, de::Error as DeError};
use url::Url;

const MAX_TOKEN_TTL_SECS: u64 = 86_400;

/// Validate and normalize a bind address string.
pub fn validate_addr(value: &str) -> Result<String, String> {
    value
        .parse::<SocketAddr>()
        .map(|_| value.to_string())
        .map_err(|e| format!("invalid socket address: {e}"))
}

/// Validate and normalize a public base URL (RFC 3986).
///
/// See also: [RFC 9728](https://datatracker.ietf.org/doc/html/rfc9728) resource server metadata.
pub fn validate_public_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("public_url must not be empty".into());
    }

    let url = Url::parse(trimmed).map_err(|e| format!("invalid URL: {e}"))?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("public_url scheme must be http or https, got {scheme}")),
    }

    if url.host().is_none() {
        return Err("public_url must include a host".into());
    }

    if url.query().is_some() {
        return Err("public_url must not contain a query string".into());
    }

    if url.fragment().is_some() {
        return Err("public_url must not contain a fragment".into());
    }

    let path = url.path();
    if !path.is_empty() && path != "/" {
        return Err("public_url must not contain a path".into());
    }

    let mut normalized = format!(
        "{}://{}",
        url.scheme(),
        url.host_str().ok_or("public_url must include a host")?
    );

    if let Some(port) = url.port() {
        normalized.push(':');
        normalized.push_str(&port.to_string());
    }

    Ok(normalized)
}

/// Validate a non-empty username (trimmed).
pub fn validate_username(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("username must not be empty".into());
    }
    Ok(trimmed.to_string())
}

/// Validate a non-empty password (not trimmed — spaces may be intentional).
pub fn validate_password(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("password must not be empty".into());
    }
    Ok(value.to_string())
}

/// Validate access-token TTL in seconds.
pub fn validate_token_ttl_secs(value: u64) -> Result<u64, String> {
    if value == 0 {
        return Err("token_ttl_secs must be greater than 0".into());
    }
    if value > MAX_TOKEN_TTL_SECS {
        return Err(format!(
            "token_ttl_secs must not exceed {MAX_TOKEN_TTL_SECS} seconds"
        ));
    }
    Ok(value)
}

/// Validate an OAuth scope string ([RFC 6749 §3.3](https://datatracker.ietf.org/doc/html/rfc6749#section-3.3)).
pub fn validate_scope(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("scope must not be empty".into());
    }

    for part in trimmed.split(' ') {
        if part.is_empty() {
            return Err("scope must not contain empty segments".into());
        }
        if !part.bytes().all(|b| (0x21..=0x7E).contains(&b)) {
            return Err(format!(
                "scope segment '{part}' contains invalid characters"
            ));
        }
    }

    Ok(trimmed.to_string())
}

/// Validate a browser origin ([RFC 6454](https://datatracker.ietf.org/doc/html/rfc6454)).
pub fn validate_origin(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("origin must not be empty".into());
    }

    let url = Url::parse(trimmed).map_err(|e| format!("invalid origin URL: {e}"))?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("origin scheme must be http or https, got {scheme}")),
    }

    if url.host().is_none() {
        return Err("origin must include a host".into());
    }

    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err("origin must not contain a path, query, or fragment".into());
    }

    Ok(trimmed.to_string())
}

pub fn deserialize_addr<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_addr(&value).map_err(D::Error::custom)
}

pub fn deserialize_public_url<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_public_url(&value).map_err(D::Error::custom)
}

pub fn deserialize_username<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_username(&value).map_err(D::Error::custom)
}

pub fn deserialize_password<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_password(&value).map_err(D::Error::custom)
}

pub fn deserialize_token_ttl_secs<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    validate_token_ttl_secs(value).map_err(D::Error::custom)
}

pub fn deserialize_scope<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_scope(&value).map_err(D::Error::custom)
}

pub fn deserialize_allowed_origins<'de, D>(
    deserializer: D,
) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<String>::deserialize(deserializer)?;
    values
        .into_iter()
        .map(|origin| validate_origin(&origin).map_err(D::Error::custom))
        .collect()
}

pub fn default_token_ttl() -> u64 {
    3600
}

pub fn default_scope() -> String {
    "mcp".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_config(toml: &str) -> Result<crate::config::McpServerConfig, toml::de::Error> {
        toml::from_str(toml)
    }

    const VALID: &str = r#"
addr = "0.0.0.0:9191"
public_url = "http://127.0.0.1:9191"
username = "Paval"
password = "1234"
token_ttl_secs = 120
scope = "mcp"
allowed_origins = []
"#;

    #[test]
    fn valid_config_parses() {
        parse_config(VALID).expect("valid config should parse");
    }

    #[test]
    fn rejects_invalid_addr() {
        let err = parse_config(&VALID.replace("0.0.0.0:9191", "not-an-addr"))
            .expect_err("bad addr");
        assert!(err.to_string().contains("addr"));
    }

    #[test]
    fn rejects_public_url_with_path() {
        let err = parse_config(&VALID.replace(
            "http://127.0.0.1:9191",
            "http://127.0.0.1:9191/mcp",
        ))
        .expect_err("path in public_url");
        assert!(err.to_string().contains("public_url"));
    }

    #[test]
    fn rejects_empty_username() {
        let err = parse_config(&VALID.replace("username = \"Paval\"", "username = \"   \""))
            .expect_err("empty username");
        assert!(err.to_string().contains("username"));
    }

    #[test]
    fn rejects_zero_token_ttl() {
        let err =
            parse_config(&VALID.replace("token_ttl_secs = 120", "token_ttl_secs = 0"))
                .expect_err("zero ttl");
        assert!(err.to_string().contains("token_ttl_secs"));
    }

    #[test]
    fn rejects_unknown_field() {
        let mut cfg = VALID.to_string();
        cfg.push_str("unknown_field = true\n");
        parse_config(&cfg).expect_err("unknown field");
    }

    #[test]
    fn rejects_origin_with_path() {
        let err = parse_config(&VALID.replace(
            "allowed_origins = []",
            "allowed_origins = [\"http://localhost/callback\"]",
        ))
        .expect_err("origin path");
        assert!(err.to_string().contains("origin"));
    }

    #[test]
    fn validate_scope_accepts_multiple() {
        assert!(validate_scope("read write").is_ok());
    }
}