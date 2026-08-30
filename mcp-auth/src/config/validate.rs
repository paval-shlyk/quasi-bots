use serde::{Deserialize, Deserializer, de::Error as DeError};
use url::Url;

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
        scheme => {
            return Err(format!(
                "public_url scheme must be http or https, got {scheme}"
            ));
        }
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

/// Validate and normalize an external authorization server issuer URL.
pub fn validate_authorization_server(value: &str) -> Result<String, String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("authorization_server must not be empty".into());
    }

    let url = Url::parse(trimmed).map_err(|e| format!("invalid URL: {e}"))?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "authorization_server scheme must be http or https, got {scheme}"
            ));
        }
    }

    if url.host().is_none() {
        return Err("authorization_server must include a host".into());
    }

    if url.query().is_some() {
        return Err(
            "authorization_server must not contain a query string".into()
        );
    }

    if url.fragment().is_some() {
        return Err("authorization_server must not contain a fragment".into());
    }

    // Rebuild without trailing slash on path.
    let mut normalized = format!(
        "{}://{}",
        url.scheme(),
        url.host_str()
            .ok_or("authorization_server must include a host")?
    );
    if let Some(port) = url.port() {
        normalized.push(':');
        normalized.push_str(&port.to_string());
    }
    let path = url.path().trim_end_matches('/');
    if !path.is_empty() && path != "/" {
        normalized.push_str(path);
    }

    Ok(normalized)
}

/// Validate introspection API client id (non-empty).
pub fn validate_introspection_client_id(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("introspection_client_id must not be empty".into());
    }
    Ok(trimmed.to_string())
}

/// Validate allowlisted token `sub` values.
pub fn validate_allowed_subs(
    values: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut subs = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("allowed_subs must not contain empty values".into());
        }
        subs.push(trimmed.to_string());
    }
    Ok(subs)
}

/// Validate a single OAuth scope token ([RFC 6749 §3.3](https://datatracker.ietf.org/doc/html/rfc6749#section-3.3)).
pub fn validate_scope_token(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("scope must not contain empty segments".into());
    }
    if trimmed
        .bytes()
        .any(|b| b == b' ' || !(0x21..=0x7E).contains(&b))
    {
        return Err(format!(
            "scope segment '{trimmed}' contains invalid characters"
        ));
    }
    Ok(trimmed.to_string())
}

fn parse_scope_list(raw: toml::Value) -> Result<Vec<String>, String> {
    let toml::Value::Array(values) = raw else {
        return Err("scope lists must be an array of strings".into());
    };

    let mut scopes = Vec::with_capacity(values.len());
    for value in values {
        let Some(segment) = value.as_str() else {
            return Err("only string array elements are supported".into());
        };
        scopes.push(validate_scope_token(segment)?);
    }
    Ok(scopes)
}

/// Validate a browser origin ([RFC 6454](https://datatracker.ietf.org/doc/html/rfc6454)).
pub fn validate_origin(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("origin must not be empty".into());
    }

    let url =
        Url::parse(trimmed).map_err(|e| format!("invalid origin URL: {e}"))?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "origin scheme must be http or https, got {scheme}"
            ));
        }
    }

    if url.host().is_none() {
        return Err("origin must include a host".into());
    }

    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err("origin must not contain a path, query, or fragment".into());
    }

    Ok(trimmed.to_string())
}

pub fn deserialize_public_url<'de, D>(
    deserializer: D,
) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_public_url(&value).map_err(D::Error::custom)
}

pub fn deserialize_authorization_server<'de, D>(
    deserializer: D,
) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_authorization_server(&value).map_err(D::Error::custom)
}

pub fn deserialize_introspection_client_id<'de, D>(
    deserializer: D,
) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    validate_introspection_client_id(&value).map_err(D::Error::custom)
}

const INTROSPECTION_CLIENT_SECRET_ENV: &str = "INTROSPECTION_CLIENT_SECRET";

fn read_introspection_client_secret_env() -> Option<String> {
    std::env::var(INTROSPECTION_CLIENT_SECRET_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Read `INTROSPECTION_CLIENT_SECRET` (serde `default` when the TOML key is absent).
pub fn introspection_client_secret_from_env() -> Option<String> {
    read_introspection_client_secret_env()
}

/// TOML value (if non-empty) overrides env; empty/missing falls back to env.
pub fn deserialize_introspection_client_secret<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let from_toml = Option::<String>::deserialize(deserializer)?;
    if let Some(value) = from_toml {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }
    Ok(read_introspection_client_secret_env())
}

pub fn deserialize_allowed_subs<'de, D>(
    deserializer: D,
) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<String>::deserialize(deserializer)?;
    validate_allowed_subs(values).map_err(D::Error::custom)
}

pub fn deserialize_scope_list<'de, D>(
    deserializer: D,
) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_value = toml::Value::deserialize(deserializer)?;
    parse_scope_list(raw_value).map_err(D::Error::custom)
}

pub fn deserialize_supported_scopes<'de, D>(
    deserializer: D,
) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let scopes = deserialize_scope_list(deserializer)?;
    if scopes.is_empty() {
        return Err(D::Error::custom("supported_scopes must not be empty"));
    }
    Ok(scopes)
}

