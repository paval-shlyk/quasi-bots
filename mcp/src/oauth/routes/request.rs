// RFC7591 — Dynamic Client Registration
#[derive(Debug, serde::Deserialize)]
pub struct ReqistrationBody {
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
}

// GET /oauth/authorize — Login + consent page
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

#[derive(Clone, Debug, serde::Deserialize)]
pub struct ApprovalForm {
    pub client_id: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub code_challenge: Option<String>,
    #[serde(default)]
    pub resource: Option<String>,
    pub username: String,
    pub password: String,
}

impl From<ApprovalForm> for crate::oauth::store::AuthSession {
    fn from(value: ApprovalForm) -> Self {
        Self {
            code_challenge: value.code_challenge,
            state: value.state,
            scope: value.scope,
            redirect_uri: value.redirect_uri,
            resource: value.resource,
        }
    }
}