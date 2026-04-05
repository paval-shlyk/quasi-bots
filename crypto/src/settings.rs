use chrono::Utc;
use rust_decimal::Decimal;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use serde::{Deserialize, Serialize};

use crate::entities::bot_settings;
use crate::error::{CryptoError, Result};
use crate::llm::LlmProvider;


/// Stored as a single JSONB blob in the `bot_settings` table so new fields
/// can be added without schema migrations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotSettings {
    pub max_position_size_pct: Decimal,
    pub stop_loss_pct: Decimal,
    pub max_open_positions: i32,
    /// Minimum LLM confidence to act (0.0 - 1.0).
    pub confidence_threshold: Decimal,

    pub trading_allocation_pct: Decimal,
    pub polymarket_allocation_pct: Decimal,

    pub llm_provider: LlmProvider,
    pub llm_temperature: Decimal,

    pub allowed_pairs: Vec<String>,
    pub trading_interval_secs: u64,

    pub max_prediction_exposure: Decimal,
    pub min_liquidity_threshold: Decimal,
    pub polymarket_interval_secs: u64,
}

impl Default for BotSettings {
    fn default() -> Self {
        Self {
            max_position_size_pct: Decimal::new(10, 0),
            stop_loss_pct: Decimal::new(5, 0),
            max_open_positions: 5,
            confidence_threshold: Decimal::new(7, 1), // 0.7
            trading_allocation_pct: Decimal::new(50, 0),
            polymarket_allocation_pct: Decimal::new(50, 0),
            llm_provider: LlmProvider::Grok,
            llm_temperature: Decimal::new(3, 1), // 0.3
            allowed_pairs: vec!["BTC/USDC".into(), "ETH/USDC".into()],
            trading_interval_secs: 60,
            max_prediction_exposure: Decimal::new(100, 0),
            min_liquidity_threshold: Decimal::new(50, 0),
            polymarket_interval_secs: 120,
        }
    }
}


/// The in-memory copy is the hot path; changes are validated, applied, then
/// persisted to the single-row `bot_settings` table.
pub struct SettingsService {
    db: DatabaseConnection,
    settings: tokio::sync::RwLock<BotSettings>,
}

impl SettingsService {
    pub fn new(db: DatabaseConnection, initial: BotSettings) -> Self {
        Self {
            db,
            settings: tokio::sync::RwLock::new(initial),
        }
    }

    /// Load settings from DB or persist defaults if none exist yet.
    pub async fn load_or_create(&self) -> Result<BotSettings> {
        let existing = bot_settings::Entity::find().one(&self.db).await?;

        match existing {
            Some(model) => {
                let settings: BotSettings =
                    serde_json::from_value(model.settings_json).map_err(|e| {
                        CryptoError::Settings(format!("Failed to parse settings: {e}"))
                    })?;
                *self.settings.write().await = settings.clone();
                Ok(settings)
            }
            None => {
                let settings = BotSettings::default();
                self.persist(&settings).await?;
                *self.settings.write().await = settings.clone();
                Ok(settings)
            }
        }
    }

    /// Read current settings (from memory - microsecond read lock).
    pub async fn get(&self) -> BotSettings {
        self.settings.read().await.clone()
    }

    /// Atomically update settings via a closure, validate, and persist.
    pub async fn update<F>(&self, updater: F) -> Result<BotSettings>
    where
        F: FnOnce(&mut BotSettings),
    {
        let mut guard = self.settings.write().await;
        updater(&mut guard);
        Self::validate(&guard)?;
        let snapshot = guard.clone();
        drop(guard);

        self.persist(&snapshot).await?;
        Ok(snapshot)
    }


    fn validate(s: &BotSettings) -> Result<()> {
        if s.trading_allocation_pct + s.polymarket_allocation_pct > Decimal::new(100, 0) {
            return Err(CryptoError::Settings(
                "Trading + Polymarket allocation exceeds 100%".into(),
            ));
        }
        if s.max_position_size_pct <= Decimal::ZERO
            || s.max_position_size_pct > Decimal::new(100, 0)
        {
            return Err(CryptoError::Settings(
                "max_position_size_pct must be in (0, 100]".into(),
            ));
        }
        if s.confidence_threshold < Decimal::ZERO || s.confidence_threshold > Decimal::ONE {
            return Err(CryptoError::Settings(
                "confidence_threshold must be in [0, 1]".into(),
            ));
        }
        Ok(())
    }

    async fn persist(&self, settings: &BotSettings) -> Result<()> {
        let json = serde_json::to_value(settings)
            .map_err(|e| CryptoError::Settings(format!("Serialization failed: {e}")))?;

        // Delete-then-insert for the single-row table.
        let _ = bot_settings::Entity::delete_many()
            .exec(&self.db)
            .await?;

        let model = bot_settings::ActiveModel {
            id: Set(1),
            settings_json: Set(json),
            updated_at: Set(Utc::now()),
        };
        model.insert(&self.db).await?;
        Ok(())
    }
}
