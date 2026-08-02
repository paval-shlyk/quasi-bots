use crate::RssSource;

#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    utoipa::ToSchema,
    schemars::JsonSchema,
)]
pub struct BrokenLink {
    pub url: String,
    pub last_attempted: chrono::DateTime<chrono::Utc>,
    pub next_attempt: chrono::DateTime<chrono::Utc>,
    pub attempt_count: u32,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema, schemars::JsonSchema)]
pub struct BrokenLinks {
    pub links: Vec<BrokenLink>,
}

impl From<Vec<BrokenLink>> for BrokenLinks {
    fn from(links: Vec<BrokenLink>) -> Self {
        Self { links }
    }
}

pub async fn fetch_broken_links(
    state: &crate::NewsState,
) -> anyhow::Result<Vec<BrokenLink>> {
    Ok(state.broken_links.read().await.clone())
}

/// Fetch active Feed sources that are not broken in broken links list
pub async fn fetch_active_sources(
    state: &crate::NewsState,
) -> anyhow::Result<Vec<RssSource>> {
    let now = chrono::Utc::now();
    let mut sources = state.config.rss_sources.clone();

    for source in sources.iter_mut() {
        let broken_links = state.broken_links.read().await;
        source.urls.retain(|url| {
            let is_broken = broken_links.iter().any(|l| {
                l.url == url.as_str()
                    && l.next_attempt > now
                    && l.attempt_count >= state.config.retry_attempts
            });

            !is_broken
        });
    }

    sources.retain(|s| !s.urls.is_empty());

    Ok(sources)
}

pub async fn restore_broken(
    state: &crate::NewsState,
    url: &str,
) -> anyhow::Result<()> {
    state.broken_links.write().await.retain(|l| l.url != url);

    Ok(())
}

pub async fn set_broken(
    state: &crate::NewsState,
    url: &str,
) -> anyhow::Result<()> {
    let mut broken_links = state.broken_links.write().await;
    let next_attempt = chrono::Utc::now() + state.config.broken_link_cooldown;

    if let Some(link) = broken_links.iter_mut().find(|l| l.url == url) {
        link.last_attempted = chrono::Utc::now();
        link.next_attempt = next_attempt;
        if link.attempt_count < state.config.retry_attempts {
            link.attempt_count += 1;
        }
    } else {
        broken_links.push(BrokenLink {
            url: url.to_string(),
            last_attempted: chrono::Utc::now(),
            next_attempt,
            attempt_count: 1,
        });
    }

    Ok(())
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema, schemars::JsonSchema)]
pub struct SourceStatistics {
    pub id: i64,
    pub url: String,
    pub article_count: i64,
    pub topics: Vec<String>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema, schemars::JsonSchema)]
pub struct SourceStatisticsList {
    pub sources: Vec<SourceStatistics>,
}

impl From<Vec<SourceStatistics>> for SourceStatisticsList {
    fn from(sources: Vec<SourceStatistics>) -> Self {
        Self { sources }
    }
}

pub async fn select_source_with_statistics(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<Vec<SourceStatistics>> {
    telemetry::execution_time!("Select url sources");

    let sources = sqlx::query!(
        r#"
        SELECT 
            s.id,
            s.url as url,
            COUNT(a.id) as article_count,
            json_group_array(DISTINCT t.name) as "topics!: sqlx::types::Json<Vec<String>>"
        FROM news_source as s
        JOIN article as a ON a.source_id = s.id
        JOIN news_topic as t ON a.topic_id = t.id
        GROUP BY s.id
        ORDER BY article_count DESC
        "#
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| SourceStatistics {
        id: r.id,
        article_count: r.article_count,
        url: r.url,
        topics: r.topics.0,
    }).collect();

    Ok(sources)
}
