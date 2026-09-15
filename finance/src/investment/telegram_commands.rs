//! Telegram inbound commands (P1 Wave A4.5) — finance-direct, never MCP.
//!
//! Long-polls Bot API `getUpdates`, allowlists chat ids, replies with short
//! `/status` `/cash` `/watches` `/help` text. No trade/order commands.
//!
//! **Gate:** mounted `telegram.commands_enabled` and/or
//! `TRADING_TELEGRAM_COMMANDS=1` (independent of delivery).
//!
//! **Single bot:** same `bot_token` + `chat_id` as outbound alerts. This
//! skill-master instance must be the **sole `getUpdates` consumer** for that
//! token (Telegram allows only one poller per bot).
//!
//! Secrets never logged / never in MCP.

use std::collections::HashSet;
use std::time::Duration;

use crate::investment::config::ResolvedTelegram;
use crate::investment::rest_api::RestClient;
use crate::investment::routes::fetch_portfolio_headline;
use crate::investment::telegram_outbox::{
    sanitize_telegram_error, send_telegram_bot_text, truncate_for_log,
};
use crate::investment::watches::{Watch, WatchCompare, list_enabled_watches};

/// Default long-poll timeout for `getUpdates` (seconds).
pub const DEFAULT_LONG_POLL_SECS: u64 = 25;

/// Runtime config for the inbound commands worker.
#[derive(Clone)]
pub struct TelegramCommandsConfig {
    bot_token: String,
    allowlist: Vec<String>,
    pub http_timeout: Duration,
    pub long_poll_timeout_secs: u64,
}

impl std::fmt::Debug for TelegramCommandsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramCommandsConfig")
            .field("bot_token", &"[redacted]")
            .field("allowlist_len", &self.allowlist.len())
            .field("http_timeout", &self.http_timeout)
            .field("long_poll_timeout_secs", &self.long_poll_timeout_secs)
            .finish()
    }
}

impl TelegramCommandsConfig {
    pub fn from_resolved(resolved: &ResolvedTelegram) -> Option<Self> {
        if !resolved.credentials_ok() {
            return None;
        }
        let long_poll =
            DEFAULT_LONG_POLL_SECS.min(resolved.poll_interval.as_secs().max(1));
        Some(Self {
            bot_token: resolved.bot_token().to_string(),
            allowlist: resolved.allowlist.clone(),
            http_timeout: resolved.http_timeout
                + Duration::from_secs(long_poll + 5),
            long_poll_timeout_secs: long_poll,
        })
    }

    pub fn chat_allowed(&self, chat_id: &str) -> bool {
        self.allowlist.iter().any(|id| id == chat_id)
    }
}

/// Legacy env-only gate (prefer mounted `commands_enabled` + resolve).
pub fn telegram_commands_enabled() -> bool {
    crate::investment::config::env_flag_or("TRADING_TELEGRAM_COMMANDS", false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramCommand {
    Status,
    Cash,
    Watches,
    Help,
}

/// Parse `/status`, `/cash`, `/watches`, `/help` (optional `@BotName` suffix).
pub fn parse_command(text: &str) -> Option<TelegramCommand> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first = trimmed.split_whitespace().next()?;
    let base = first.split('@').next().unwrap_or(first);
    match base {
        "/status" => Some(TelegramCommand::Status),
        "/cash" => Some(TelegramCommand::Cash),
        "/watches" => Some(TelegramCommand::Watches),
        "/help" => Some(TelegramCommand::Help),
        _ => None,
    }
}

fn fmt_opt_money(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => format!("{x:.2}"),
        _ => "n/a".into(),
    }
}

fn fmt_money(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.2}")
    } else {
        "n/a".into()
    }
}

fn compare_glyph(compare: WatchCompare) -> &'static str {
    match compare {
        WatchCompare::Lte => "≤",
        WatchCompare::Gte => "≥",
        WatchCompare::AbsGte => "|·|≥",
    }
}

