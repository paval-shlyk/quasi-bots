//! Watch store (A2) + alert outbox / evaluate hooks (A3 skeleton).
//!
//! Full Dzengi WS evaluator loop and Telegram delivery are follow-ups;
//! this module owns CRUD, schema, condition checks, and outbox insert.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default cooldown when upsert omits `cooldown_secs`.
pub const DEFAULT_COOLDOWN_SECS: i64 = 3600;

/// Watch rule kinds (P1 Wave A).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WatchRule {
    DayChangePct,
    MarkVsEntryPct,
    NavDayChangePct,
    CashBelow,
    WeightBookPctAbove,
}

impl WatchRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DayChangePct => "day_change_pct",
            Self::MarkVsEntryPct => "mark_vs_entry_pct",
            Self::NavDayChangePct => "nav_day_change_pct",
            Self::CashBelow => "cash_below",
            Self::WeightBookPctAbove => "weight_book_pct_above",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "day_change_pct" => Some(Self::DayChangePct),
            "mark_vs_entry_pct" => Some(Self::MarkVsEntryPct),
            "nav_day_change_pct" => Some(Self::NavDayChangePct),
            "cash_below" => Some(Self::CashBelow),
            "weight_book_pct_above" => Some(Self::WeightBookPctAbove),
            _ => None,
        }
    }

    /// Portfolio-level rules must have `symbol = null`; others require a symbol.
    pub fn requires_symbol(self) -> bool {
        !matches!(self, Self::NavDayChangePct | Self::CashBelow)
    }
}

impl std::fmt::Display for WatchRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WatchCompare {
    Lte,
    Gte,
    AbsGte,
}

impl WatchCompare {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lte => "lte",
            Self::Gte => "gte",
            Self::AbsGte => "abs_gte",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "lte" => Some(Self::Lte),
            "gte" => Some(Self::Gte),
            "abs_gte" => Some(Self::AbsGte),
            _ => None,
        }
    }

    pub fn holds(self, observed: f64, threshold: f64) -> bool {
        match self {
            Self::Lte => observed <= threshold,
            Self::Gte => observed >= threshold,
            Self::AbsGte => observed.abs() >= threshold,
        }
    }
}

impl std::fmt::Display for WatchCompare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WatchChannel {
    Telegram,
    Mcp,
    Both,
}

impl WatchChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Mcp => "mcp",
            Self::Both => "both",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "telegram" => Some(Self::Telegram),
            "mcp" => Some(Self::Mcp),
            "both" => Some(Self::Both),
            _ => None,
        }
    }
}

impl std::fmt::Display for WatchChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Persisted watch row.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Watch {
    pub id: i64,
    pub symbol: Option<String>,
    pub rule: WatchRule,
    pub threshold: f64,
    pub compare: WatchCompare,
    pub channel: WatchChannel,
    pub cooldown_secs: i64,
    pub enabled: bool,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WatchList {
    pub watches: Vec<Watch>,
}

/// Fields accepted by `trading_watches_upsert`.
#[derive(Debug, Clone)]
pub struct UpsertWatch {
    pub id: Option<i64>,
    pub symbol: Option<String>,
    pub rule: WatchRule,
    pub threshold: f64,
    pub compare: WatchCompare,
    pub channel: WatchChannel,
    pub cooldown_secs: Option<i64>,
    pub enabled: Option<bool>,
}

fn parse_rfc3339(raw: Option<String>) -> Option<DateTime<Utc>> {
    raw.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    })
}

