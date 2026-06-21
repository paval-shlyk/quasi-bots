use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::sync::RwLock;

use super::token::{StoredToken, TokenResponse, new_stored_token};

#[derive(Clone, Debug)]
pub struct AuthSession {
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub resource: Option<String>,
}

#[derive(Clone, Debug)]
struct RegisteredClient {
    pub redirect_uris: Vec<String>,
    pub client_name: Option<String>,
}

#[derive(Clone)]
pub struct OAuthStore {
    pub clients: Arc<RwLock<HashMap<String, RegisteredClient>>>,
    /// auth_code → session (consumed on first use)
    pub sessions: Arc<RwLock<HashMap<String, AuthSession>>>,
    /// access_token → record
    pub tokens: Arc<RwLock<HashMap<String, StoredToken>>>,
}

impl OAuthStore {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            tokens: Arc::new(RwLock::new(HashMap::new())),
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

    pub async fn authorize_client(
        &self,
        client_id: &str,
        redirect_uri: &String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let is_valid = self
            .clients
            .read()
            .await
            .get(client_id)
            .map(|c| c.redirect_uris.contains(redirect_uri))
            .unwrap_or(false);

        if !is_valid {
            return Err(
                format!("Unknown client or invalid redirect_uri").into()
            );
        }

        Ok(())
    }

    /// Save client and return its ID
    pub async fn save_client(
        &self,
        client_name: Option<String>,
        redirect_uris: Vec<String>,
    ) -> String {
        let client_id = format!("client-{}", uuid::Uuid::new_v4());

        self.clients.write().await.insert(
            client_id.clone(),
            RegisteredClient {
                client_name,
                redirect_uris,
            },
        );

        client_id
    }

    pub async fn save_session(&self, form: impl Into<AuthSession>) -> String {
        let code = format!("code-{}", uuid::Uuid::new_v4());

        self.sessions
            .write()
            .await
            .insert(code.clone(), form.into());

        code
    }

    /// Issue a new token pair and return the RFC 6749 wire response.
    ///
    /// Refresh rotation: each call generates a new access and refresh token.
    pub async fn issue_token(
        &self,
        ttl: Duration,
        scope: Option<String>,
        issuer: Option<String>,
    ) -> TokenResponse {
        let record = new_stored_token(ttl, scope, issuer);
        let response = record.to_response();
        self.tokens
            .write()
            .await
            .insert(record.access_token.clone(), record);
        response
    }
}