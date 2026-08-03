use finance::investment::{
    AuthRequest, PortfolioEvent, PortfolioSnapshot, Position, now_ms, sign,
};
use std::env;

// Load env vars from .env if present (use dotenvy so tests can run locally with a .env file)
fn load_dotenv() {
    // ignore errors — if .env is not present we still rely on system env vars
    let _ = dotenvy::from_filename(".env");
}

// This integration test exercises signing and basic DTO serialization used by the
// websocket client. It requires API_KEY and API_SECRET env vars to be present
// because the real client would use them; here we only use them to create a realistic
// signature and ensure no panics.

#[tokio::test]
async fn sign_and_serialization() {
    load_dotenv();

    let api_key = env::var("API_KEY").unwrap_or_else(|_| {
        tracing::warn!("API_KEY not set, using dummy value for test");
        "dummy_api_key".to_string()
    });
    let api_secret = env::var("API_SECRET").unwrap_or_else(|_| {
        tracing::warn!("API_SECRET not set, using dummy value for test");
        "dummy_api_secret".to_string()
    });

    // ensure now_ms returns a reasonable timestamp
    let ts = now_ms();
    assert!(ts > 0);

    let params = format!("timestamp={}&apiKey={}", ts, api_key);
    let sig = sign(&api_secret, &params);
    // signature should be hex and non-empty
    assert!(!sig.is_empty());
    assert!(hex::decode(&sig).is_ok());

    // Build auth request DTO and test serde
    let req = AuthRequest {
        op: "auth".to_string(),
        api_key: api_key.clone(),
        signature: sig.clone(),
        timestamp: ts,
    };

    let txt = serde_json::to_string(&req).expect("serialize auth request");
    let decoded: AuthRequest =
        serde_json::from_str(&txt).expect("deserialize auth request");
    assert_eq!(decoded.api_key, api_key);
    assert_eq!(decoded.signature, sig);
    assert_eq!(decoded.timestamp, ts);

    // Test snapshot and position DTOs
    let pos = Position {
        symbol: "BTCUSD".to_string(),
        quantity: 1.23,
        avg_price: 42000.0,
    };
    let snap = PortfolioSnapshot {
        positions: vec![pos.clone()],
        updated_at: now_ms(),
    };

    let ps_txt = serde_json::to_string(&snap).expect("serialize snapshot");
    let ps_dec: PortfolioSnapshot =
        serde_json::from_str(&ps_txt).expect("deserialize snapshot");
    assert_eq!(ps_dec.positions.len(), 1);
    assert_eq!(ps_dec.positions[0].symbol, pos.symbol);

    // Test enum variants roundtrip via serde_json Value path
    let evt = PortfolioEvent::PositionUpdate(pos);
    let v = serde_json::to_value(&evt).expect("to value");
    // deserializing into raw Value should succeed
    let _ = serde_json::from_value::<serde_json::Value>(v).expect("from value");

    // Attempt to exercise REST endpoints if API_URL provided
    if let Ok(base) = env::var("API_URL") {
        let rc = finance::investment::RestClient::new(
            base,
            api_key.clone(),
            api_secret.clone(),
        );

        // fetch server time and use it for signed endpoints
        let server_ts = rc.time().await.expect("fetch server time");

        // depth for symbol
        let _depth = rc.depth("BTC/USD").await.expect("depth");

        // exchange info
        let _info = rc.exchange_info(server_ts).await.expect("exchange info");

        // account (signed)
        let _acct = rc.account(server_ts).await.expect("account");

        // deposits
        let _deposits = rc.deposits(server_ts).await.expect("deposits");

        // myTrades
        let _trades =
            rc.my_trades("BTC/USD", server_ts).await.expect("my trades");

        // currencies
        // let _currencies = rc.currencies().await.expect("currencies");

        // klines
        let _klines = rc.klines("BTC/USD", "1m").await.expect("klines");

        // open orders
        let _open_orders =
            rc.open_orders(None, server_ts).await.expect("open orders");

        // ledger
        let _ledger = rc
            .ledger(None, None, None, None, None, server_ts)
            .await
            .expect("ledger");

        // transactions
        let _transactions = rc
            .fetch_all_transactions(server_ts)
            .await
            .expect("transactions");

        // trading positions
        let _positions = rc
            .trading_positions(server_ts)
            .await
            .expect("trading positions");

        // trading position history
        // let _history = rc.trading_position_history(None, server_ts).await.expect("trading position history");
    }
}