#[allow(clippy::too_many_arguments)]
fn row_to_watch(
    id: i64,
    symbol: Option<String>,
    rule: String,
    threshold: f64,
    compare: String,
    channel: String,
    cooldown_secs: i64,
    enabled: i64,
    last_fired_at: Option<String>,
    created_at: String,
) -> anyhow::Result<Watch> {
    let rule = WatchRule::parse(&rule)
        .ok_or_else(|| anyhow::anyhow!("unknown watch rule: {rule}"))?;
    let compare = WatchCompare::parse(&compare)
        .ok_or_else(|| anyhow::anyhow!("unknown compare: {compare}"))?;
    let channel = WatchChannel::parse(&channel)
        .ok_or_else(|| anyhow::anyhow!("unknown channel: {channel}"))?;
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| anyhow::anyhow!("bad created_at: {e}"))?;
    Ok(Watch {
        id,
        symbol,
        rule,
        threshold,
        compare,
        channel,
        cooldown_secs,
        enabled: enabled != 0,
        last_fired_at: parse_rfc3339(last_fired_at),
        created_at,
    })
}

fn validate_upsert(input: &UpsertWatch) -> anyhow::Result<()> {
    if !input.threshold.is_finite() {
        anyhow::bail!("threshold must be finite");
    }
    let symbol = input
        .symbol
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if input.rule.requires_symbol() {
        if symbol.is_none() {
            anyhow::bail!(
                "rule {} requires a non-empty symbol",
                input.rule.as_str()
            );
        }
    } else if symbol.is_some() {
        anyhow::bail!(
            "rule {} is portfolio-level; symbol must be null",
            input.rule.as_str()
        );
    }
    if let Some(cd) = input.cooldown_secs
        && cd < 0
    {
        anyhow::bail!("cooldown_secs must be >= 0");
    }
    Ok(())
}

/// List all watches (enabled and disabled), newest first.
pub async fn list_watches(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<WatchList> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id AS "id!",
            symbol,
            rule AS "rule!",
            threshold AS "threshold!",
            compare AS "compare!",
            channel AS "channel!",
            cooldown_secs AS "cooldown_secs!",
            enabled AS "enabled!",
            last_fired_at,
            created_at AS "created_at!"
        FROM trading_watches
        ORDER BY id DESC
        "#
    )
    .fetch_all(pool)
    .await?;

    let mut watches = Vec::with_capacity(rows.len());
    for r in rows {
        watches.push(row_to_watch(
            r.id,
            r.symbol,
            r.rule,
            r.threshold,
            r.compare,
            r.channel,
            r.cooldown_secs,
            r.enabled,
            r.last_fired_at,
            r.created_at,
        )?);
    }
    Ok(WatchList { watches })
}

