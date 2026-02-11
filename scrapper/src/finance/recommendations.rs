use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct RecommendationResponse {
    pub strategy: String, // "Momentum & Volume Breakout"
    pub assets: Vec<RecommendedAsset>,
}

#[derive(Serialize)]
pub struct RecommendedAsset {
    pub symbol: String,
    pub name: String,
    pub price: f64,
    pub change_percent: f64,
    pub volume_spike: String, // "2.5x Normal" - понятная метрика для LLM
    pub reason: String,       // "High Volume Rally"
    ///  
    pub score: f64,           
}

#[derive(Deserialize)]
struct YahooScreenerResponse {
    #[serde(rename = "finance")]
    finance: ScreenerFinance,
}

#[derive(Deserialize)]
struct ScreenerFinance {
    result: Vec<ScreenerResult>,
}

#[derive(Deserialize)]
struct ScreenerResult {
    quotes: Vec<YahooQuote>,
}

#[derive(Deserialize)]
struct YahooQuote {
    symbol: String,
    #[serde(rename = "shortName", default)]
    short_name: String,
    #[serde(rename = "regularMarketPrice", default)]
    price: f64,
    #[serde(rename = "regularMarketChangePercent", default)]
    change_percent: f64,
    #[serde(rename = "regularMarketVolume", default)]
    volume: u64,
    #[serde(rename = "averageDailyVolume3Month", default)]
    avg_volume: u64,
    #[serde(rename = "fiftyTwoWeekHigh", default)]
    fifty_two_week_high: f64,
}

#[derive(Serialize)]
pub struct FullMarketReport {
    pub opportunities: Vec<RecommendedAsset>, // Gainers (Momentum)
    pub risks_and_dips: Vec<RecommendedAsset>, // Losers (Dip Hunting)
}

pub struct Sources {

}

// Обновляем функцию получения рекомендаций
pub async fn get_full_market_report() -> anyhow::Result<FullMarketReport> {
    let client = reqwest::Client::new();

    // 1. Параллельно запрашиваем Gainers и Losers
    let gainers_url = "https://query2.finance.yahoo.com/v1/finance/screener/predefined/saved/day_gainers?count=10";
    let losers_url = "https://query2.finance.yahoo.com/v1/finance/screener/predefined/saved/day_losers?count=25"; // Берем больше, чтобы отфильтровать мусор

    let (gainers_resp, losers_resp) = tokio::join!(
        client.get(gainers_url).send(),
        client.get(losers_url).send()
    );

    // Обработка Gainers (код из прошлого ответа)...
    let gainers_data = gainers_resp?.text().await?;
    // json::<serde_json::Value>().await?;
    // let opportunities = process_gainers(gainers_data); // Твоя прошлая логика
    tracing::info!("gain_resp = {gainers_data:?}");

    // 2. Обработка Losers (Новая логика)
    // let losers_data = losers_resp?.json::<serde_json::Value>().await?;
    let losers_data = losers_resp?.text().await?;
    // let risks_and_dips = process_losers(losers_data);

    tracing::info!("losers_resp = {losers_data:?}");
    Ok(FullMarketReport {
        opportunities: vec![],
        risks_and_dips: vec![],
    })
}

fn process_losers(data: YahooScreenerResponse) -> Vec<RecommendedAsset> {
    let empty_vec = Vec::new();
    let raw = data
        .finance
        .result
        .first()
        .map(|r| &r.quotes)
        .unwrap_or(&empty_vec);

    let mut recommended: Vec<RecommendedAsset> = raw
        .iter()
        // Фильтр 1: Только крупные компании (Price > 10, Volume > 1M)
        // Мы хотим отсеять "мусор", который падает заслуженно
        .filter(|q| q.price > 10.0 && q.volume > 1_000_000)
        .map(|q| {
            let drop_percent = q.change_percent; // Оно отрицательное, например -15.0
            let rvol = if q.avg_volume > 0 {
                q.volume as f64 / q.avg_volume as f64
            } else {
                0.0
            };

            // Определяем тип падения для LLM
            let (reason, _severity) = if drop_percent < -10.0 {
                ("CRASH / PANIC SELLING".to_string(), "High Risk")
            } else if rvol > 2.0 {
                ("Heavy Institutional Selling".to_string(), "Medium Risk")
            } else {
                ("Correction".to_string(), "Low Risk")
            };

            // Рассчитываем скидку от годового максимума
            let discount = if q.fifty_two_week_high > 0.0 {
                (q.fifty_two_week_high - q.price) / q.fifty_two_week_high
                    * 100.0
            } else {
                0.0
            };

            RecommendedAsset {
                symbol: q.symbol.clone(),
                name: q.short_name.clone(),
                price: q.price,
                change_percent: q.change_percent,
                volume_spike: format!("{:.1}x Avg Vol", rvol),
                // Формируем "нарратив" для бота
                reason: format!(
                    "{} (Discount from High: -{:.1}%)",
                    reason, discount
                ),
                score: discount + (rvol * 10.0), // Чем больше скидка и объем, тем интереснее
            }
        })
        .collect();

    recommended.sort_by(|a, b| {
        b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal)
    });

    recommended.into_iter().take(5).collect()
}

fn process_gainers(data: YahooScreenerResponse) -> Vec<RecommendedAsset> {
    // 1. Извлекаем сырой список (или пустой, если Yahoo отдал мусор)
    let empty_vec = Vec::new();
    let raw_quotes = data
        .finance
        .result
        .first()
        .map(|r| &r.quotes)
        .unwrap_or(&empty_vec);

    let mut recommended: Vec<RecommendedAsset> = raw_quotes
        .iter()
        // ФИЛЬТР 1: Ликвидность.
        // Отсекаем "Penny Stocks" (< $5) и мертвые акции (Volume < 500k).
        // LLM не должна советовать скам.
        .filter(|q| q.price > 5.0 && q.volume > 500_000)
        // ФИЛЬТР 2: Relative Volume (RVOL).
        // Если объем меньше среднего, рост цены подозрителен.
        // Берем только те, где активность выше нормы на 20% (1.2).
        .filter(|q| {
            q.avg_volume > 0 && (q.volume as f64 / q.avg_volume as f64) > 1.2
        })
        .map(|q| {
            let rvol = q.volume as f64 / q.avg_volume as f64;

            // ФОРМУЛА SCORING:
            // Volume Spike важнее, чем % роста.
            // Умножаем RVOL на 20, чтобы "вес" объема перевешивал просто скачок цены.
            let score = (rvol * 20.0) + q.change_percent.abs();

            // Генерируем "Reason" для LLM, чтобы она понимала контекст
            let reason = if rvol > 3.0 {
                "Extreme Volume Breakout (Institutional Buying?)"
            } else if rvol > 2.0 {
                "High Volume Rally"
            } else {
                "Strong Momentum"
            };

            RecommendedAsset {
                symbol: q.symbol.clone(),
                name: q.short_name.clone(),
                price: q.price,
                change_percent: q.change_percent,
                volume_spike: format!("{:.1}x Avg", rvol),
                reason: reason.to_string(),
                score,
            }
        })
        .collect();

    recommended.sort_by(|a, b| {
        b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal)
    });

    recommended.into_iter().take(5).collect()
}
