use axum::{
    Form, Json,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};

use crate::oauth::metadata::{self, ProtectedResourceMetadata};

use super::SharedOAuthState;

mod authorize;
mod request;

pub use authorize::authorize_or_refresh_token;

const LOGIN_HTML: &str = include_str!("login.html");

/// Minimal HTML-escape for attribute values.
fn he(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// RFC 9728
pub async fn protected_resource_metadata(
    State(state): State<SharedOAuthState>,
) -> impl IntoResponse {
    let meta = ProtectedResourceMetadata::from_config(&state.config);
    (StatusCode::OK, Json(meta))
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

    let client_id = store
        .save_client(req.client_name.clone(), req.redirect_uris.clone())
        .await;

    let mut resp = serde_json::json!({
        "client_id": client_id,
        "redirect_uris": req.redirect_uris,
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
    });
    if let Some(name) = req.client_name {
        resp["client_name"] = serde_json::Value::String(name);
    }

    (StatusCode::CREATED, Json(resp)).into_response()
}

pub async fn authorize(
    Query(params): Query<request::AuthorizeQuery>,
    State(state): State<SharedOAuthState>,
) -> impl IntoResponse {
    let store = &state.store;
    let config = &state.config;

    if let Err(msg) = validate_pkce(&params.code_challenge, &params.code_challenge_method) {
        return (StatusCode::BAD_REQUEST, Html(format!("<h1>{msg}</h1>"))).into_response();
    }

    if let Some(resource) = params.resource.as_deref() {
        if !metadata::resource_matches(config, resource) {
            return (
                StatusCode::BAD_REQUEST,
                Html("<h1>Invalid resource parameter</h1>".to_string()),
            )
                .into_response();
        }
    }

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
           <input type="hidden" name="code_challenge" value="{cc}">
           <input type="hidden" name="resource"       value="{resource}">"#,
        cid = he(&params.client_id),
        ruri = he(&params.redirect_uri),
        scope = he(&params.scope.as_deref().unwrap_or(&config.scope)),
        state = he(&params.state.unwrap_or_default()),
        cc = he(params.code_challenge.as_deref().unwrap_or_default()),
        resource = he(params.resource.as_deref().unwrap_or(&config.resource_url())),
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
    let meta = metadata::authorization_metadata(&state.config);
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

fn validate_pkce(
    code_challenge: &Option<String>,
    code_challenge_method: &Option<String>,
) -> Result<(), &'static str> {
    let challenge = code_challenge.as_deref().filter(|s| !s.is_empty());
    match challenge {
        None => Err("PKCE code_challenge is required"),
        Some(_) if code_challenge_method.as_deref() != Some("S256") => {
            Err("PKCE code_challenge_method must be S256")
        }
        Some(_) => Ok(()),
    }
}