/// Short `/status` body (finance-direct headline fields).
pub fn format_status_reply(
    equity: Option<f64>,
    cash_available: Option<f64>,
    cash_locked: Option<f64>,
    gross_exposure: f64,
    unrealized_pnl: f64,
) -> String {
    format!(
        "equity {}\ncash_available {}\ncash_locked {}\ngross_exposure {}\nunrealized_pnl {}",
        fmt_opt_money(equity),
        fmt_opt_money(cash_available),
        fmt_opt_money(cash_locked),
        fmt_money(gross_exposure),
        fmt_money(unrealized_pnl),
    )
}

/// Short `/cash` body.
pub fn format_cash_reply(
    cash_available: Option<f64>,
    cash_locked: Option<f64>,
) -> String {
    format!(
        "free {}\nlocked {}",
        fmt_opt_money(cash_available),
        fmt_opt_money(cash_locked),
    )
}

/// Short `/watches` summary of enabled watches.
pub fn format_watches_reply(watches: &[Watch]) -> String {
    if watches.is_empty() {
        return "0 enabled watches".into();
    }
    let mut lines = Vec::with_capacity(watches.len() + 1);
    lines.push(format!("{} enabled", watches.len()));
    for w in watches {
        let sym = w.symbol.as_deref().unwrap_or("—");
        let glyph = compare_glyph(w.compare);
        lines.push(format!(
            "#{} {} {} {} {} {}",
            w.id,
            sym,
            w.rule.as_str(),
            glyph,
            w.threshold,
            w.channel.as_str(),
        ));
    }
    lines.join("\n")
}

pub fn format_help_reply() -> String {
    String::from(
        "/status — equity, cash, exposure, uPnL\n/cash — free vs locked\n/watches — enabled watches\n/help",
    )
}

fn get_updates_url(bot_token: &str) -> String {
    format!("https://api.telegram.org/bot{bot_token}/getUpdates")
}

/// One `getUpdates` long-poll. Returns raw updates array (may be empty).
async fn fetch_updates(
    cfg: &TelegramCommandsConfig,
    offset: Option<i64>,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let client = reqwest::Client::builder()
        .timeout(cfg.http_timeout)
        .build()?;

    let url = get_updates_url(&cfg.bot_token);
    let mut body = serde_json::json!({
        "timeout": cfg.long_poll_timeout_secs,
        "allowed_updates": ["message"],
    });
    if let Some(off) = offset {
        body["offset"] = serde_json::json!(off);
    }

    let response = client.post(&url).json(&body).send().await.map_err(|e| {
        anyhow::anyhow!(
            "telegram getUpdates transport error (url redacted): {}",
            sanitize_telegram_error(&e.to_string(), &cfg.bot_token)
        )
    })?;

    let status = response.status();
    let resp_body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(
            "telegram getUpdates HTTP {} (body redacted/truncated): {}",
            status.as_u16(),
            truncate_for_log(
                &sanitize_telegram_error(&resp_body, &cfg.bot_token),
                200
            )
        );
    }

    let v: serde_json::Value = serde_json::from_str(&resp_body)
        .map_err(|e| anyhow::anyhow!("telegram getUpdates JSON error: {e}"))?;
    if v.get("ok").and_then(|o| o.as_bool()) != Some(true) {
        let desc = v
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("ok=false");
        anyhow::bail!(
            "telegram getUpdates rejected: {}",
            sanitize_telegram_error(desc, &cfg.bot_token)
        );
    }
    let updates = v
        .get("result")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(updates)
}

struct InboundMessage {
    update_id: i64,
    chat_id: String,
    text: String,
}

fn parse_inbound(update: &serde_json::Value) -> Option<InboundMessage> {
    let update_id = update.get("update_id")?.as_i64()?;
    let message = update.get("message")?;
    let chat_id = message.get("chat")?.get("id").and_then(|id| {
        id.as_i64()
            .map(|n| n.to_string())
            .or_else(|| id.as_str().map(str::to_string))
    })?;
    let text = message.get("text")?.as_str()?.to_string();
    Some(InboundMessage {
        update_id,
        chat_id,
        text,
    })
}