/// List only enabled watches (evaluator input).
pub async fn list_enabled_watches(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<Watch>> {
    let all = list_watches(pool).await?;
    Ok(all.watches.into_iter().filter(|w| w.enabled).collect())
}

/// Create or update one watch.
pub async fn upsert_watch(
    pool: &sqlx::SqlitePool,
    input: UpsertWatch,
) -> anyhow::Result<Watch> {
    validate_upsert(&input)?;

    let symbol = input
        .symbol
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let rule = input.rule.as_str().to_string();
    let compare = input.compare.as_str().to_string();
    let channel = input.channel.as_str().to_string();
    let cooldown = input.cooldown_secs.unwrap_or(DEFAULT_COOLDOWN_SECS);
    let enabled = if input.enabled.unwrap_or(true) {
        1_i64
    } else {
        0_i64
    };

    if let Some(id) = input.id {
        let updated = sqlx::query!(
            r#"
            UPDATE trading_watches
            SET
                symbol = ?,
                rule = ?,
                threshold = ?,
                compare = ?,
                channel = ?,
                cooldown_secs = ?,
                enabled = ?
            WHERE id = ?
            RETURNING
                id AS "id!",
                symbol,
                rule AS "rule!",
                threshold AS "threshold!",
                compare AS "compare!",
                channel AS "channel!",
                cooldown_secs AS "cooldown_secs!",
                enabled AS "enabled!",
                last_fired_at,
                created_at AS "created_at!"
            "#,
            symbol,
            rule,
            input.threshold,
            compare,
            channel,
            cooldown,
            enabled,
            id
        )
        .fetch_optional(pool)
        .await?;

        let Some(r) = updated else {
            anyhow::bail!("watch id {id} not found");
        };
        return row_to_watch(
            r.id,
            r.symbol,
            r.rule,
            r.threshold,
            r.compare,
            r.channel,
            r.cooldown_secs,
            r.enabled,
            r.last_fired_at,
            r.created_at,
        );
    }

    let created_at = Utc::now().to_rfc3339();
    let r = sqlx::query!(
        r#"
        INSERT INTO trading_watches (
            symbol, rule, threshold, compare, channel,
            cooldown_secs, enabled, last_fired_at, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?)
        RETURNING
            id AS "id!",
            symbol,
            rule AS "rule!",
            threshold AS "threshold!",
            compare AS "compare!",
            channel AS "channel!",
            cooldown_secs AS "cooldown_secs!",
            enabled AS "enabled!",
            last_fired_at,
            created_at AS "created_at!"
        "#,
        symbol,
        rule,
        input.threshold,
        compare,
        channel,
        cooldown,
        enabled,
        created_at
    )
    .fetch_one(pool)
    .await?;

    row_to_watch(
        r.id,
        r.symbol,
        r.rule,
        r.threshold,
        r.compare,
        r.channel,
        r.cooldown_secs,
        r.enabled,
        r.last_fired_at,
        r.created_at,
    )
}

/// Delete a watch by id. Returns true if a row was removed.
pub async fn delete_watch(
    pool: &sqlx::SqlitePool,
    id: i64,
) -> anyhow::Result<bool> {
    let res = sqlx::query!("DELETE FROM trading_watches WHERE id = ?", id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Enable or disable a watch. Returns the updated row.
pub async fn set_watch_enabled(
    pool: &sqlx::SqlitePool,
    id: i64,
    enabled: bool,
) -> anyhow::Result<Watch> {
    let enabled_i = if enabled { 1_i64 } else { 0_i64 };
    let r = sqlx::query!(
        r#"
        UPDATE trading_watches
        SET enabled = ?
        WHERE id = ?
        RETURNING
            id AS "id!",
            symbol,
            rule AS "rule!",
            threshold AS "threshold!",
            compare AS "compare!",
            channel AS "channel!",
            cooldown_secs AS "cooldown_secs!",
            enabled AS "enabled!",
            last_fired_at,
            created_at AS "created_at!"
        "#,
        enabled_i,
        id
    )
    .fetch_optional(pool)
    .await?;

    let Some(r) = r else {
        anyhow::bail!("watch id {id} not found");
    };
    row_to_watch(
        r.id,
        r.symbol,
        r.rule,
        r.threshold,
        r.compare,
        r.channel,
        r.cooldown_secs,
        r.enabled,
        r.last_fired_at,
        r.created_at,
    )
}

// ---------------------------------------------------------------------------
// Outbox + evaluate hooks (A3 skeleton; no WS worker / Telegram here)
// ---------------------------------------------------------------------------

/// Versioned alert event payload stored in `alert_outbox.payload`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AlertEvent {
    pub v: u32,
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub rule: WatchRule,
    pub symbol: Option<String>,
    pub threshold: f64,
    pub compare: WatchCompare,
    pub observed: f64,
    pub mark: Option<f64>,
    pub nav: Option<f64>,
    pub message: String,
    pub ts: DateTime<Utc>,
}

/// Outbox row visible to agents / Telegram consumer.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AlertOutboxRow {
    pub id: i64,
    pub event_id: String,
    pub watch_id: i64,
    pub channel: WatchChannel,
    pub event: AlertEvent,
    pub created_at: DateTime<Utc>,
    pub delivered_telegram_at: Option<DateTime<Utc>>,
    pub acked_mcp_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AlertList {
    pub alerts: Vec<AlertOutboxRow>,
}

/// Per-symbol observations for evaluation (filled by WS/REST worker later).
#[derive(Debug, Clone, Default)]
pub struct SymbolObservation {
    pub change_pct: Option<f64>,
    pub mark: Option<f64>,
    pub unrealized_pnl_pct: Option<f64>,
    pub weight_book_pct: Option<f64>,
}

/// Portfolio + quote snapshot for [`evaluate_watch`] / [`try_fire_watch`].
#[derive(Debug, Clone, Default)]
pub struct EvalSnapshot {
    pub symbols: HashMap<String, SymbolObservation>,
    pub cash: Option<f64>,
    pub nav: Option<f64>,
    pub nav_day_change_pct: Option<f64>,
    pub now: DateTime<Utc>,
}

/// Result of a successful condition check.
#[derive(Debug, Clone)]
pub struct FireObservation {
    pub observed: f64,
    pub mark: Option<f64>,
    pub message: String,
}

fn cooldown_elapsed(watch: &Watch, now: DateTime<Utc>) -> bool {
    match watch.last_fired_at {
        None => true,
        Some(last) => {
            let elapsed = (now - last).num_seconds();
            elapsed >= watch.cooldown_secs
        }
    }
}

fn lookup_symbol<'a>(
    snap: &'a EvalSnapshot,
    symbol: &str,
) -> Option<&'a SymbolObservation> {
    snap.symbols.get(symbol).or_else(|| {
        // Case-insensitive fallback for book vs quote symbol casing.
        snap.symbols
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(symbol))
            .map(|(_, v)| v)
    })
}

