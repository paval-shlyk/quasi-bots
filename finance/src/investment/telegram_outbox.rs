//! Telegram alert outbox consumer (P1 Wave A4).
//!
//! Drains undelivered `alert_outbox` rows with `channel in (telegram, both)`,
//! sends a short movers-style message via Telegram Bot API, then marks
//! `delivered_telegram_at`. On failure retries with backoff.
//!
//! **Secrets:** mounted `[finance.telegram]` and/or `TELEGRAM_*` env (Vault).
//! Never log the bot token, never put it in MCP tool responses, never embed
//! it in error strings that reach logs (Telegram URLs contain the token —
//! construct them privately and log only status codes / event ids).
//!
//! Single bot with inbound commands (`telegram_commands`): this process must
//! be the sole `getUpdates` consumer for `bot_token`.
//!
//! **Not in scope:** `trading_notify_test` MCP tool (skipped for P1); sendMessage
//! inside quote/watch MCP tools.

use chrono::{DateTime, FixedOffset, Utc};
use std::time::Duration;
use tokio::time::MissedTickBehavior;

use crate::investment::config::{ResolvedTelegram, env_flag_or};
use crate::investment::watches::{
    AlertEvent, AlertOutboxRow, WatchCompare, WatchRule,
    list_pending_telegram_alerts, mark_telegram_delivered,
};

/// Default outbox poll interval (seconds).
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 30;
/// Soft cap per drain cycle.
pub const DEFAULT_BATCH_LIMIT: i64 = 20;
/// Initial backoff after a send failure (seconds).
pub const DEFAULT_FAILURE_BACKOFF_SECS: u64 = 5;
/// Max backoff after consecutive send failures (seconds).
pub const MAX_FAILURE_BACKOFF_SECS: u64 = 300;

/// Legacy env-only gate (prefer mounted `telegram.delivery_enabled` +
/// [`TelegramConfig::resolve`](crate::investment::TelegramConfig::resolve)).
pub fn telegram_delivery_enabled() -> bool {
    env_flag_or("TRADING_TELEGRAM_DELIVERY", false)
}

/// Credentials + poll knobs. Token/chat id never appear in [`Debug`].
#[derive(Clone)]
pub struct TelegramDeliveryConfig {
    bot_token: String,
    chat_id: String,
    pub poll_interval: Duration,
    pub batch_limit: i64,
    pub http_timeout: Duration,
}

impl std::fmt::Debug for TelegramDeliveryConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramDeliveryConfig")
            .field("bot_token", &"[redacted]")
            .field("chat_id", &"[redacted]")
            .field("poll_interval", &self.poll_interval)
            .field("batch_limit", &self.batch_limit)
            .field("http_timeout", &self.http_timeout)
            .finish()
    }
}

impl TelegramDeliveryConfig {
    /// Load from env. Returns `None` when token or chat id is missing/empty.
    ///
    /// Never logs secret values.
    pub fn from_env() -> Option<Self> {
        let bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        let chat_id = std::env::var("TELEGRAM_CHAT_ID")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;

        let mut cfg = Self {
            bot_token,
            chat_id,
            poll_interval: Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS),
            batch_limit: DEFAULT_BATCH_LIMIT,
            http_timeout: Duration::from_secs(15),
        };
        if let Ok(v) = std::env::var("TRADING_TELEGRAM_POLL_SECS")
            && let Ok(secs) = v.parse::<u64>()
            && secs > 0
        {
            cfg.poll_interval = Duration::from_secs(secs.min(600));
        }
        if let Ok(v) = std::env::var("TRADING_TELEGRAM_BATCH_LIMIT")
            && let Ok(n) = v.parse::<i64>()
            && n > 0
        {
            cfg.batch_limit = n.min(100);
        }
        Some(cfg)
    }

    /// Build from resolved mounted config (+ env). Uses **delivery** bot token.
    pub fn from_resolved(resolved: &ResolvedTelegram) -> Option<Self> {
        if !resolved.credentials_ok() {
            return None;
        }
        Some(Self {
            bot_token: resolved.bot_token().to_string(),
            chat_id: resolved.primary_chat_id().to_string(),
            poll_interval: resolved.poll_interval,
            batch_limit: resolved.batch_limit,
            http_timeout: resolved.http_timeout,
        })
    }
}

