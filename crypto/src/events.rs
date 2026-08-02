use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Internal events broadcast between modules via Tokio channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BotEvent {
    MarketDataUpdate {
        pair: String,
        price: Decimal,
        timestamp: DateTime<Utc>,
    },
    TradeExecuted {
        trade_id: Uuid,
        pair: String,
        side: String,
        quantity: Decimal,
        price: Decimal,
    },
    PredictionPlaced {
        prediction_id: Uuid,
        market_id: String,
        side: String,
        shares: Decimal,
    },
    PortfolioUpdated {
        total_balance: Decimal,
        available_balance: Decimal,
    },
    LlmDecisionMade {
        module: String,
        action: String,
        confidence: Decimal,
        rationale: String,
    },
    SettingsChanged {
        key: String,
        old_value: String,
        new_value: String,
    },
    ModuleError {
        module: String,
        error: String,
    },
}

/// Broadcast-based internal event bus.
/// each module can publish events and any number of subscribers
/// (including the API layer) can react.
pub struct EventBus {
    sender: broadcast::Sender<BotEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event. If no receivers are listening the event is dropped.
    pub fn publish(&self, event: BotEvent) {
        let _ = self.sender.send(event);
    }

    /// Create a new receiver for this bus.
    pub fn subscribe(&self) -> broadcast::Receiver<BotEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}
