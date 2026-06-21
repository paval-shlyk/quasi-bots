/// OAuth 2.1 Authorization Code + PKCE flow.
///
/// The MCP server acts as its own Authorization Server:
///   GET  /.well-known/oauth-authorization-server  — RFC8414 metadata
///   POST /oauth/register                          — RFC7591 dynamic client reg
///   GET  /oauth/authorize                         — login + consent HTML
///   POST /oauth/approve                           — form submit → auth code
///   POST /oauth/token                             — code/refresh → Bearer token
///
/// Only one owner account is supported; credentials come from config.toml.
use std::{collections::HashMap, sync::Arc};

use axum::{
    Form, Json,
    body::Body,
    extract::{Query, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use uuid::Uuid;

use rmcp::transport::auth::{
    AuthorizationMetadata, ClientRegistrationResponse,
};

use crate::config::ServerConfig;

// ─── Data types ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct RegisteredClient {
    redirect_uris: Vec<String>,
    client_name:   Option<String>,
}

#[derive(Clone, Debug)]
struct AuthSession {
    redirect_uri:   String,
    scope:          Option<String>,
    state:          Option<String>,
    code_challenge: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AccessToken {
    pub access_token:  String,
    pub token_type:    String,
    pub expires_in:    u64,
    pub refresh_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope:         Option<String>,
    #[serde(skip)]
    pub issued_at:     chrono::DateTime<Utc>,
    #[serde(skip)]
    pub ttl_secs:      u64,
}

impl AccessToken {
    fn is_expired(&self) -> bool {
        Utc::now()
            .signed_duration_since(self.issued_at)
            .num_seconds()
            >= self.ttl_secs as i64
    }
}

// ─── Store ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct McpOAuthStore {
    pub server_cfg: ServerConfig,
    clients:        Arc<RwLock<HashMap<String, RegisteredClient>>>,
    /// auth_code → session (consumed on first use)
    sessions:       Arc<RwLock<HashMap<String, AuthSession>>>,
    /// access_token → record
    tokens:         Arc<RwLock<HashMap<String, AccessToken>>>,
}

impl McpOAuthStore {
    pub fn new(server_cfg: ServerConfig) -> Self {
        Self {
            server_cfg,
            clients:  Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            tokens:   Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn validate_token(&self, token: &str) -> bool {
        self.tokens
            .read()
            .await
            .get(token)
            .map(|t| !t.is_expired())
            .unwrap_or(false)
    }
}

fn random_string(len: usize) -> String {
    rand::rng()
        .sample_iter(rand::distr::Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

// ─── RFC8414 — Authorization Server Metadata ─────────────────────────────────

pub async fn oauth_metadata(
    State(store): State<Arc<McpOAuthStore>>,
) -> impl IntoResponse {
    let base = format!("http://{}", store.server_cfg.addr);
    let mut meta = AuthorizationMetadata::default();
    meta.authorization_endpoint          = format!("{base}/oauth/authorize");
    meta.token_endpoint                  = format!("{base}/oauth/token");
    meta.registration_endpoint           = Some(format!("{base}/oauth/register"));
    meta.scopes_supported                = Some(vec!["mcp".into()]);
    meta.response_types_supported        = Some(vec!["code".into()]);
    meta.code_challenge_methods_supported = Some(vec!["S256".into()]);
    meta.issuer                          = Some(base);
    (StatusCode::OK, Json(meta))
}

// ─── RFC7591 — Dynamic Client Registration ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegistrationRequest {
    pub client_name:   Option<String>,
    pub redirect_uris: Vec<String>,
}

pub async fn oauth_register(
    State(store): State<Arc<McpOAuthStore>>,
    Json(req): Json<RegistrationRequest>,
) -> impl IntoResponse {
    if req.redirect_uris.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_request",
                "error_description": "at least one redirect_uri is required"
            })),
        )
            .into_response();
    }

    let client_id     = format!("client-{}", Uuid::new_v4());
    let client_secret = random_string(32);

    store.clients.write().await.insert(
        client_id.clone(),
        RegisteredClient {
            redirect_uris: req.redirect_uris.clone(),
            client_name:   req.client_name.clone(),
        },
    );

    let mut resp =
        ClientRegistrationResponse::new(client_id, req.redirect_uris);
    resp.client_secret = Some(client_secret);
    resp.client_name   = req.client_name;

    (StatusCode::CREATED, Json(resp)).into_response()
}

