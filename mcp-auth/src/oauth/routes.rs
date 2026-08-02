use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
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

    if let Err(msg) =
        validate_pkce(&params.code_challenge, &params.code_challenge_method)
    {
        return (StatusCode::BAD_REQUEST, Html(format!("<h1>{msg}</h1>")))
            .into_response();
    }

    if let Some(resource) = params.resource.as_deref()
        && !metadata::resource_matches(config, resource) {
            return (
                StatusCode::BAD_REQUEST,
                Html("<h1>Invalid resource parameter</h1>".to_string()),
            )
                .into_response();
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

    let pending_id = store
        .save_pending_auth(super::store::PendingAuth {
            client_id: params.client_id.clone(),
            redirect_uri: params.redirect_uri.clone(),
            scope: params.scope.clone().or_else(|| Some(config.scope.clone())),
            state: params.state.clone(),
            code_challenge: params.code_challenge.clone(),
            resource: params
                .resource
                .clone()
                .or_else(|| Some(config.resource_url())),
            created_at: chrono::Utc::now(),
        })
        .await;

    let google_href =
        format!("/oauth/google/login?pending={}", he(&pending_id));
    let config_notice = if config.auth.google_configured() {
        String::new()
    } else {
        r#"<div class="error">Google OAuth is not configured yet. Set auth.google.client_id and GOOGLE_CLIENT_SECRET.</div>"#.into()
    };

    Html(
        LOGIN_HTML
            .replace("{{GOOGLE_HREF}}", &google_href)
            .replace("{{ERROR_BLOCK}}", &config_notice),
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
