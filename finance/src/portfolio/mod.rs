mod config;
pub mod model;
pub mod rest_api;
pub mod ws_api;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

pub use model::*;
pub use rest_api::RestClient;
pub use ws_api::Client;

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
