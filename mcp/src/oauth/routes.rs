use axum::{
    Form, Json,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use rmcp::transport::auth::{
    AuthorizationMetadata, ClientRegistrationResponse,
};

use crate::oauth::random_string;

use super::SharedOAuthState;

mod authorize;
mod request;
mod response;

pub use authorize::authorize_or_refresh_token;

const LOGIN_HTML: &str = include_str!("login.html");

/// Minimal HTML-escape for attribute values.
fn he(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub async fn register(
    State(state): State<SharedOAuthState>,
    Json(req): Json<request::ReqistrationBody>,
) -> impl IntoResponse {
    let store = &state.store;

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

    let client_secret = random_string(32);
    let client_id = store
        .save_client(req.client_name.clone(), req.redirect_uris.clone())
        .await;

    let mut resp =
        ClientRegistrationResponse::new(client_id, req.redirect_uris);
    resp.client_secret = Some(client_secret);
    resp.client_name = req.client_name;

    (StatusCode::CREATED, Json(resp)).into_response()
}

pub async fn authorize(
    Query(params): Query<request::AuthorizeQuery>,
    State(state): State<SharedOAuthState>,
) -> impl IntoResponse {
    let store = &state.store;

    match store
        .authorize_client(&params.client_id, &params.redirect_uri)
        .await
    {
        Ok(()) => {}
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Html(format!("<h1>{e}</h1>")))
                .into_response();
        }
    };

    let hidden = format!(
        r#"<input type="hidden" name="client_id"      value="{cid}">
           <input type="hidden" name="redirect_uri"   value="{ruri}">
           <input type="hidden" name="scope"          value="{scope}">
           <input type="hidden" name="state"          value="{state}">
           <input type="hidden" name="code_challenge" value="{cc}">"#,
        cid = he(&params.client_id),
        ruri = he(&params.redirect_uri),
        scope = he(&params.scope.unwrap_or_default()),
        state = he(&params.state.unwrap_or_default()),
        cc = he(&params.code_challenge.unwrap_or_default()),
    );

    Html(
        LOGIN_HTML
            .replace("{{HIDDEN_FIELDS}}", &hidden)
            .replace("{{ERROR_BLOCK}}", ""),
    )
    .into_response()
}

// RFC8414
pub async fn metadata(
    State(state): State<SharedOAuthState>,
) -> impl IntoResponse {
    let config = &state.config;

    let base = format!("http://{}", config.addr);
    let mut meta = AuthorizationMetadata::default();

    meta.authorization_endpoint = format!("{base}/oauth/authorize");
    meta.token_endpoint = format!("{base}/oauth/token");
    meta.registration_endpoint = Some(format!("{base}/oauth/register"));
    meta.scopes_supported = Some(vec!["mcp".into()]);
    meta.response_types_supported = Some(vec!["code".into()]);
    meta.code_challenge_methods_supported = Some(vec!["S256".into()]);
    meta.issuer = Some(base);

    (StatusCode::OK, Json(meta))
}

// POST /oauth/approve
pub async fn approve(
    State(state): State<SharedOAuthState>,
    Form(form): Form<request::ApprovalForm>,
) -> impl IntoResponse {
    let config = &state.config;
    let store = &state.store;

    if form.username != config.username || form.password != config.password {
        // Re-render login page with an error banner — hidden fields are gone
        // (client must restart the flow), so we return 401.
        return (
            StatusCode::UNAUTHORIZED,
            Html(LOGIN_HTML.replace("{{HIDDEN_FIELDS}}", "").replace(
                "{{ERROR_BLOCK}}",
                r#"<div class="error">Invalid username or password.</div>"#,
            )),
        )
            .into_response();
    }

    let code = store.save_session(form.clone()).await;

    let mut url = format!("{}?code={}", form.redirect_uri, code);

    if let Some(s) = form.state.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&state={s}"));
    }

    Redirect::to(&url).into_response()
}