/// Pure condition check. Returns [`None`] when inputs are missing or false.
pub fn evaluate_watch(
    watch: &Watch,
    snap: &EvalSnapshot,
) -> Option<FireObservation> {
    let (observed, mark) = match watch.rule {
        WatchRule::DayChangePct => {
            let sym = watch.symbol.as_deref()?;
            let obs = lookup_symbol(snap, sym)?;
            (obs.change_pct?, obs.mark)
        }
        WatchRule::MarkVsEntryPct => {
            let sym = watch.symbol.as_deref()?;
            let obs = lookup_symbol(snap, sym)?;
            (obs.unrealized_pnl_pct?, obs.mark)
        }
        WatchRule::WeightBookPctAbove => {
            let sym = watch.symbol.as_deref()?;
            let obs = lookup_symbol(snap, sym)?;
            (obs.weight_book_pct?, obs.mark)
        }
        WatchRule::NavDayChangePct => (snap.nav_day_change_pct?, None),
        WatchRule::CashBelow => (snap.cash?, None),
    };

    if !watch.compare.holds(observed, watch.threshold) {
        return None;
    }

    let message = format_fire_message(watch, observed);
    Some(FireObservation {
        observed,
        mark,
        message,
    })
}

fn format_fire_message(watch: &Watch, observed: f64) -> String {
    let cmp = match watch.compare {
        WatchCompare::Lte => "≤",
        WatchCompare::Gte => "≥",
        WatchCompare::AbsGte => "|·|≥",
    };
    match watch.symbol.as_deref() {
        Some(sym) => format!(
            "{sym} {} {observed:.4} {cmp} {:.4}",
            watch.rule.as_str(),
            watch.threshold
        ),
        None => format!(
            "{} {observed:.4} {cmp} {:.4}",
            watch.rule.as_str(),
            watch.threshold
        ),
    }
}

fn new_event_id(watch_id: i64, ts: DateTime<Utc>) -> String {
    format!("evt_{watch_id}_{}", ts.timestamp_millis())
}

