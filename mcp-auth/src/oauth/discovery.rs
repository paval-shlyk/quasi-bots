use jsonwebtoken::jwk::JwkSet;

pub type DiscoveryError = Box<dyn std::error::Error + Send + Sync>;

async fn fetch_jwks_unchecked(
    client: &reqwest::Client,
    uri: &str,
) -> Result<JwkSet, DiscoveryError> {
    let resp = client
        .get(uri)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch JWKS: {e}"))?;

    let resp = resp
        .error_for_status()
        .map_err(|e| format!("Server returns not 2xx status: {e}"))?;

    let set: JwkSet = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response :{e}"))?;

    Ok(set)
}

pub async fn fetch_jwks(
    client: &reqwest::Client,
    trusted_issuer: &str,
) -> Result<JwkSet, DiscoveryError> {
    let discovery_url =
        format!("{trusted_issuer}/.well-known/openid-configuration");

    #[derive(Debug, Clone, serde::Deserialize)]
    struct OAuthMetadata {
        pub issuer: String,
        pub jwks_uri: Option<String>,
    }

    let resp =
        client.get(discovery_url).send().await.map_err(|e| {
            format!("Failed to fetch OpenID configuration: {e}")
        })?;

    let resp = resp
        .error_for_status()
        .map_err(|e| format!("Server returns not 2xx status: {e}"))?;

    let metadata: OAuthMetadata = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Open ID configuration: {e}"))?;

    // OpenID Connect Discovery: the `issuer` value MUST exactly match the
    // Issuer Identifier used for discovery (here: configured authorization_server).
    let discovered = metadata.issuer.trim_end_matches('/');
    let trusted = trusted_issuer.trim_end_matches('/');
    if discovered != trusted {
        return Err(format!(
            "OIDC issuer mismatch: discovery returned {discovered}, expected {trusted}"
        )
        .into());
    }

    let Some(jwks_uri) = metadata.jwks_uri.as_deref().filter(|u| !u.is_empty())
    else {
        return Err(
            format!("The `jwks_uri` field is missing: {metadata:?}").into()
        );
    };

    fetch_jwks_unchecked(client, jwks_uri).await
}