/// Europe/Minsk is UTC+3 year-round (no DST).
fn msk_offset() -> FixedOffset {
    FixedOffset::east_opt(3 * 3600).expect("MSK offset")
}

fn compare_glyph(compare: WatchCompare) -> &'static str {
    match compare {
        WatchCompare::Lte => "≤",
        WatchCompare::Gte => "≥",
        WatchCompare::AbsGte => "|·|≥",
    }
}

fn rule_short_label(rule: WatchRule) -> &'static str {
    match rule {
        WatchRule::DayChangePct => "day",
        WatchRule::MarkVsEntryPct => "vs entry",
        WatchRule::NavDayChangePct => "nav day",
        WatchRule::CashBelow => "cash",
        WatchRule::WeightBookPctAbove => "weight",
    }
}

fn format_observed(rule: WatchRule, observed: f64) -> String {
    match rule {
        WatchRule::DayChangePct
        | WatchRule::MarkVsEntryPct
        | WatchRule::NavDayChangePct
        | WatchRule::WeightBookPctAbove => {
            format!("{observed:.1}%")
        }
        WatchRule::CashBelow => {
            if observed.abs() >= 1000.0 {
                format!("{observed:.0}")
            } else {
                format!("{observed:.2}")
            }
        }
    }
}

fn format_threshold(rule: WatchRule, threshold: f64) -> String {
    match rule {
        WatchRule::DayChangePct
        | WatchRule::MarkVsEntryPct
        | WatchRule::NavDayChangePct
        | WatchRule::WeightBookPctAbove => {
            format!("{threshold:.0}%")
        }
        WatchRule::CashBelow => {
            if threshold.abs() >= 1000.0 {
                format!("{threshold:.0}")
            } else {
                format!("{threshold:.2}")
            }
        }
    }
}

/// Short movers-style Telegram body (design §A4 template).
pub fn format_telegram_message(event: &AlertEvent) -> String {
    let glyph = compare_glyph(event.compare);
    let label = rule_short_label(event.rule);
    let observed = format_observed(event.rule, event.observed);
    let threshold = format_threshold(event.rule, event.threshold);

    let headline = match event.symbol.as_deref() {
        Some(sym) => {
            format!("⚠ {sym} {label} {observed} ({glyph} {threshold})")
        }
        None => {
            format!("⚠ {label} {observed} ({glyph} {threshold})")
        }
    };

    let mut details = Vec::new();
    if let Some(mark) = event.mark.filter(|m| m.is_finite()) {
        details.push(format!("mark {mark:.2}"));
    }
    if let Some(nav) = event.nav.filter(|n| n.is_finite()) {
        // Design template shows whole units without rounding up (e.g. 5878.72 → 5878).
        details.push(format!("nav {}", nav.trunc() as i64));
    }
    let detail_line = if details.is_empty() {
        None
    } else {
        Some(details.join(" · "))
    };

    let ts_line = format_msk_ts(event.ts);

    let body = match detail_line {
        Some(d) => format!("{headline}\n{d}\n{ts_line}"),
        None => format!("{headline}\n{ts_line}"),
    };
    // Prefix so shared-chat / single-bot traffic is recognizable vs other bots.
    format!("[trading]\n{body}")
}

fn format_msk_ts(ts: DateTime<Utc>) -> String {
    let local = ts.with_timezone(&msk_offset());
    local.format("%Y-%m-%d %H:%M MSK").to_string()
}

/// Build Bot API URL privately. Caller must **never** log this string.
pub(crate) fn send_message_url(bot_token: &str) -> String {
    format!("https://api.telegram.org/bot{bot_token}/sendMessage")
}

/// POST `sendMessage` to the configured delivery chat.
pub async fn send_telegram_text(
    cfg: &TelegramDeliveryConfig,
    text: &str,
) -> anyhow::Result<()> {
    send_telegram_chat_text(cfg, &cfg.chat_id, text).await
}