/// Insert an outbox row + bump `last_fired_at`. Caller must enforce enabled/cooldown.
pub async fn insert_alert(
    pool: &sqlx::SqlitePool,
    watch: &Watch,
    fire: &FireObservation,
    snap: &EvalSnapshot,
) -> anyhow::Result<AlertOutboxRow> {
    let ts = snap.now;
    let event_id = new_event_id(watch.id, ts);
    let event = AlertEvent {
        v: 1,
        id: event_id.clone(),
        event_type: "watch.fired".into(),
        rule: watch.rule,
        symbol: watch.symbol.clone(),
        threshold: watch.threshold,
        compare: watch.compare,
        observed: fire.observed,
        mark: fire.mark,
        nav: snap.nav,
        message: fire.message.clone(),
        ts,
    };
    let payload = serde_json::to_string(&event)?;
    let channel = watch.channel.as_str().to_string();
    let created_at = ts.to_rfc3339();
    let last_fired = created_at.clone();

    let mut tx = pool.begin().await?;

    let row = sqlx::query!(
        r#"
        INSERT INTO alert_outbox (
            event_id, watch_id, channel, payload, created_at,
            delivered_telegram_at, acked_mcp_at
        ) VALUES (?, ?, ?, ?, ?, NULL, NULL)
        RETURNING
            id AS "id!",
            event_id AS "event_id!",
            watch_id AS "watch_id!",
            channel AS "channel!",
            payload AS "payload!",
            created_at AS "created_at!",
            delivered_telegram_at,
            acked_mcp_at
        "#,
        event_id,
        watch.id,
        channel,
        payload,
        created_at
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        r#"
        UPDATE trading_watches
        SET last_fired_at = ?
        WHERE id = ?
        "#,
        last_fired,
        watch.id
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    outbox_row_from_parts(
        row.id,
        row.event_id,
        row.watch_id,
        row.channel,
        row.payload,
        row.created_at,
        row.delivered_telegram_at,
        row.acked_mcp_at,
    )
}

#[allow(clippy::too_many_arguments)]
fn outbox_row_from_parts(
    id: i64,
    event_id: String,
    watch_id: i64,
    channel: String,
    payload: String,
    created_at: String,
    delivered_telegram_at: Option<String>,
    acked_mcp_at: Option<String>,
) -> anyhow::Result<AlertOutboxRow> {
    let channel = WatchChannel::parse(&channel)
        .ok_or_else(|| anyhow::anyhow!("unknown channel: {channel}"))?;
    let event: AlertEvent = serde_json::from_str(&payload)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| anyhow::anyhow!("bad created_at: {e}"))?;
    Ok(AlertOutboxRow {
        id,
        event_id,
        watch_id,
        channel,
        event,
        created_at,
        delivered_telegram_at: parse_rfc3339(delivered_telegram_at),
        acked_mcp_at: parse_rfc3339(acked_mcp_at),
    })
}

/// Evaluate one watch and insert outbox if enabled, condition true, cooldown ok.
///
/// Disabled watches never fire (acceptance). Returns `Ok(None)` when skipped.
pub async fn try_fire_watch(
    pool: &sqlx::SqlitePool,
    watch: &Watch,
    snap: &EvalSnapshot,
) -> anyhow::Result<Option<AlertOutboxRow>> {
    if !watch.enabled {
        return Ok(None);
    }
    if !cooldown_elapsed(watch, snap.now) {
        return Ok(None);
    }
    let Some(fire) = evaluate_watch(watch, snap) else {
        return Ok(None);
    };
    let row = insert_alert(pool, watch, &fire, snap).await?;
    Ok(Some(row))
}

/// Load enabled watches and attempt fire for each against `snap`.
pub async fn evaluate_enabled_watches(
    pool: &sqlx::SqlitePool,
    snap: &EvalSnapshot,
) -> anyhow::Result<Vec<AlertOutboxRow>> {
    let watches = list_enabled_watches(pool).await?;
    let mut fired = Vec::new();
    for watch in watches {
        if let Some(row) = try_fire_watch(pool, &watch, snap).await? {
            fired.push(row);
        }
    }
    Ok(fired)
}

