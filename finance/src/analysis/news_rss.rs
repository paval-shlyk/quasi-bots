//! Dynamic RSS news (no DB, no cache).

use feed_rs::parser;

use super::providers::{AssetNewsItem, NewsProvider};

/// Fetches a Google News RSS query for the symbol (and optional name).
#[derive(Debug, Clone)]
pub struct RssNewsProvider {
    client: reqwest::Client,
    limit: usize,
}

impl Default for RssNewsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl RssNewsProvider {
    pub fn new() -> Self {
        Self::with_limit(5)
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            client: reqwest::Client::new(),
            limit,
        }
    }

    fn feed_url(symbol: &str, name: Option<&str>) -> String {
        let q = match name {
            Some(n) if !n.is_empty() => format!("{symbol} OR \"{n}\" stock"),
            _ => format!("{symbol} stock"),
        };
        let encoded = urlencoding::encode(&q);
        format!(
            "https://news.google.com/rss/search?q={encoded}&hl=en-US&gl=US&ceid=US:en"
        )
    }
}

impl NewsProvider for RssNewsProvider {
    async fn recent(
        &self,
        symbol: &str,
        name: Option<&str>,
    ) -> anyhow::Result<Vec<AssetNewsItem>> {
        if self.limit == 0 {
            return Ok(vec![]);
        }

        let url = Self::feed_url(symbol, name);
        let bytes = self
            .client
            .get(&url)
            .header("User-Agent", "quasi-bots-finance/0.1")
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let feed = parser::parse(bytes.as_ref())
            .map_err(|e| anyhow::anyhow!("rss parse: {e}"))?;

        let mut items = Vec::new();
        for entry in feed.entries.into_iter().take(self.limit) {
            let title = entry.title.map(|t| t.content).unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let url = entry.links.first().map(|l| l.href.clone());
            let summary = entry.summary.map(|s| s.content);
            let published_at = entry.published.or(entry.updated);

            items.push(AssetNewsItem {
                title,
                published_at,
                url,
                summary,
                source: Some("google_news_rss".into()),
            });
        }

        Ok(items)
    }
}