async fn build_reply(
    pool: &sqlx::SqlitePool,
    api: &RestClient,
    cmd: TelegramCommand,
) -> anyhow::Result<String> {
    match cmd {
        TelegramCommand::Help => Ok(format_help_reply()),
        TelegramCommand::Status => {
            let h = fetch_portfolio_headline(api).await?;
            Ok(format_status_reply(
                h.equity,
                h.cash_available,
                h.cash_locked,
                h.gross_exposure,
                h.unrealized_pnl,
            ))
        }
        TelegramCommand::Cash => {
            let h = fetch_portfolio_headline(api).await?;
            Ok(format_cash_reply(h.cash_available, h.cash_locked))
        }
        TelegramCommand::Watches => {
            let watches = list_enabled_watches(pool).await?;
            Ok(format_watches_reply(&watches))
        }
    }
}

/// Handle one inbound update. Returns next offset hint (`update_id + 1`) when
/// the update was consumed (including ignored foreign chats / unknown cmds).
pub async fn handle_update(
    pool: &sqlx::SqlitePool,
    api: &RestClient,
    cfg: &TelegramCommandsConfig,
    update: &serde_json::Value,
    seen: &mut HashSet<i64>,
) -> anyhow::Result<Option<i64>> {
    let Some(msg) = parse_inbound(update) else {
        let next = update
            .get("update_id")
            .and_then(|u| u.as_i64())
            .map(|id| id + 1);
        return Ok(next);
    };

    if !seen.insert(msg.update_id) {
        return Ok(Some(msg.update_id + 1));
    }

    if !cfg.chat_allowed(&msg.chat_id) {
        tracing::debug!(
            update_id = msg.update_id,
            "telegram command ignored (chat not allowlisted)"
        );
        return Ok(Some(msg.update_id + 1));
    }

    let Some(cmd) = parse_command(&msg.text) else {
        if msg.text.trim().starts_with('/') {
            let _ = send_telegram_bot_text(
                &cfg.bot_token,
                &msg.chat_id,
                "unknown command — try /help",
                cfg.http_timeout,
            )
            .await;
        }
        return Ok(Some(msg.update_id + 1));
    };

    match build_reply(pool, api, cmd).await {
        Ok(reply) => {
            send_telegram_bot_text(
                &cfg.bot_token,
                &msg.chat_id,
                &reply,
                cfg.http_timeout,
            )
            .await?;
            tracing::info!(
                update_id = msg.update_id,
                command = ?cmd,
                "telegram command replied (token redacted)"
            );
        }
        Err(e) => {
            tracing::warn!(
                update_id = msg.update_id,
                command = ?cmd,
                error = %e,
                "telegram command handler failed"
            );
            let _ = send_telegram_bot_text(
                &cfg.bot_token,
                &msg.chat_id,
                "error fetching data — try again",
                cfg.http_timeout,
            )
            .await;
        }
    }

    Ok(Some(msg.update_id + 1))
}

