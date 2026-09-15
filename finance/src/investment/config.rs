//! Finance TOML sections + Dzengi client credentials.
//!
//! Alerts / Telegram knobs live on the mounted skill-master config
//! (`[finance.alerts]`, `[finance.telegram]`). Env vars optionally override
//! when set (legacy `TRADING_*` / `TELEGRAM_*` for back-compat).
//!
//! **Tokens / chats:** never log; never put in MCP tool responses. Custom
//! [`Debug`] redacts secrets. Git template keeps `[finance.alerts]` /
//! `[finance.telegram]` empty/disabled; real values only on pod mount.
//!
//! **Single bot:** one `bot_token` + `chat_id` for outbound alerts and inbound
//! commands. This skill-master instance must be the **sole `getUpdates`
//! consumer** for that token (Telegram allows only one poller per bot).

use std::time::Duration;

/// Dzengi REST credentials (existing).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DzengiConfig {
    pub base_url: String,
    pub api_key: String,
    pub api_secret: String,
}

impl std::fmt::Debug for DzengiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DzengiConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &"[redacted]")
            .field("api_secret", &"[redacted]")
            .finish()
    }
}

fn default_eval_interval_secs() -> u64 {
    60
}

fn default_rest_reconcile_secs() -> u64 {
    600
}

fn default_telegram_poll_secs() -> u64 {
    30
}

fn default_telegram_batch_limit() -> i64 {
    20
}

/// `[finance.alerts]` — evaluator gate + cadence (A3).
/// Git template: disabled defaults; enable on pod mount.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AlertsConfig {
    #[serde(default)]
    pub evaluator_enabled: bool,
    #[serde(default = "default_eval_interval_secs")]
    pub eval_interval_secs: u64,
    #[serde(default = "default_rest_reconcile_secs")]
    pub rest_reconcile_secs: u64,
}

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            evaluator_enabled: false,
            eval_interval_secs: default_eval_interval_secs(),
            rest_reconcile_secs: default_rest_reconcile_secs(),
        }
    }
}

impl std::fmt::Debug for AlertsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlertsConfig")
            .field("evaluator_enabled", &self.evaluator_enabled)
            .field("eval_interval_secs", &self.eval_interval_secs)
            .field("rest_reconcile_secs", &self.rest_reconcile_secs)
            .finish()
    }
}

/// `[finance.telegram]` — delivery (A4) + inbound commands (A4.5).
///
/// Single bot: `bot_token` + `chat_id` for both `sendMessage` and
/// `getUpdates`. This process must be the sole getUpdates consumer for that
/// token. Git template keeps tokens empty; real values only on pod mount.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TelegramConfig {
    #[serde(default)]
    pub delivery_enabled: bool,
    #[serde(default)]
    pub commands_enabled: bool,
    /// Bot API token (Vault/mount). Shared by outbound + inbound in this
    /// process — do not run another getUpdates poller against it.
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub chat_id: String,
    #[serde(default = "default_telegram_poll_secs")]
    pub poll_secs: u64,
    #[serde(default = "default_telegram_batch_limit")]
    pub batch_limit: i64,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            delivery_enabled: false,
            commands_enabled: false,
            bot_token: String::new(),
            chat_id: String::new(),
            poll_secs: default_telegram_poll_secs(),
            batch_limit: default_telegram_batch_limit(),
        }
    }
}

impl std::fmt::Debug for TelegramConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramConfig")
            .field("delivery_enabled", &self.delivery_enabled)
            .field("commands_enabled", &self.commands_enabled)
            .field("bot_token", &"[redacted]")
            .field("chat_id", &"[redacted]")
            .field("poll_secs", &self.poll_secs)
            .field("batch_limit", &self.batch_limit)
            .finish()
    }
}

/// Parse `1|true|yes` (case-insensitive) as enabled.
pub fn parse_enabled_flag(raw: Option<&str>) -> bool {
    raw.map(|v| {
        v == "1"
            || v.eq_ignore_ascii_case("true")
            || v.eq_ignore_ascii_case("yes")
    })
    .unwrap_or(false)
}

/// When env var is set, its truthiness wins; otherwise use `file_default`.
pub fn env_flag_or(name: &str, file_default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => parse_enabled_flag(Some(v.trim())),
        Err(_) => file_default,
    }
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Runtime view after merging mounted config + optional env overrides.
#[derive(Clone)]
pub struct ResolvedTelegram {
    pub delivery_enabled: bool,
    pub commands_enabled: bool,
    bot_token: String,
    chat_id: String,
    /// Primary chat id plus optional `TELEGRAM_CHAT_ALLOWLIST` entries.
    pub allowlist: Vec<String>,
    pub poll_interval: Duration,
    pub batch_limit: i64,
    pub http_timeout: Duration,
}

impl std::fmt::Debug for ResolvedTelegram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedTelegram")
            .field("delivery_enabled", &self.delivery_enabled)
            .field("commands_enabled", &self.commands_enabled)
            .field("bot_token", &"[redacted]")
            .field("chat_id", &"[redacted]")
            .field("allowlist_len", &self.allowlist.len())
            .field("poll_interval", &self.poll_interval)
            .field("batch_limit", &self.batch_limit)
            .field("http_timeout", &self.http_timeout)
            .finish()
    }
}

impl ResolvedTelegram {
    pub fn credentials_ok(&self) -> bool {
        !self.bot_token.is_empty() && !self.allowlist.is_empty()
    }