// ─── GET /oauth/authorize — Login + consent page ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AuthorizeQuery {
    #[allow(dead_code)]
    response_type:         String,
    client_id:             String,
    redirect_uri:          String,
    scope:                 Option<String>,
    state:                 Option<String>,
    code_challenge:        Option<String>,
    #[allow(dead_code)]
    code_challenge_method: Option<String>,
}

const LOGIN_HTML: &str = include_str!("login.html");

pub async fn oauth_authorize(
    Query(params): Query<AuthorizeQuery>,
    State(store): State<Arc<McpOAuthStore>>,
) -> impl IntoResponse {
    let clients = store.clients.read().await;
    let valid = clients
        .get(&params.client_id)
        .map(|c| c.redirect_uris.contains(&params.redirect_uri))
        .unwrap_or(false);
    drop(clients);

    if !valid {
        return (
            StatusCode::BAD_REQUEST,
            Html("<h1>Unknown client or invalid redirect_uri</h1>"),
        )
            .into_response();
    }

    let hidden = format!(
        r#"<input type="hidden" name="client_id"      value="{cid}">
           <input type="hidden" name="redirect_uri"   value="{ruri}">
           <input type="hidden" name="scope"          value="{scope}">
           <input type="hidden" name="state"          value="{state}">
           <input type="hidden" name="code_challenge" value="{cc}">"#,
        cid   = he(&params.client_id),
        ruri  = he(&params.redirect_uri),
        scope = he(&params.scope.unwrap_or_default()),
        state = he(&params.state.unwrap_or_default()),
        cc    = he(&params.code_challenge.unwrap_or_default()),
    );

    Html(LOGIN_HTML
        .replace("{{HIDDEN_FIELDS}}", &hidden)
        .replace("{{ERROR_BLOCK}}", ""))
    .into_response()
}

/// Minimal HTML-escape for attribute values.
fn he(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ─── POST /oauth/approve — Form submit ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ApprovalForm {
    client_id:      String,
    redirect_uri:   String,
    #[serde(default)]
    scope:          Option<String>,
    #[serde(default)]
    state:          Option<String>,
    #[serde(default)]
    code_challenge: Option<String>,
    username:       String,
    password:       String,
}

pub async fn oauth_approve(
    State(store): State<Arc<McpOAuthStore>>,
    Form(form): Form<ApprovalForm>,
) -> impl IntoResponse {
    if form.username != store.server_cfg.username
        || form.password != store.server_cfg.password
    {
        // Re-render login page with an error banner — hidden fields are gone
        // (client must restart the flow), so we return 401.
        return (
            StatusCode::UNAUTHORIZED,
            Html(
                LOGIN_HTML
                    .replace("{{HIDDEN_FIELDS}}", "")
                    .replace(
                        "{{ERROR_BLOCK}}",
                        r#"<div class="error">Invalid username or password.</div>"#,
                    ),
            ),
        )
            .into_response();
    }

    let code = format!("code-{}", Uuid::new_v4());

    store.sessions.write().await.insert(
        code.clone(),
        AuthSession {
            redirect_uri:   form.redirect_uri.clone(),
            scope:          form.scope,
            state:          form.state.clone(),
            code_challenge: form.code_challenge,
        },
    );

    let mut url = format!("{}?code={}", form.redirect_uri, code);
    if let Some(s) = form.state.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&state={s}"));
    }
    Redirect::to(&url).into_response()
}

// ─── POST /oauth/token ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    grant_type:    String,
    #[serde(default)]
    code:          String,
    #[serde(default)]
    redirect_uri:  String,
    #[serde(default)]
    code_verifier: Option<String>,
    #[serde(default)]
    refresh_token: String,
}