/// Entry point for skill-master. Never returns.
///
/// Sole `getUpdates` poller for the configured bot token — do not run another
/// getUpdates consumer against the same token.
pub async fn run_telegram_commands_worker(
    pool: sqlx::SqlitePool,
    api: RestClient,
    config: TelegramCommandsConfig,
) {
    tracing::info!(
        long_poll_secs = config.long_poll_timeout_secs,
        allowlist_len = config.allowlist.len(),
        "trading telegram commands worker starting (sole getUpdates; token/chat redacted; no MCP)"
    );

    let mut offset: Option<i64> = None;
    let mut seen: HashSet<i64> = HashSet::new();
    let mut failure_backoff = Duration::from_secs(5);

    loop {
        match fetch_updates(&config, offset).await {
            Ok(updates) => {
                failure_backoff = Duration::from_secs(5);
                if updates.is_empty() {
                    continue;
                }
                for update in &updates {
                    match handle_update(&pool, &api, &config, update, &mut seen)
                        .await
                    {
                        Ok(Some(next)) => {
                            offset = Some(next);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "telegram command update handling error"
                            );
                            if let Some(id) =
                                update.get("update_id").and_then(|u| u.as_i64())
                            {
                                offset = Some(id + 1);
                            }
                        }
                    }
                }
                if seen.len() > 2000 {
                    seen.clear();
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    backoff_secs = failure_backoff.as_secs(),
                    "telegram getUpdates failed; backing off"
                );
                tokio::time::sleep(failure_backoff).await;
                failure_backoff =
                    (failure_backoff * 2).min(Duration::from_secs(300));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investment::telegram_outbox::{
        sanitize_telegram_error, send_message_url,
    };
    use crate::investment::watches::{
        UpsertWatch, WatchChannel, WatchCompare, WatchRule, upsert_watch,
    };

    #[test]
    fn parse_commands_with_bot_suffix() {
        assert_eq!(parse_command("/status"), Some(TelegramCommand::Status));
        assert_eq!(parse_command(" /cash@MyBot "), Some(TelegramCommand::Cash));
        assert_eq!(parse_command("/watches@x"), Some(TelegramCommand::Watches));
        assert_eq!(parse_command("/help"), Some(TelegramCommand::Help));
        assert_eq!(parse_command("/buy"), None);
        assert_eq!(parse_command("hello"), None);
    }

    #[test]
    fn status_and_cash_format_short() {
        let s = format_status_reply(
            Some(5878.72),
            Some(100.5),
            Some(20.0),
            5000.25,
            -10.5,
        );
        assert!(s.contains("equity 5878.72"));
        assert!(s.contains("cash_available 100.50"));
        assert!(s.contains("cash_locked 20.00"));
        assert!(s.contains("gross_exposure 5000.25"));
        assert!(s.contains("unrealized_pnl -10.50"));
        assert!(!s.contains("token"));

        let c = format_cash_reply(Some(100.0), Some(20.0));
        assert_eq!(c, "free 100.00\nlocked 20.00");
    }

    #[test]
    fn help_lists_only_safe_commands() {
        let h = format_help_reply();
        assert!(h.contains("/status"));
        assert!(h.contains("/cash"));
        assert!(h.contains("/watches"));
        assert!(!h.contains("/buy"));
        assert!(!h.contains("order"));
    }

    #[test]
    fn commands_config_debug_redacts() {
        let cfg = TelegramCommandsConfig {
            bot_token: "SECRET".into(),
            allowlist: vec!["123".into()],
            http_timeout: Duration::from_secs(30),
            long_poll_timeout_secs: 25,
        };
        let dbg = format!("{cfg:?}");
        assert!(dbg.contains("[redacted]"));
        assert!(!dbg.contains("SECRET"));
        assert!(!dbg.contains("123"));
    }

    #[tokio::test]
    async fn watches_summary_lists_enabled_only() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE trading_watches (
                id INTEGER PRIMARY KEY,
                symbol TEXT NULL,
                rule TEXT NOT NULL,
                threshold REAL NOT NULL,
                compare TEXT NOT NULL,
                channel TEXT NOT NULL,
                cooldown_secs INTEGER NOT NULL DEFAULT 3600,
                enabled INTEGER NOT NULL DEFAULT 1,
                last_fired_at TEXT NULL,
                created_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        upsert_watch(
            &pool,
            UpsertWatch {
                id: None,
                symbol: Some("TSLA".into()),
                rule: WatchRule::DayChangePct,
                threshold: -5.0,
                compare: WatchCompare::Lte,
                channel: WatchChannel::Telegram,
                cooldown_secs: Some(60),
                enabled: Some(true),
            },
        )
        .await
        .unwrap();

        let disabled = upsert_watch(
            &pool,
            UpsertWatch {
                id: None,
                symbol: None,
                rule: WatchRule::CashBelow,
                threshold: 100.0,
                compare: WatchCompare::Lte,
                channel: WatchChannel::Both,
                cooldown_secs: Some(60),
                enabled: Some(false),
            },
        )
        .await
        .unwrap();
        assert!(!disabled.enabled);

        let enabled = list_enabled_watches(&pool).await.unwrap();
        let text = format_watches_reply(&enabled);
        assert!(text.starts_with("1 enabled"));
        assert!(text.contains("TSLA"));
        assert!(text.contains("day_change_pct"));
        assert!(!text.contains("cash_below"));
    }

    #[test]
    fn send_message_url_helper_still_tokenized() {
        let url = send_message_url("TOK");
        assert!(url.contains("TOK"));
        let cleaned = sanitize_telegram_error(&url, "TOK");
        assert!(!cleaned.contains("TOK"));
    }
}