    pub fn bot_token(&self) -> &str {
        &self.bot_token
    }

    pub fn primary_chat_id(&self) -> &str {
        &self.chat_id
    }

    pub fn chat_allowed(&self, chat_id: &str) -> bool {
        self.allowlist.iter().any(|id| id == chat_id)
    }
}

impl TelegramConfig {
    /// Merge file values with env overrides (`TELEGRAM_*` / `TRADING_TELEGRAM_*`).
    pub fn resolve(&self) -> ResolvedTelegram {
        let delivery_enabled =
            env_flag_or("TRADING_TELEGRAM_DELIVERY", self.delivery_enabled);
        let commands_enabled =
            env_flag_or("TRADING_TELEGRAM_COMMANDS", self.commands_enabled);

        let bot_token = env_nonempty("TELEGRAM_BOT_TOKEN")
            .unwrap_or_else(|| self.bot_token.trim().to_string());
        let chat_id = env_nonempty("TELEGRAM_CHAT_ID")
            .unwrap_or_else(|| self.chat_id.trim().to_string());

        let mut allowlist = Vec::new();
        if !chat_id.is_empty() {
            allowlist.push(chat_id.clone());
        }
        if let Some(extra) = env_nonempty("TELEGRAM_CHAT_ALLOWLIST") {
            for part in extra.split(',') {
                let t = part.trim();
                if !t.is_empty() && !allowlist.iter().any(|id| id == t) {
                    allowlist.push(t.to_string());
                }
            }
        }

        let mut poll_secs = if self.poll_secs > 0 {
            self.poll_secs.min(600)
        } else {
            default_telegram_poll_secs()
        };
        if let Some(v) = env_nonempty("TRADING_TELEGRAM_POLL_SECS")
            && let Ok(secs) = v.parse::<u64>()
            && secs > 0
        {
            poll_secs = secs.min(600);
        }

        let mut batch_limit = if self.batch_limit > 0 {
            self.batch_limit.min(100)
        } else {
            default_telegram_batch_limit()
        };
        if let Some(v) = env_nonempty("TRADING_TELEGRAM_BATCH_LIMIT")
            && let Ok(n) = v.parse::<i64>()
            && n > 0
        {
            batch_limit = n.min(100);
        }

        ResolvedTelegram {
            delivery_enabled,
            commands_enabled,
            bot_token,
            chat_id,
            allowlist,
            poll_interval: Duration::from_secs(poll_secs),
            batch_limit,
            http_timeout: Duration::from_secs(15),
        }
    }
}

impl AlertsConfig {
    /// `evaluator_enabled` after optional `TRADING_ALERT_EVALUATOR` override.
    pub fn resolve_enabled(&self) -> bool {
        env_flag_or("TRADING_ALERT_EVALUATOR", self.evaluator_enabled)
    }

    /// Cadence after optional `TRADING_ALERT_*_SECS` overrides.
    pub fn resolve_intervals(&self) -> (Duration, Duration) {
        let mut eval = if self.eval_interval_secs > 0 {
            self.eval_interval_secs
        } else {
            default_eval_interval_secs()
        };
        let mut rest = if self.rest_reconcile_secs > 0 {
            self.rest_reconcile_secs.min(3600)
        } else {
            default_rest_reconcile_secs()
        };
        if let Some(v) = env_nonempty("TRADING_ALERT_EVAL_INTERVAL_SECS")
            && let Ok(secs) = v.parse::<u64>()
            && secs > 0
        {
            eval = secs;
        }
        if let Some(v) = env_nonempty("TRADING_ALERT_REST_RECONCILE_SECS")
            && let Ok(secs) = v.parse::<u64>()
            && secs > 0
        {
            rest = secs.min(3600);
        }
        (Duration::from_secs(eval), Duration::from_secs(rest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telegram_debug_redacts_secrets() {
        let cfg = TelegramConfig {
            delivery_enabled: true,
            commands_enabled: true,
            bot_token: "SECRET_TOKEN".into(),
            chat_id: "99999".into(),
            poll_secs: 30,
            batch_limit: 20,
        };
        let dbg = format!("{cfg:?}");
        assert!(dbg.contains("[redacted]"));
        assert!(!dbg.contains("SECRET_TOKEN"));
        assert!(!dbg.contains("99999"));
    }

    #[test]
    fn dzengi_debug_redacts_keys() {
        let cfg = DzengiConfig {
            base_url: "https://example.test".into(),
            api_key: "KEY".into(),
            api_secret: "SECRET".into(),
        };
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("KEY"));
        assert!(!dbg.contains("SECRET"));
        assert!(dbg.contains("[redacted]"));
    }

    #[test]
    fn parse_flag_helpers() {
        assert!(!parse_enabled_flag(None));
        assert!(parse_enabled_flag(Some("1")));
        assert!(parse_enabled_flag(Some("YES")));
        assert!(!parse_enabled_flag(Some("0")));
    }

    #[test]
    fn resolve_single_bot_token() {
        let cfg = TelegramConfig {
            bot_token: "shared".into(),
            chat_id: "1".into(),
            delivery_enabled: true,
            commands_enabled: true,
            ..Default::default()
        };
        let r = cfg.resolve();
        assert_eq!(r.bot_token(), "shared");
        assert_eq!(r.primary_chat_id(), "1");
        assert!(r.credentials_ok());
        assert!(r.delivery_enabled);
        assert!(r.commands_enabled);
    }
}
