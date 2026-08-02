/// Custom serde helpers for prost_types::Timestamp, which does not implement
/// Serialize/Deserialize natively. Converts to/from RFC 3339 strings.
pub mod serde_timestamp {
    use prost_types::Timestamp;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(
        ts: &Option<Timestamp>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match ts {
            Some(t) => {
                let secs = t.seconds;
                let nanos = t.nanos as u32;
                let dt = chrono::DateTime::from_timestamp(secs, nanos)
                    .unwrap_or_default();
                serializer.serialize_str(&dt.to_rfc3339())
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Option<Timestamp>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => {
                let dt = chrono::DateTime::parse_from_rfc3339(&s)
                    .map_err(serde::de::Error::custom)?;
                Ok(Some(Timestamp {
                    seconds: dt.timestamp(),
                    nanos: dt.timestamp_subsec_nanos() as i32,
                }))
            }
            None => Ok(None),
        }
    }
}

pub mod proto {
    tonic::include_proto!("crypto");
}

pub use proto::master_service_client::MasterServiceClient;
pub use proto::master_service_server::{MasterService, MasterServiceServer};
pub use proto::worker_service_client::WorkerServiceClient;
pub use proto::worker_service_server::{WorkerService, WorkerServiceServer};

#[cfg(test)]
mod tests {
    use super::proto;
    use prost_types::Timestamp;

    #[test]
    fn worker_status_serde_roundtrip_with_timestamps() {
        let status = proto::WorkerStatus {
            worker_id: "w-1".into(),
            llm_provider: "grok".into(),
            trading_active: true,
            polymarket_active: false,
            started_at: Some(Timestamp {
                seconds: 1700000000,
                nanos: 123_000_000,
            }),
            last_heartbeat: Some(Timestamp {
                seconds: 1700001000,
                nanos: 0,
            }),
            open_trade_count: 3,
            open_prediction_count: 1,
        };

        let json = serde_json::to_string(&status).expect("serialize");
        assert!(json.contains("w-1"));
        assert!(json.contains("2023-11-14")); // rough date check

        let restored: proto::WorkerStatus =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.worker_id, "w-1");
        assert_eq!(restored.started_at.as_ref().unwrap().seconds, 1700000000);
        assert_eq!(restored.started_at.as_ref().unwrap().nanos, 123_000_000);
        assert_eq!(
            restored.last_heartbeat.as_ref().unwrap().seconds,
            1700001000
        );
    }

    #[test]
    fn worker_status_serde_with_null_timestamps() {
        let status = proto::WorkerStatus {
            worker_id: "w-2".into(),
            llm_provider: "gemini".into(),
            trading_active: false,
            polymarket_active: true,
            started_at: None,
            last_heartbeat: None,
            open_trade_count: 0,
            open_prediction_count: 0,
        };

        let json = serde_json::to_string(&status).expect("serialize");
        let restored: proto::WorkerStatus =
            serde_json::from_str(&json).expect("deserialize");
        assert!(restored.started_at.is_none());
        assert!(restored.last_heartbeat.is_none());
    }

    #[test]
    fn portfolio_serde_roundtrip() {
        let portfolio = proto::Portfolio {
            total_balance: "10000.50".into(),
            available_balance: "8000".into(),
            trading_allocated: "1500.25".into(),
            polymarket_allocated: "500.25".into(),
            unrealized_pnl: "50".into(),
            realized_pnl: "-20".into(),
            base_currency: "USDC".into(),
        };

        let json = serde_json::to_string(&portfolio).expect("serialize");
        let restored: proto::Portfolio =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.total_balance, "10000.50");
        assert_eq!(restored.realized_pnl, "-20");
    }

    #[test]
    fn bot_settings_serde_roundtrip() {
        let settings = proto::BotSettings {
            max_position_size_pct: "10".into(),
            stop_loss_pct: "5".into(),
            max_open_positions: 5,
            confidence_threshold: "0.7".into(),
            trading_allocation_pct: "50".into(),
            polymarket_allocation_pct: "50".into(),
            llm_provider: "grok".into(),
            llm_temperature: "0.3".into(),
            allowed_pairs: vec!["BTC/USDC".into(), "ETH/USDC".into()],
            trading_interval_secs: 60,
            max_prediction_exposure: "100".into(),
            min_liquidity_threshold: "50".into(),
            polymarket_interval_secs: 120,
        };

        let json = serde_json::to_string(&settings).expect("serialize");
        let restored: proto::BotSettings =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.allowed_pairs.len(), 2);
        assert_eq!(restored.max_open_positions, 5);
        assert_eq!(restored.trading_interval_secs, 60);
    }

    #[test]
    fn performance_report_serde_with_timestamps() {
        let report = proto::PerformanceReport {
            worker_id: "w-1".into(),
            llm_provider: "grok".into(),
            total_pnl: "250".into(),
            realized_pnl: "200".into(),
            unrealized_pnl: "50".into(),
            win_rate: "0.6".into(),
            total_trades: 10,
            winning_trades: 6,
            losing_trades: 4,
            sharpe_ratio: "1.5".into(),
            max_drawdown: "100".into(),
            total_predictions: 5,
            correct_predictions: 3,
            prediction_accuracy: "0.6".into(),
            period_start: Some(Timestamp {
                seconds: 1700000000,
                nanos: 0,
            }),
            period_end: Some(Timestamp {
                seconds: 1700100000,
                nanos: 0,
            }),
        };

        let json = serde_json::to_string(&report).expect("serialize");
        let restored: proto::PerformanceReport =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.total_trades, 10);
        assert!(restored.period_start.is_some());
        assert!(restored.period_end.is_some());
    }

    #[test]
    fn open_position_serde_roundtrip() {
        let pos = proto::OpenPosition {
            id: "pos-1".into(),
            pair: "BTC/USDC".into(),
            side: "long".into(),
            entry_price: "50000".into(),
            quantity: "0.1".into(),
            current_price: "51000".into(),
            unrealized_pnl: "100".into(),
            stop_loss_price: "49000".into(),
            take_profit_price: "55000".into(),
            allocated_capital: "5000".into(),
            opened_at: Some(Timestamp {
                seconds: 1700000000,
                nanos: 0,
            }),
            updated_at: Some(Timestamp {
                seconds: 1700001000,
                nanos: 0,
            }),
        };

        let json = serde_json::to_string(&pos).expect("serialize");
        let restored: proto::OpenPosition =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.pair, "BTC/USDC");
        assert_eq!(restored.entry_price, "50000");
        assert!(restored.opened_at.is_some());
    }

    #[test]
    fn trade_record_serde_missing_timestamps_default_to_none() {
        // JSON without timestamp fields should deserialize with None
        let json = r#"{
            "id": "t-1",
            "pair": "ETH/USDC",
            "side": "buy",
            "order_type": "limit",
            "quantity": "1.0",
            "price": "3000",
            "filled_quantity": "1.0",
            "avg_fill_price": "3000",
            "fee": "3",
            "status": "filled",
            "llm_rationale": "test",
            "llm_confidence": "0.8"
        }"#;

        let record: proto::TradeRecord =
            serde_json::from_str(json).expect("deserialize");
        assert_eq!(record.id, "t-1");
        assert!(record.created_at.is_none());
        assert!(record.updated_at.is_none());
    }
}
