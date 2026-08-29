use axum::http::uri;
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

    let jwks_uri = match metadata.issuer.parse::<uri::Uri>() {
        Ok(issuer_uri)
            if let Some((jwks_uri, scheme)) = metadata.jwks_uri.as_ref().zip(issuer_uri.scheme())
                && *scheme == uri::Scheme::HTTPS
                && issuer_uri == trusted_issuer => {
                    jwks_uri
            }
        Ok(_uri) => {
            return Err(format!("The `jwks_uri` field is missing or issuer is not trusted: {metadata:?}").into())
            }
        Err(e) => {
            return Err(format!("Failed to parse the `issuer` ({issuer}) as a valid URI: {e}", issuer= metadata.issuer).into());
        }
    };

    let set = fetch_jwks_unchecked(client, jwks_uri).await?;

    Ok(set)
}