/// List recent outbox rows for agents (`trading_alerts_list`).
pub async fn list_alerts(
    pool: &sqlx::SqlitePool,
    since: Option<DateTime<Utc>>,
    limit: Option<i64>,
    pending_mcp_only: bool,
) -> anyhow::Result<AlertList> {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let since_s = since.map(|t| t.to_rfc3339());
    // Single query so sqlx::query! Record types unify across filter branches.
    let pending_i = if pending_mcp_only { 1_i64 } else { 0_i64 };

    let rows = sqlx::query!(
        r#"
        SELECT
            id AS "id!",
            event_id AS "event_id!",
            watch_id AS "watch_id!",
            channel AS "channel!",
            payload AS "payload!",
            created_at AS "created_at!",
            delivered_telegram_at,
            acked_mcp_at
        FROM alert_outbox
        WHERE (?1 IS NULL OR created_at >= ?1)
          AND (
            ?2 = 0
            OR (
                acked_mcp_at IS NULL
                AND channel IN ('mcp', 'both')
            )
          )
        ORDER BY id DESC
        LIMIT ?3
        "#,
        since_s,
        pending_i,
        limit
    )
    .fetch_all(pool)
    .await?;

    let mut alerts = Vec::with_capacity(rows.len());
    for r in rows {
        alerts.push(outbox_row_from_parts(
            r.id,
            r.event_id,
            r.watch_id,
            r.channel,
            r.payload,
            r.created_at,
            r.delivered_telegram_at,
            r.acked_mcp_at,
        )?);
    }
    Ok(AlertList { alerts })
}

