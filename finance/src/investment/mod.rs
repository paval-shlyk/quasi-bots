pub mod alert_evaluator;
mod config;
pub mod debug;
pub mod model;
pub mod quotes;
pub mod rest_api;
mod routes;
pub mod telegram_commands;
pub mod telegram_outbox;
pub mod watches;
pub mod ws_api;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

pub use alert_evaluator::{
    AlertEvaluatorConfig, DEFAULT_EVAL_INTERVAL_SECS,
    DEFAULT_REST_RECONCILE_SECS, alert_evaluator_enabled, run_alert_evaluator,
};
pub use config::{
    AlertsConfig, DzengiConfig, ResolvedTelegram, TelegramConfig, env_flag_or,
    parse_enabled_flag,
};
pub use debug::{
    finance_debug_enabled, redact_secrets, rest_ticker_sample,
    ws_ticker_frame_redacted, ws_topics,
};
pub use model::*;
pub use quotes::{
    MAX_QUOTE_SYMBOLS, Quote, QuotesResponse, fetch_quotes, resolve_quote_pair,
};
pub use rest_api::RestClient;
pub use routes::*;
pub use telegram_commands::{
    TelegramCommand, TelegramCommandsConfig, format_cash_reply,
    format_help_reply, format_status_reply, format_watches_reply,
    parse_command, run_telegram_commands_worker, telegram_commands_enabled,
};
pub use telegram_outbox::{
    DEFAULT_POLL_INTERVAL_SECS as TELEGRAM_DEFAULT_POLL_SECS,
    TelegramDeliveryConfig, format_telegram_message,
    run_telegram_outbox_consumer, send_telegram_bot_text,
    send_telegram_chat_text, send_telegram_text, telegram_delivery_enabled,
};
pub use watches::{
    AlertEvent, AlertList, AlertOutboxRow, DEFAULT_COOLDOWN_SECS, EvalSnapshot,
    FireObservation, MAX_ACK_EVENT_IDS, MAX_TELEGRAM_DRAIN, SymbolObservation,
    UpsertWatch, Watch, WatchChannel, WatchCompare, WatchList, WatchRule,
    ack_alerts, delete_watch, evaluate_enabled_watches, evaluate_watch,
    insert_alert, list_alerts, list_enabled_watches,
    list_pending_telegram_alerts, list_watches, mark_telegram_delivered,
    set_watch_enabled, try_fire_watch, upsert_watch,
};
pub use ws_api::{Client, ws_api_prefix, ws_connect_url};

type HmacSha256 = Hmac<Sha256>;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn sign(secret: &str, msg: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(msg.as_bytes());
    let result = mac.finalize();
    let bytes = result.into_bytes();
    hex::encode(bytes)
}
