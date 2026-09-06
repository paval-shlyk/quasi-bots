use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::types::Json;

use crate::{ArticlesWithTopic, SavedArticle, TodayNews};

#[derive(Debug, Clone)]
pub struct InstrumentHeadline {
    pub title: String,
    pub url: Option<String>,
    pub published_at: DateTime<Utc>,
    pub source: Option<String>,
}

pub fn headline_matches(title: &str, topic: &str, needles: &[String]) -> bool {
    if needles.is_empty() {
        return false;
    }
    let hay = format!("{title} {topic}").to_lowercase();
    needles.iter().any(|n| {
        let n = n.trim();
        !n.is_empty() && hay.contains(&n.to_lowercase())
    })
}

/// Recent bank headlines whose title or topic mentions the instrument.
pub async fn select_recent_for_instrument(
    pool: &sqlx::SqlitePool,
    needles: &[String],
    limit: i64,
) -> anyhow::Result<Vec<InstrumentHeadline>> {
    if needles.is_empty() || limit <= 0 {
        return Ok(vec![]);
    }

    #[derive(sqlx::FromRow)]
    struct Row {
        topic: String,
        title: String,
        links: Json<Vec<String>>,
        published_at: DateTime<Utc>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        r#"
            SELECT
                t.name as topic,
                a.title,
                a.links as links,
                a.published_at as published_at
            FROM article as a
            JOIN news_topic as t ON a.topic_id = t.id
            WHERE a.published_at > datetime('now', '-7 days')
            ORDER BY a.published_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::new();
    for row in rows {
        if !headline_matches(&row.title, &row.topic, needles) {
            continue;
        }
        out.push(InstrumentHeadline {
            title: row.title,
            url: row.links.0.first().cloned(),
            published_at: row.published_at,
            source: Some(row.topic),
        });
        if out.len() as i64 >= limit {
            break;
        }
    }
    Ok(out)
}

pub async fn select_today_articles(
    pool: &sqlx::SqlitePool,
) -> anyhow::Result<TodayNews> {
    use sqlx::types::Json;

    let articles = sqlx::query_as!(
        SavedArticle,
        r#"
            SELECT 
                t.name as topic,
                a.title,
                a.content as "content!", 
                authors as "authors: Json<Vec<String>>",
                links as "links: Json<Vec<String>>",
                published_at as "published_at: chrono::DateTime<chrono::Utc>"
            FROM article as a
            JOIN topic as t ON a.topic_id = t.id
            WHERE a.published_at > datetime('now', '-1 days') AND a.content IS NOT NULL
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::new();

    for a in articles {
        map.entry(a.topic.clone())
            .or_insert_with(Vec::new)
            .push(a.into_feed());
    }

    let topics = map
        .into_iter()
        .map(|(topic, articles)| ArticlesWithTopic { topic, articles })
        .collect();

    Ok(TodayNews { topics })
}

#[cfg(test)]
mod tests {
    use super::headline_matches;

    #[test]
    fn given_title_contains_symbol_when_headline_matches_then_true() {
        assert!(headline_matches(
            "Tesla misses deliveries",
            "markets",
            &["TSLA".into(), "Tesla".into()],
        ));
    }

    #[test]
    fn given_unrelated_headline_when_headline_matches_then_false() {
        assert!(!headline_matches(
            "Gujarat State Petronet",
            "india",
            &["US500".into(), "S&P 500".into()],
        ));
    }
}