/// Mark outbox rows as seen by the agent (`trading_alerts_ack`).
pub async fn ack_alerts(
    pool: &sqlx::SqlitePool,
    event_ids: &[String],
) -> anyhow::Result<u64> {
    if event_ids.is_empty() {
        return Ok(0);
    }
    let now = Utc::now().to_rfc3339();
    let mut affected = 0_u64;
    for event_id in event_ids {
        let res = sqlx::query!(
            r#"
            UPDATE alert_outbox
            SET acked_mcp_at = ?
            WHERE event_id = ?
              AND acked_mcp_at IS NULL
            "#,
            now,
            event_id
        )
        .execute(pool)
        .await?;
        affected += res.rows_affected();
    }
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_watch(rule: WatchRule, symbol: Option<&str>) -> Watch {
        Watch {
            id: 1,
            symbol: symbol.map(str::to_string),
            rule,
            threshold: -5.0,
            compare: WatchCompare::Lte,
            channel: WatchChannel::Mcp,
            cooldown_secs: 3600,
            enabled: true,
            last_fired_at: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn day_change_fires_on_lte() {
        let watch = sample_watch(WatchRule::DayChangePct, Some("TSLA"));
        let mut snap = EvalSnapshot {
            now: Utc::now(),
            ..Default::default()
        };
        snap.symbols.insert(
            "TSLA".into(),
            SymbolObservation {
                change_pct: Some(-6.2),
                mark: Some(352.0),
                ..Default::default()
            },
        );
        let fire = evaluate_watch(&watch, &snap).expect("should fire");
        assert!((fire.observed - -6.2).abs() < 1e-9);
        assert_eq!(fire.mark, Some(352.0));
    }

    #[test]
    fn day_change_skips_when_above_threshold() {
        let watch = sample_watch(WatchRule::DayChangePct, Some("TSLA"));
        let mut snap = EvalSnapshot {
            now: Utc::now(),
            ..Default::default()
        };
        snap.symbols.insert(
            "TSLA".into(),
            SymbolObservation {
                change_pct: Some(-1.0),
                mark: Some(352.0),
                ..Default::default()
            },
        );
        assert!(evaluate_watch(&watch, &snap).is_none());
    }

    #[test]
    fn abs_gte_compare() {
        let mut watch = sample_watch(WatchRule::DayChangePct, Some("TSLA"));
        watch.compare = WatchCompare::AbsGte;
        watch.threshold = 5.0;
        let mut snap = EvalSnapshot {
            now: Utc::now(),
            ..Default::default()
        };
        snap.symbols.insert(
            "TSLA".into(),
            SymbolObservation {
                change_pct: Some(5.5),
                ..Default::default()
            },
        );
        assert!(evaluate_watch(&watch, &snap).is_some());
    }

    #[test]
    fn cash_below_portfolio_rule() {
        let mut watch = sample_watch(WatchRule::CashBelow, None);
        watch.threshold = 100.0;
        watch.compare = WatchCompare::Lte;
        let snap = EvalSnapshot {
            cash: Some(50.0),
            now: Utc::now(),
            ..Default::default()
        };
        assert!(evaluate_watch(&watch, &snap).is_some());
    }

    #[test]
    fn cooldown_blocks_until_elapsed() {
        let mut watch = sample_watch(WatchRule::DayChangePct, Some("TSLA"));
        watch.last_fired_at = Some(Utc::now());
        watch.cooldown_secs = 3600;
        assert!(!cooldown_elapsed(&watch, Utc::now()));
        assert!(cooldown_elapsed(
            &watch,
            Utc::now() + chrono::Duration::seconds(3601)
        ));
    }

    #[test]
    fn validate_rejects_symbol_on_portfolio_rule() {
        let err = validate_upsert(&UpsertWatch {
            id: None,
            symbol: Some("TSLA".into()),
            rule: WatchRule::CashBelow,
            threshold: 10.0,
            compare: WatchCompare::Lte,
            channel: WatchChannel::Mcp,
            cooldown_secs: None,
            enabled: None,
        })
        .unwrap_err();
        assert!(err.to_string().contains("portfolio-level"));
    }

    #[test]
    fn validate_requires_symbol_for_day_change() {
        let err = validate_upsert(&UpsertWatch {
            id: None,
            symbol: None,
            rule: WatchRule::DayChangePct,
            threshold: -5.0,
            compare: WatchCompare::Lte,
            channel: WatchChannel::Both,
            cooldown_secs: None,
            enabled: None,
        })
        .unwrap_err();
        assert!(err.to_string().contains("requires a non-empty symbol"));
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
    async fn crud_round_trip_and_disabled_never_fires() {
        let pool = setup_pool().await;

        let created = upsert_watch(
            &pool,
            UpsertWatch {
                id: None,
                symbol: Some("TSLA".into()),
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
        assert!(created.id > 0);
        assert_eq!(created.symbol.as_deref(), Some("TSLA"));

        let listed = list_watches(&pool).await.unwrap();
        assert_eq!(listed.watches.len(), 1);

        let disabled =
            set_watch_enabled(&pool, created.id, false).await.unwrap();
        assert!(!disabled.enabled);

        let mut snap = EvalSnapshot {
            now: Utc::now(),
            nav: Some(5000.0),
            ..Default::default()
        };
        snap.symbols.insert(
            "TSLA".into(),
            SymbolObservation {
                change_pct: Some(-9.0),
                mark: Some(300.0),
                ..Default::default()
            },
        );

        let fired = try_fire_watch(&pool, &disabled, &snap).await.unwrap();
        assert!(fired.is_none(), "disabled watches must never fire");

        let enabled = set_watch_enabled(&pool, created.id, true).await.unwrap();
        let fired = try_fire_watch(&pool, &enabled, &snap)
            .await
            .unwrap()
            .expect("enabled watch should fire");
        assert_eq!(fired.event.event_type, "watch.fired");
        assert_eq!(fired.event.v, 1);

        // Cooldown: reload from DB so last_fired_at is visible.
        let after_fire = list_watches(&pool)
            .await
            .unwrap()
            .watches
            .into_iter()
            .next()
            .unwrap();
        assert!(after_fire.last_fired_at.is_some());
        let again = try_fire_watch(&pool, &after_fire, &snap).await.unwrap();
        assert!(again.is_none());

        let pending = list_alerts(&pool, None, Some(10), true).await.unwrap();
        assert_eq!(pending.alerts.len(), 1);
        let acked = ack_alerts(&pool, std::slice::from_ref(&fired.event_id))
            .await
            .unwrap();
        assert_eq!(acked, 1);
        let pending = list_alerts(&pool, None, Some(10), true).await.unwrap();
        assert!(pending.alerts.is_empty());

        assert!(delete_watch(&pool, created.id).await.unwrap());
        assert!(list_watches(&pool).await.unwrap().watches.is_empty());
    }
}
