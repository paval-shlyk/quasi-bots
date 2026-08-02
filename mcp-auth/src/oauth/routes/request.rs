// RFC7591 — Dynamic Client Registration
#[derive(Debug, serde::Deserialize)]
pub struct ReqistrationBody {
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
}

// GET /oauth/authorize — consent page before Google sign-in
#[derive(Debug, serde::Deserialize)]
pub struct AuthorizeQuery {
    #[allow(dead_code)]
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    /// RFC 8707 resource indicator — canonical MCP server URI.
    pub resource: Option<String>,
}