pub fn default_supported_scopes() -> Vec<String> {
    vec!["mcp".into()]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_config(
        toml: &str,
    ) -> Result<crate::config::McpAuthConfig, toml::de::Error> {
        toml::from_str(toml)
    }

    const VALID: &str = r#"
public_url = "http://127.0.0.1:8080"
authorization_server = "https://auth.example.com/realms/mcp"
supported_scopes = ["mcp"]
allowed_origins = []
allowed_subs = []
introspection_client_id = "rs-api-client"
"#;

    #[test]
    fn valid_config_parses() {
        parse_config(VALID).expect("valid config should parse");
    }

    #[test]
    fn accepts_keycloak_realm_path() {
        let cfg = parse_config(VALID).unwrap();
        assert_eq!(
            cfg.authorization_server,
            "https://auth.example.com/realms/mcp"
        );
    }

    #[test]
    fn strips_trailing_slash_on_authorization_server() {
        let cfg = parse_config(&VALID.replace(
            "https://auth.example.com/realms/mcp",
            "https://auth.example.com/realms/mcp/",
        ))
        .unwrap();
        assert_eq!(
            cfg.authorization_server,
            "https://auth.example.com/realms/mcp"
        );
    }

    #[test]
    fn rejects_public_url_with_path() {
        let err = parse_config(
            &VALID
                .replace("http://127.0.0.1:8080", "http://127.0.0.1:8080/mcp"),
        )
        .expect_err("path in public_url");
        assert!(err.to_string().contains("public_url"));
    }

    #[test]
    fn rejects_empty_authorization_server() {
        let err = parse_config(&VALID.replace(
            "authorization_server = \"https://auth.example.com/realms/mcp\"",
            "authorization_server = \"   \"",
        ))
        .expect_err("empty authorization_server");
        assert!(err.to_string().contains("authorization_server"));
    }

    #[test]
    fn rejects_authorization_server_with_query() {
        let err = parse_config(&VALID.replace(
            "https://auth.example.com/realms/mcp",
            "https://auth.example.com/realms/mcp?x=1",
        ))
        .expect_err("query in authorization_server");
        assert!(err.to_string().contains("authorization_server"));
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
    fn validate_scope_token_rejects_space() {
        assert!(validate_scope_token("read write").is_err());
        assert!(validate_scope_token("mcp").is_ok());
    }

    #[test]
    fn supported_scopes_default_is_mcp() {
        let cfg = parse_config(VALID).unwrap();
        assert_eq!(cfg.supported_scopes, vec!["mcp".to_string()]);
        assert!(cfg.required_introspection_scopes.is_empty());
        assert!(cfg.required_roles.is_empty());
        assert_eq!(cfg.supported_scopes_param(), "mcp");
    }

    #[test]
    fn scope_lists_deserialize() {
        let cfg = parse_config(
            r#"
public_url = "http://127.0.0.1:8080"
authorization_server = "https://auth.example.com/realms/mcp"
supported_scopes = ["mcp"]
required_introspection_scopes = [
    "openid",
    "offline_access",
    "urn:zitadel:iam:org:project:id:my_client_id:aud",
]
allowed_origins = []
allowed_subs = []
introspection_client_id = "rs-api-client"
"#,
        )
        .expect("scope lists should parse");
        assert_eq!(cfg.supported_scopes, vec!["mcp".to_string()]);
        assert_eq!(
            cfg.required_introspection_scopes,
            vec![
                "openid".to_string(),
                "offline_access".to_string(),
                "urn:zitadel:iam:org:project:id:my_client_id:aud".to_string(),
            ]
        );
    }

    #[test]
    fn rejects_empty_supported_scopes() {
        let err =
            parse_config(&VALID.replace(
                "supported_scopes = [\"mcp\"]",
                "supported_scopes = []",
            ))
            .expect_err("empty supported_scopes");
        assert!(err.to_string().contains("supported_scopes"));
    }

    #[test]
    fn resource_url_from_config() {
        let cfg = parse_config(VALID).unwrap();
        assert_eq!(cfg.resource_url(), "http://127.0.0.1:8080/mcp");
        assert_eq!(
            cfg.protected_resource_metadata_url(),
            "http://127.0.0.1:8080/.well-known/oauth-protected-resource/mcp"
        );
    }

    #[test]
    fn subject_allowlist() {
        let mut cfg = parse_config(VALID).unwrap();
        assert!(cfg.subject_allowed("anyone"));
        cfg.allowed_subs = vec!["alice".into()];
        assert!(cfg.subject_allowed("alice"));
        assert!(!cfg.subject_allowed("bob"));
    }

    #[test]
    fn rejects_empty_introspection_client_id() {
        let err = parse_config(&VALID.replace(
            "introspection_client_id = \"rs-api-client\"",
            "introspection_client_id = \"   \"",
        ))
        .expect_err("empty introspection_client_id");
        assert!(err.to_string().contains("introspection_client_id"));
    }

    #[test]
    fn secret_from_toml_wins() {
        let cfg = parse_config(&format!(
            "{VALID}\nintrospection_client_secret = \"s3cret\"\n"
        ))
        .unwrap();
        assert_eq!(cfg.introspection_client_secret.as_deref(), Some("s3cret"));
    }

    #[test]
    fn empty_toml_secret_uses_env_helper_shape() {
        // When TOML is empty/absent, deserializer falls back to env helper.
        // We only assert the helper returns Option without mutating process env.
        let _ = introspection_client_secret_from_env();
        let cfg = parse_config(&format!(
            "{VALID}\nintrospection_client_secret = \"\"\n"
        ))
        .unwrap();
        // Result is either None or Some(env); never Some("").
        assert!(
            cfg.introspection_client_secret
                .as_deref()
                .is_none_or(|s| !s.is_empty())
        );
    }
}
