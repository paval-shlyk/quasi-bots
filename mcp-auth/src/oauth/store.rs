use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use super::token::{StoredToken, TokenResponse, new_stored_token};

#[derive(Clone, Debug)]
pub struct AuthSession {
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    // round-trip state from PendingAuth (used only for redirect, not after)
    #[allow(unused)]
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub resource: Option<String>,
    pub owner_sub: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PendingAuth {
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub code_challenge: Option<String>,
    pub resource: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl PendingAuth {
    pub fn is_expired(&self) -> bool {
        Utc::now()
            .signed_duration_since(self.created_at)
            .num_minutes()
            >= 10
    }
}

#[derive(Clone, Debug)]
struct RegisteredClient {
    redirect_uris: Vec<String>,
    #[allow(unused)]
    client_name: Option<String>,
}

#[derive(Clone, Debug)]
struct GoogleOAuthState {
    pending_id: String,
    pkce_verifier: String,
    nonce: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct OAuthStore {
    clients: Arc<RwLock<HashMap<String, RegisteredClient>>>,
    /// auth_code → session (consumed on first use)
    sessions: Arc<RwLock<HashMap<String, AuthSession>>>,
    /// access_token → record
    tokens: Arc<RwLock<HashMap<String, StoredToken>>>,
    pending_auth: Arc<RwLock<HashMap<String, PendingAuth>>>,
    google_oauth_state: Arc<RwLock<HashMap<String, GoogleOAuthState>>>,
}

impl OAuthStore {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            tokens: Arc::new(RwLock::new(HashMap::new())),
            pending_auth: Arc::new(RwLock::new(HashMap::new())),
            google_oauth_state: Arc::new(RwLock::new(HashMap::new())),
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
            return Err("Unknown client or invalid redirect_uri".into());
        }

        Ok(())
    }

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

    pub async fn save_session(&self, session: AuthSession) -> String {
        let code = format!("code-{}", uuid::Uuid::new_v4());
        self.sessions.write().await.insert(code.clone(), session);
        code
    }

    pub async fn take_session(&self, code: &str) -> Option<AuthSession> {
        self.sessions.write().await.remove(code)
    }

    pub async fn issue_token(
        &self,
        ttl: Duration,
        scope: Option<String>,
        issuer: Option<String>,
        owner_sub: Option<String>,
        client_id: Option<String>,
    ) -> TokenResponse {
        let mut record = new_stored_token(ttl, scope, issuer);
        record.owner_sub = owner_sub;
        record.client_id = client_id;
        let response = record.to_response();
        self.tokens
            .write()
            .await
            .insert(record.access_token.clone(), record);
        response
    }

    pub async fn take_token_by_refresh(
        &self,
        refresh_token: &str,
    ) -> Option<StoredToken> {
        let old_key = {
            let tokens = self.tokens.read().await;
            tokens
                .iter()
                .find(|(_, v)| v.refresh_token == refresh_token)
                .map(|(k, _)| k.clone())
        };

        match old_key {
            Some(k) => self.tokens.write().await.remove(&k),
            None => None,
        }
    }

    pub async fn save_pending_auth(&self, pending: PendingAuth) -> String {
        let id = format!("pending-{}", uuid::Uuid::new_v4());
        self.pending_auth.write().await.insert(id.clone(), pending);
        id
    }

    pub async fn get_pending_auth(&self, id: &str) -> Option<PendingAuth> {
        let pending = self.pending_auth.read().await.get(id).cloned()?;
        if pending.is_expired() {
            self.pending_auth.write().await.remove(id);
            return None;
        }
        Some(pending)
    }

    pub async fn take_pending_auth(&self, id: &str) -> Option<PendingAuth> {
        let pending = self.pending_auth.write().await.remove(id)?;
        if pending.is_expired() {
            return None;
        }
        Some(pending)
    }

    pub async fn save_google_state(
        &self,
        state: &str,
        pending_id: &str,
        pkce_verifier: &str,
        nonce: &str,
    ) {
        self.google_oauth_state.write().await.insert(
            state.to_string(),
            GoogleOAuthState {
                pending_id: pending_id.to_string(),
                pkce_verifier: pkce_verifier.to_string(),
                nonce: nonce.to_string(),
                created_at: Utc::now(),
            },
        );
    }

    pub async fn take_google_state(
        &self,
        state: &str,
    ) -> Option<(String, String, String)> {
        let record = self.google_oauth_state.write().await.remove(state)?;
        if Utc::now()
            .signed_duration_since(record.created_at)
            .num_minutes()
            >= 10
        {
            return None;
        }
        Some((record.pending_id, record.pkce_verifier, record.nonce))
    }
}
