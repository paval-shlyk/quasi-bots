use rust_decimal::Decimal;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("Portfolio error: {0}")]
    Portfolio(String),

    #[error("Trading error: {0}")]
    Trading(String),

    #[error("Polymarket error: {0}")]
    Polymarket(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Settings error: {0}")]
    Settings(String),

    #[error("Insufficient balance: available={available}, required={required}")]
    InsufficientBalance {
        available: Decimal,
        required: Decimal,
    },

    #[error("Risk limit exceeded: {0}")]
    RiskLimitExceeded(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, CryptoError>;
