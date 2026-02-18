use crate::portfolio::model::*;
use crate::portfolio::sign;
use reqwest;
use serde::de::DeserializeOwned;

const API_KEY_HEADER: &str = "X-MBX-APIKEY";

/// Lightweight REST client for dzengi-like API used by tests.
pub struct RestClient {
    pub base_url: String,
    pub api_key: String,
    pub api_secret: String,
    client: reqwest::Client,
}

impl RestClient {
    /// Create a new RestClient.
    ///
    /// `base_url` must be the HTTP(S) endpoint (without trailing slash preferred).
    /// `api_key`/`api_secret` are used to sign requests for private endpoints.
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        api_secret: impl Into<String>,
    ) -> Self {
        let api_key = api_key.into();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            API_KEY_HEADER,
            reqwest::header::HeaderValue::from_str(&api_key).unwrap(),
        );

        RestClient {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            api_secret: api_secret.into(),
            client: reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .unwrap(),
        }
    }

    async fn get<T: DeserializeOwned>(&self, url: &str) -> anyhow::Result<T> {
        let resp = self.client.get(url).send().await?.error_for_status()?;
        let txt = resp.text().await?;
        // tracing::debug!("Response from {}: {}", url, txt);
        let v: T = serde_json::from_str(&txt)?;
        Ok(v)
    }

    /// Fetch server time from the `/time` REST endpoint.
    pub async fn time(&self) -> anyhow::Result<u64> {
        let url = format!("{}/time", self.base_url);
        let resp: ServerTime = self.get(&url).await?;
        Ok(resp.server_time)
    }

    /// Fetch list of currencies from `/currencies` REST endpoint.
    pub async fn currencies(&self) -> anyhow::Result<Vec<Currency>> {
        let url = format!("{}/currencies", self.base_url);
        self.get(&url).await
    }

    /// Fetch current order book depth for `symbol` from the `/depth` REST endpoint.
    pub async fn depth(&self, symbol: &str) -> anyhow::Result<OrderBook> {
        let url = format!("{}/depth", self.base_url);
        let url = format!("{}?symbol={}", url, urlencoding::encode(symbol));
        self.get(&url).await
    }

    /// Fetch exchange metadata (symbols, filters, rate limits) from `/exchangeInfo`.
    pub async fn exchange_info(&self) -> anyhow::Result<ExchangeInfo> {
        let url = format!("{}/exchangeInfo", self.base_url);
        self.get(&url).await
    }

    /// Fetch klines (candlestick data).
    pub async fn klines(
        &self,
        symbol: &str,
        interval: &str,
    ) -> anyhow::Result<Vec<Kline>> {
        let url = format!("{}/klines", self.base_url);
        let url = format!(
            "{}?symbol={}&interval={}",
            url,
            urlencoding::encode(symbol),
            urlencoding::encode(interval)
        );
        // Note: Check if API returns array of arrays or array of objects.
        // Assuming array of arrays based on standard Binance-like API which uses custom deserializer in model.rs
        self.get(&url).await
    }

    /// Fetch account information (balances, positions) from `/account`.
    pub async fn account(
        &self,
        server_ts: u64,
    ) -> anyhow::Result<AccountInformation> {
        let url = format!("{}/account", self.base_url);
        let params = format!("timestamp={}", server_ts);
        let sig = sign(&self.api_secret, &params);
        let url = format!("{}?{}&signature={}", url, params, sig);
        self.get(&url).await
    }

    /// Fetch deposit history for the account from `/deposits`.
    pub async fn deposits(
        &self,
        server_ts: u64,
    ) -> anyhow::Result<Vec<Deposit>> {
        let url = format!("{}/deposits", self.base_url);
        let params = format!("timestamp={}", server_ts);
        let sig = sign(&self.api_secret, &params);
        let url = format!("{}?{}&signature={}", url, params, sig);
        self.get(&url).await
    }

    /// Fetch account trades (myTrades) for `symbol` from `/myTrades`.
    pub async fn my_trades(
        &self,
        symbol: &str,
        server_ts: u64,
    ) -> anyhow::Result<Vec<Trade>> {
        let url = format!("{}/myTrades", self.base_url);
        let params = format!(
            "symbol={}&timestamp={}",
            urlencoding::encode(symbol),
            server_ts
        );
        let sig = sign(&self.api_secret, &params);
        let url = format!("{}?{}&signature={}", url, params, sig);
        self.get(&url).await
    }

    /// Fetch a specific order by ID.
    pub async fn fetch_order(
        &self,
        order_id: &str,
        server_ts: u64,
    ) -> anyhow::Result<Order> {
        let url = format!("{}/fetchOrder", self.base_url);
        let params = format!(
            "orderId={}&timestamp={}",
            urlencoding::encode(order_id),
            server_ts
        );
        let sig = sign(&self.api_secret, &params);
        let url = format!("{}?{}&signature={}", url, params, sig);
        self.get(&url).await
    }

    /// Fetch open orders.
    pub async fn open_orders(
        &self,
        symbol: Option<&str>,
        server_ts: u64,
    ) -> anyhow::Result<Vec<Order>> {
        let url = format!("{}/openOrders", self.base_url);
        let mut params = format!("timestamp={}", server_ts);
        if let Some(s) = symbol {
            params = format!("symbol={}&{}", urlencoding::encode(s), params);
        }
        let sig = sign(&self.api_secret, &params);
        let url = format!("{}?{}&signature={}", url, params, sig);
        self.get(&url).await
    }

    /// Fetch ledger/transaction history.
    pub async fn ledger(
        &self,
        currency: Option<&str>,
        server_ts: u64,
    ) -> anyhow::Result<Vec<LedgerEntry>> {
        let url = format!("{}/ledger", self.base_url);
        let mut params = format!("timestamp={}", server_ts);
        if let Some(c) = currency {
            params = format!("currency={}&{}", urlencoding::encode(c), params);
        }
        let sig = sign(&self.api_secret, &params);
        let url = format!("{}?{}&signature={}", url, params, sig);
        self.get(&url).await
    }

    /// Fetch transactions (deposit/withdrawals).
    pub async fn transactions(
        &self,
        server_ts: u64,
    ) -> anyhow::Result<Vec<Transaction>> {
        let url = format!("{}/transactions", self.base_url);
        let params = format!("timestamp={}", server_ts);
        let sig = sign(&self.api_secret, &params);
        let url = format!("{}?{}&signature={}", url, params, sig);
        self.get(&url).await
    }

    /// Fetch trading positions.
    pub async fn trading_positions(
        &self,
        server_ts: u64,
    ) -> anyhow::Result<Vec<TradingPosition>> {
        let url = format!("{}/tradingPositions", self.base_url);
        let params = format!("timestamp={}", server_ts);
        let sig = sign(&self.api_secret, &params);
        let url = format!("{}?{}&signature={}", url, params, sig);
        let resp: TradingPositionsResponse = self.get(&url).await?;
        Ok(resp.positions)
    }

    /// Fetch trading position history.
    pub async fn trading_position_history(
        &self,
        symbol: Option<&str>,
        server_ts: u64,
    ) -> anyhow::Result<Vec<TradingPositionHistory>> {
        let url = format!("{}/tradingPositionHistory", self.base_url);
        let mut params = format!("timestamp={}", server_ts);
        if let Some(s) = symbol {
            params = format!("symbol={}&{}", urlencoding::encode(s), params);
        }
        let sig = sign(&self.api_secret, &params);
        let url = format!("{}?{}&signature={}", url, params, sig);
        let resp: TradingPositionHistoryResponse = self.get(&url).await?;
        Ok(resp.history)
    }
}