/// POST `sendMessage` to an explicit chat id (allowlisted inbound replies).
/// On error, returns a message that does **not** include the bot token or URL.
pub async fn send_telegram_chat_text(
    cfg: &TelegramDeliveryConfig,
    chat_id: &str,
    text: &str,
) -> anyhow::Result<()> {
    send_telegram_bot_text(&cfg.bot_token, chat_id, text, cfg.http_timeout)
        .await
}

/// Low-level `sendMessage` with an explicit bot token (commands may use a
/// dedicated token). Never log `bot_token` or the request URL.
pub async fn send_telegram_bot_text(
    bot_token: &str,
    chat_id: &str,
    text: &str,
    http_timeout: Duration,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder().timeout(http_timeout).build()?;

    // URL contains the token — keep it in a local only; never put in tracing.
    let url = send_message_url(bot_token);
    let body = serde_json::json!({
        "chat_id": chat_id,
        "text": text,
        "disable_web_page_preview": true,
    });

    let response = client.post(&url).json(&body).send().await.map_err(|e| {
        // reqwest errors can embed the URL (and thus the token). Sanitize.
        anyhow::anyhow!(
            "telegram sendMessage transport error (url redacted): {}",
            sanitize_telegram_error(&e.to_string(), bot_token)
        )
    })?;

    let status = response.status();
    let resp_body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(
            "telegram sendMessage HTTP {} (body redacted/truncated): {}",
            status.as_u16(),
            truncate_for_log(
                &sanitize_telegram_error(&resp_body, bot_token),
                200
            )
        );
    }

    // Bot API returns JSON `{ ok: true/false, ... }` even on HTTP 200 for some cases.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resp_body)
        && v.get("ok").and_then(|o| o.as_bool()) == Some(false)
    {
        let desc = v
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("ok=false");
        anyhow::bail!(
            "telegram sendMessage rejected: {}",
            sanitize_telegram_error(desc, bot_token)
        );
    }

    Ok(())
}

pub(crate) fn sanitize_telegram_error(raw: &str, bot_token: &str) -> String {
    let mut out = raw.replace(bot_token, "[redacted]");
    // Also scrub URL-shaped bot paths if token somehow differed.
    if let Some(idx) = out.find("api.telegram.org/bot") {
        // Replace from /bot… until next / or whitespace with /bot[redacted]
        let rest = &out[idx..];
        if let Some(end) = rest.find([' ', '"', '\n', '\r', '?']) {
            let scrubbed = format!(
                "{}api.telegram.org/bot[redacted]{}",
                &out[..idx],
                &rest[end..]
            );
            out = scrubbed;
        } else {
            out = format!("{}api.telegram.org/bot[redacted]", &out[..idx]);
        }
    }
    out
}

pub(crate) fn truncate_for_log(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{truncated}…")
    }
}

/// Deliver one outbox row: send then mark. Idempotent mark.
pub async fn deliver_one(
    pool: &sqlx::SqlitePool,
    cfg: &TelegramDeliveryConfig,
    row: &AlertOutboxRow,
) -> anyhow::Result<()> {
    if row.delivered_telegram_at.is_some() {
        return Ok(());
    }
    let text = format_telegram_message(&row.event);
    send_telegram_text(cfg, &text).await?;
    let marked = mark_telegram_delivered(pool, &row.event_id).await?;
    if !marked {
        // Another worker raced us — treat as success (idempotent).
        tracing::debug!(
            event_id = %row.event_id,
            "telegram already marked delivered (idempotent)"
        );
    }
    Ok(())
}