pub async fn oauth_token(
    State(store): State<Arc<McpOAuthStore>>,
    request: Request<Body>,
) -> impl IntoResponse {
    let bytes = match axum::body::to_bytes(request.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"invalid_request"})),
            )
                .into_response()
        }
    };

    let token_req: TokenRequest =
        if let Ok(f) = serde_urlencoded::from_bytes(&bytes) {
            f
        } else if let Ok(j) = serde_json::from_slice(&bytes) {
            j
        } else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_request",
                    "error_description": "could not parse request body"
                })),
            )
                .into_response();
        };

    match token_req.grant_type.as_str() {
        "authorization_code" => handle_auth_code(store, token_req).await,
        "refresh_token"      => handle_refresh(store, token_req).await,
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"unsupported_grant_type"})),
        )
            .into_response(),
    }
}

async fn handle_auth_code(
    store: Arc<McpOAuthStore>,
    req: TokenRequest,
) -> Response {
    let session = store.sessions.write().await.remove(&req.code);
    let session = match session {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "unknown or expired code"
                })),
            )
                .into_response()
        }
    };

    if session.redirect_uri != req.redirect_uri {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "redirect_uri mismatch"
            })),
        )
            .into_response();
    }

    // PKCE S256 verification.
    if let Some(challenge) = session.code_challenge.as_deref().filter(|s| !s.is_empty()) {
        match req.code_verifier.as_deref() {
            Some(verifier) if pkce_s256_matches(verifier, challenge) => {}
            Some(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_grant",
                        "error_description": "code_verifier mismatch"
                    })),
                )
                    .into_response()
            }
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_grant",
                        "error_description": "code_verifier required"
                    })),
                )
                    .into_response()
            }
        }
    }

    issue_token(store, session.scope).await
}

async fn handle_refresh(
    store: Arc<McpOAuthStore>,
    req: TokenRequest,
) -> Response {
    let old_key = {
        let tokens = store.tokens.read().await;
        tokens
            .iter()
            .find(|(_, v)| v.refresh_token == req.refresh_token)
            .map(|(k, _)| k.clone())
    };

    let old = match old_key {
        Some(k) => store.tokens.write().await.remove(&k),
        None    => None,
    };

    match old {
        None => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "unknown refresh_token"
            })),
        )
            .into_response(),
        Some(prev) => issue_token(store, prev.scope).await,
    }
}

async fn issue_token(store: Arc<McpOAuthStore>, scope: Option<String>) -> Response {
    let ttl    = store.server_cfg.token_ttl_secs;
    let token  = format!("mcp-{}", Uuid::new_v4());
    let record = AccessToken {
        access_token:  token.clone(),
        token_type:    "Bearer".to_string(),
        expires_in:    ttl,
        refresh_token: format!("refresh-{}", Uuid::new_v4()),
        scope,
        issued_at:     Utc::now(),
        ttl_secs:      ttl,
    };
    store.tokens.write().await.insert(token, record.clone());
    (StatusCode::OK, Json(record)).into_response()
}

// ─── PKCE S256 verification ───────────────────────────────────────────────────

/// Returns true when SHA-256(verifier) base64url-no-pad == challenge.
fn pkce_s256_matches(verifier: &str, challenge: &str) -> bool {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest) == challenge
}

// ─── Bearer token validation middleware ──────────────────────────────────────

pub async fn bearer_auth_middleware(
    State(store): State<Arc<McpOAuthStore>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    match token {
        Some(t) if store.validate_token(&t).await => next.run(request).await,
        _ => {
            let mut resp = Response::new(Body::from(
                r#"{"error":"unauthorized","error_description":"valid Bearer token required"}"#,
            ));
            *resp.status_mut() = StatusCode::UNAUTHORIZED;
            resp.headers_mut().insert(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static(
                    r#"Bearer realm="finance-mcp""#,
                ),
            );
            resp.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            );
            resp
        }
    }
}
