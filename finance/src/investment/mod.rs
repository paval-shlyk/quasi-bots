mod config;
pub mod debug;
pub mod model;
pub mod quotes;
pub mod rest_api;
mod routes;
pub mod ws_api;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

pub use config::DzengiConfig;
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