/// Entry point for skill-master. Never returns.
pub async fn run_telegram_outbox_consumer(
    pool: sqlx::SqlitePool,
    config: TelegramDeliveryConfig,
) {
    tracing::info!(
        poll_interval_secs = config.poll_interval.as_secs(),
        batch_limit = config.batch_limit,
        "trading telegram outbox consumer starting (token/chat redacted; no MCP)"
    );

    let mut interval = tokio::time::interval(config.poll_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut failure_backoff = Duration::from_secs(DEFAULT_FAILURE_BACKOFF_SECS);

    loop {
        interval.tick().await;
        match drain_once(&pool, &config).await {
            Ok(0) => {
                failure_backoff =
                    Duration::from_secs(DEFAULT_FAILURE_BACKOFF_SECS);
            }
            Ok(n) => {
                tracing::info!(
                    delivered = n,
                    "telegram outbox drain cycle complete"
                );
                failure_backoff =
                    Duration::from_secs(DEFAULT_FAILURE_BACKOFF_SECS);
            }
            Err(e) => {
                // Error strings are sanitized in send path; still avoid dumping raw.
                tracing::warn!(
                    error = %e,
                    backoff_secs = failure_backoff.as_secs(),
                    "telegram outbox drain failed; backing off"
                );
                tokio::time::sleep(failure_backoff).await;
                failure_backoff = (failure_backoff * 2)
                    .min(Duration::from_secs(MAX_FAILURE_BACKOFF_SECS));
            }
        }
    }
}

/// One poll: list pending telegram|both, send each, mark delivered.
/// Returns number delivered. First send failure aborts the batch (remaining
/// stay pending for retry) so we don't hammer Telegram after auth/rate issues.
pub async fn drain_once(
    pool: &sqlx::SqlitePool,
    config: &TelegramDeliveryConfig,
) -> anyhow::Result<usize> {
    let pending =
        list_pending_telegram_alerts(pool, Some(config.batch_limit)).await?;
    let mut delivered = 0_usize;
    for row in pending {
        match deliver_one(pool, config, &row).await {
            Ok(()) => {
                delivered += 1;
                tracing::info!(
                    event_id = %row.event_id,
                    watch_id = row.watch_id,
                    symbol = ?row.event.symbol,
                    rule = %row.event.rule,
                    "telegram alert delivered"
                );
            }
            Err(e) => {
                return Err(e.context(format!(
                    "failed delivering event_id={} (token redacted)",
                    row.event_id
                )));
            }
        }
    }
    Ok(delivered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investment::config::parse_enabled_flag;
    use crate::investment::watches::{
        EvalSnapshot, SymbolObservation, UpsertWatch, WatchChannel,
        WatchCompare, WatchRule, list_pending_telegram_alerts,
        mark_telegram_delivered, try_fire_watch, upsert_watch,
    };

    #[test]
    fn delivery_flag_parse() {
        assert!(!parse_enabled_flag(None));
        assert!(!parse_enabled_flag(Some("0")));
        assert!(!parse_enabled_flag(Some("false")));
        assert!(parse_enabled_flag(Some("1")));
        assert!(parse_enabled_flag(Some("true")));
        assert!(parse_enabled_flag(Some("YES")));
    }

    #[test]
    fn config_debug_redacts_secrets() {
        let cfg = TelegramDeliveryConfig {
            bot_token: "SECRET_TOKEN_VALUE".into(),
            chat_id: "12345".into(),
            poll_interval: Duration::from_secs(30),
            batch_limit: 20,
            http_timeout: Duration::from_secs(15),
        };
        let dbg = format!("{cfg:?}");
        assert!(dbg.contains("[redacted]"));
        assert!(!dbg.contains("SECRET_TOKEN_VALUE"));
        assert!(!dbg.contains("12345"));
    }

    #[test]
    fn sanitize_strips_token_and_bot_url() {
        let token = "123456:ABC-DEF";
        let raw = format!(
            "error sending request for url (https://api.telegram.org/bot{token}/sendMessage): boom"
        );
        let cleaned = sanitize_telegram_error(&raw, token);
        assert!(!cleaned.contains(token));
        assert!(cleaned.contains("[redacted]"));
    }

    #[test]
    fn message_template_day_change_msk() {
        let ts = DateTime::parse_from_rfc3339("2026-09-07T00:05:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let event = AlertEvent {
            v: 1,
            id: "evt_1".into(),
            event_type: "watch.fired".into(),
            rule: WatchRule::DayChangePct,
            symbol: Some("TSLA".into()),
            threshold: -5.0,
            compare: WatchCompare::Lte,
            observed: -6.2,
            mark: Some(352.68),
            nav: Some(5878.72),
            message: "TSLA day_change_pct …".into(),
            ts,
        };
        let text = format_telegram_message(&event);
        assert!(text.starts_with("[trading]\n"), "got: {text}");
        assert!(text.contains("⚠ TSLA day -6.2% (≤ -5%)"), "got: {text}");
        assert!(text.contains("mark 352.68"), "got: {text}");
        assert!(text.contains("nav 5878"), "got: {text}");
        // 00:05 UTC → 03:05 MSK
        assert!(text.contains("2026-09-07 03:05 MSK"), "got: {text}");
        assert!(!text.contains("SECRET"));
        assert!(!text.contains("token"));
    }

    #[test]
    fn message_portfolio_rule_no_symbol() {
        let event = AlertEvent {
            v: 1,
            id: "evt_2".into(),
            event_type: "watch.fired".into(),
            rule: WatchRule::CashBelow,
            symbol: None,
            threshold: 100.0,
            compare: WatchCompare::Lte,
            observed: 42.5,
            mark: None,
            nav: Some(5000.0),
            message: "cash_below …".into(),
            ts: Utc::now(),
        };
        let text = format_telegram_message(&event);
        assert!(text.starts_with("[trading]\n"), "got: {text}");
        assert!(text.contains("⚠ cash "), "got: {text}");
        assert!(text.contains("nav 5000"), "got: {text}");
    }

    async fn setup_pool() -> sqlx::SqlitePool {
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
            CREATE TABLE alert_outbox (
                id INTEGER PRIMARY KEY,
                event_id TEXT NOT NULL UNIQUE,
                watch_id INTEGER NOT NULL,
                channel TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL,
                delivered_telegram_at TEXT NULL,
                acked_mcp_at TEXT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn pending_telegram_skips_mcp_only_and_mark_is_idempotent() {
        let pool = setup_pool().await;

        let tg = upsert_watch(
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

        let mcp = upsert_watch(
            &pool,
            UpsertWatch {
                id: None,
                symbol: Some("GOLD".into()),
                rule: WatchRule::DayChangePct,
                threshold: -5.0,
                compare: WatchCompare::Lte,
                channel: WatchChannel::Mcp,
                cooldown_secs: Some(60),
                enabled: Some(true),
            },
        )
        .await
        .unwrap();

        let both = upsert_watch(
            &pool,
            UpsertWatch {
                id: None,
                symbol: Some("US500".into()),
                rule: WatchRule::DayChangePct,
                threshold: -3.0,
                compare: WatchCompare::Lte,
                channel: WatchChannel::Both,
                cooldown_secs: Some(60),
                enabled: Some(true),
            },
        )
        .await
        .unwrap();

        let mut snap = EvalSnapshot {
            now: Utc::now(),
            nav: Some(5878.0),
            ..Default::default()
        };
        for (sym, pct, mark) in [
            ("TSLA", -6.2, 352.68),
            ("GOLD", -7.0, 2400.0),
            ("US500", -4.0, 5000.0),
        ] {
            snap.symbols.insert(
                sym.into(),
                SymbolObservation {
                    change_pct: Some(pct),
                    mark: Some(mark),
                    ..Default::default()
                },
            );
        }

        try_fire_watch(&pool, &tg, &snap).await.unwrap().unwrap();
        try_fire_watch(&pool, &mcp, &snap).await.unwrap().unwrap();
        try_fire_watch(&pool, &both, &snap).await.unwrap().unwrap();

        let pending =
            list_pending_telegram_alerts(&pool, Some(50)).await.unwrap();
        assert_eq!(pending.len(), 2, "mcp-only must be skipped");
        let channels: Vec<_> = pending.iter().map(|r| r.channel).collect();
        assert!(channels.contains(&WatchChannel::Telegram));
        assert!(channels.contains(&WatchChannel::Both));
        assert!(!channels.contains(&WatchChannel::Mcp));

        let event_id = pending[0].event_id.clone();
        assert!(mark_telegram_delivered(&pool, &event_id).await.unwrap());
        assert!(!mark_telegram_delivered(&pool, &event_id).await.unwrap());

        let pending2 =
            list_pending_telegram_alerts(&pool, Some(50)).await.unwrap();
        assert_eq!(pending2.len(), 1);
        assert_ne!(pending2[0].event_id, event_id);
    }
}
