use crate::portfolio::model::*;
use crate::portfolio::{now_ms, sign};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use std::time::Duration;

/// Client connects to dzengi.com websocket, authenticates and produces parsed events.
pub struct Client {
    // Receiver for parsed events
    pub rx: mpsc::UnboundedReceiver<PortfolioEvent>,
    // Sender for outgoing raw text messages (commands)
    pub tx: mpsc::UnboundedSender<String>,
    // stored credentials for signed ws requests
    pub api_key: String,
    pub api_secret: String,
}

impl Client {
    /// Connect to a websocket endpoint and authenticate using API key/secret.
    pub async fn connect(
        url: &str,
        api_key: &str,
        api_secret: &str,
    ) -> anyhow::Result<Client> {
        let (ws_stream, _resp) = tokio_tungstenite::connect_async(url).await?;
        let (mut write, mut read) = ws_stream.split();

        // channels for external use
        let (tx_out, mut rx_out) = mpsc::unbounded_channel::<String>();
        let (tx_evt, rx_evt) = mpsc::unbounded_channel::<PortfolioEvent>();

        // spawn task to forward outgoing commands to websocket sink and periodically ping
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(20));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = write.send(Message::Ping(vec![].into())).await {
                            tracing::warn!("WS ping error: {}", e);
                            break;
                        }
                    }
                    opt = rx_out.recv() => {
                        match opt {
                            Some(cmd) => {
                                if let Err(e) = write.send(Message::Text(cmd.into())).await {
                                    tracing::warn!("WS send error: {}", e);
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        // perform authentication
        let ts = now_ms();
        let param_str = format!("timestamp={}&apiKey={}", ts, api_key);
        let sig = sign(api_secret, &param_str);

        let payload = serde_json::json!({
            "apiKey": api_key,
            "timestamp": ts,
            "signature": sig
        });

        let ws_request = serde_json::json!({
            "destination": "/api/v1/auth",
            "payload": payload,
            "correlationId": "1"
        });

        let auth_text = serde_json::to_string(&ws_request)?;
        tx_out.send(auth_text)?;

        // spawn reader task
        let tx_evt_clone = tx_evt.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(txt)) => {
                        match serde_json::from_str::<serde_json::Value>(&txt) {
                            Ok(json) => {
                                // try to decode known shapes
                                if let Some(obj) = json.as_object() {
                                    if obj.get("op").and_then(|v| v.as_str())
                                        == Some("auth")
                                    {
                                        let resp: Result<AuthResponse, _> =
                                            serde_json::from_value(json.clone());
                                        if let Ok(r) = resp {
                                            let _ = tx_evt_clone
                                                .send(PortfolioEvent::Auth(r));
                                            continue;
                                        }
                                    }

                                    if obj.get("type").and_then(|v| v.as_str())
                                        == Some("portfolio_snapshot")
                                    {
                                        if let Ok(snap) = serde_json::from_value::<
                                            PortfolioSnapshot,
                                        >(json.clone()) {
                                            let _ = tx_evt_clone.send(
                                                PortfolioEvent::Snapshot(snap),
                                            );
                                            continue;
                                        }
                                    }

                                    if obj.get("type").and_then(|v| v.as_str())
                                        == Some("position_update")
                                    {
                                        if let Ok(pos) =
                                            serde_json::from_value::<Position>(
                                                json.clone(),
                                            )
                                        {
                                            let _ = tx_evt_clone.send(
                                                PortfolioEvent::PositionUpdate(
                                                    pos,
                                                ),
                                            );
                                            continue;
                                        }
                                    }
                                }

                                // fallback: raw JSON
                                let _ = tx_evt_clone
                                    .send(PortfolioEvent::Raw(json));
                            }
                            Err(e) => tracing::warn!(
                                "Failed to parse incoming ws json: {}",
                                e
                            ),
                        }
                    }
                    Ok(Message::Binary(_)) => {}
                    Ok(Message::Close(_)) => break,
                    Err(e) => {
                        tracing::warn!("Websocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok(Client {
            rx: rx_evt,
            tx: tx_out,
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
        })
    }

    /// Subscribe to portfolio updates.
    pub fn subscribe_portfolio(&self) -> anyhow::Result<()> {
        let cmd =
            serde_json::json!({"op": "subscribe", "channel": "portfolio"})
                .to_string();
        self.tx.send(cmd)?;
        Ok(())
    }

    /// Send a request over the existing websocket and wait for a matching response.
    pub async fn request(
        &mut self,
        destination: &str,
        payload: serde_json::Value,
        correlation_id: &str,
        timeout_secs: u64,
    ) -> anyhow::Result<serde_json::Value> {
        let ws_request = serde_json::json!({
            "destination": destination,
            "payload": payload,
            "correlationId": correlation_id,
        });

        let txt = serde_json::to_string(&ws_request)?;
        self.tx.send(txt)?;

        let timeout = Duration::from_secs(timeout_secs);
        loop {
            let recv = tokio::time::timeout(timeout, self.rx.recv()).await;
            match recv {
                Ok(Some(evt)) => match evt {
                    PortfolioEvent::Raw(val) => {
                        if val.get("correlationId").and_then(|v| v.as_str())
                            == Some(correlation_id)
                        {
                            // Some responses wrap the result in "payload"
                            // If the caller expects the inner payload, we should extract it here or let caller do it.
                            // REST endpoints usually return the data directly.
                            // WS `request` generic helper returns the FULL response JSON.
                            // Wrapper methods below should extract `payload`.
                            return Ok(val);
                        }
                    }
                    _ => {} // ignore other events
                },
                Ok(None) => return Err(anyhow::anyhow!("event receiver closed")),
                Err(_) => return Err(anyhow::anyhow!("timeout waiting for websocket response")),
            }
        }
    }

    /// Helper to request and deserialize payload
    async fn request_payload<T: serde::de::DeserializeOwned>(
        &mut self,
        destination: &str,
        payload: serde_json::Value,
        correlation_id: &str,
        timeout_secs: u64,
    ) -> anyhow::Result<T> {
        let resp = self.request(destination, payload, correlation_id, timeout_secs).await?;
        // Check if response has "payload" field, if so try to deserialize that.
        // If not, try to deserialize the whole response.
        if let Some(p) = resp.get("payload") {
             if let Ok(v) = serde_json::from_value(p.clone()) {
                 return Ok(v);
             }
        }
        let v = serde_json::from_value(resp)?;
        Ok(v)
    }

    /// Request server time.
    pub async fn ws_time(&mut self) -> anyhow::Result<u64> {
        let cid = format!("time-{}", now_ms());
        let payload = serde_json::json!({});
        // special case: time response structure might vary
        let resp = self.request("/api/v1/time", payload, &cid, 5).await?;
        
        if let Some(st) = resp.get("payload").and_then(|p| p.get("serverTime")).and_then(|v| v.as_u64()) {
            return Ok(st);
        }
        if let Some(st) = resp.get("serverTime").and_then(|v| v.as_u64()) {
            return Ok(st);
        }
         Err(anyhow::anyhow!("serverTime not found in time response"))
    }

    /// Fetch order book depth.
    pub async fn ws_depth(&mut self, symbol: &str) -> anyhow::Result<OrderBook> {
        let cid = format!("depth-{}-{}", symbol, now_ms());
        let payload = serde_json::json!({"symbol": symbol});
        self.request_payload("/api/v1/depth", payload, &cid, 5).await
    }

    /// Fetch exchange metadata.
    pub async fn ws_exchange_info(&mut self) -> anyhow::Result<ExchangeInfo> {
        let cid = format!("exinfo-{}", now_ms());
        let payload = serde_json::json!({});
        self.request_payload("/api/v1/exchangeInfo", payload, &cid, 5).await
    }

    /// Fetch account information.
    pub async fn ws_account(&mut self, server_ts: u64) -> anyhow::Result<AccountInformation> {
        let cid = format!("account-{}", server_ts);
        let param_str = format!("timestamp={}&apiKey={}", server_ts, self.api_key);
        let sig = sign(&self.api_secret, &param_str);
        let payload = serde_json::json!({"apiKey": self.api_key, "timestamp": server_ts, "signature": sig});
        self.request_payload("/api/v1/account", payload, &cid, 5).await
    }

    /// Fetch deposit history.
    pub async fn ws_deposits(&mut self, server_ts: u64) -> anyhow::Result<Vec<Deposit>> {
        let cid = format!("deposits-{}", server_ts);
        let param_str = format!("timestamp={}&apiKey={}", server_ts, self.api_key);
        let sig = sign(&self.api_secret, &param_str);
        let payload = serde_json::json!({"apiKey": self.api_key, "timestamp": server_ts, "signature": sig});
        self.request_payload("/api/v1/deposits", payload, &cid, 5).await
    }

    /// Fetch account trades.
    pub async fn ws_my_trades(&mut self, symbol: &str, server_ts: u64) -> anyhow::Result<Vec<Trade>> {
        let cid = format!("mytrades-{}-{}", symbol, server_ts);
        let param_str = format!("timestamp={}&apiKey={}", server_ts, self.api_key);
        let sig = sign(&self.api_secret, &param_str);
        let payload = serde_json::json!({"symbol": symbol, "apiKey": self.api_key, "timestamp": server_ts, "signature": sig});
        self.request_payload("/api/v1/myTrades", payload, &cid, 5).await
    }
}
