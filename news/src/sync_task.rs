use crate::article;

pub fn spawn(state: crate::NewsState) {
    tokio::task::spawn(refresh_task(state.clone()));
    tokio::task::spawn(purge_task(state));
}

pub async fn refresh_task(state: crate::NewsState) {
    let client = reqwest::Client::new();

    loop {
        let config = state.config.clone();

        let sources = state.config.rss_sources.clone();

        let mut tasks = tokio::task::JoinSet::new();

        let next_task = |client, topic: String, url: reqwest::Url| {
            let gemini_api = state.gemini_api.clone();
            let pool = state.pool.clone();

            async move {
                let articles = article::fetch_feed_articles(
                    &client,
                    url.clone(),
                    gemini_api,
                )
                .await
                .inspect_err(|e| {
                    tracing::warn!("Error fetching feed {}: {}", url, e);
                })
                .ok()?;

                let topic_id = sqlx::query!(
                    r#"
                        INSERT INTO news_topic (name) VALUES (?)
                        ON CONFLICT(name) DO UPDATE SET name = excluded.name
                        RETURNING id
                    "#,
                    topic
                )
                .fetch_one(&pool)
                .await
                .inspect_err(|e| {
                    tracing::warn!("Failed to insert topic: {e}");
                })
                .ok()?
                .id;

                //fixme: use batch insert instead of single queries
                for a in articles {
                    use sqlx::types::Json;
                    let authors = Json(a.authors);
                    let links = Json(a.links);

                    let insert_result = sqlx::query!(
                        r#"
                            INSERT INTO article
                                (topic_id, title, content, authors, links, published_at)
                            VALUES
                                (?, ?, ?, ?, ?, ?)
                        "#,
                        topic_id, a.title, a.content, authors, links, a.published_at
                    )
                    .execute(&pool)
                    .await;

                    match insert_result {
                        Ok(_) => {}
                        Err(sqlx::Error::Database(e))
                            if e.is_unique_violation() =>
                        {
                            tracing::info!(
                                "Article '{}' already exists, skipping",
                                a.title
                            );
                        }
                        Err(e) => {
                            tracing::warn!("Failed to insert article: {e}");
                        }
                    }
                }

                Some(())
            }
        };

        for source in sources {
            for url in source.urls {
                tasks.spawn(next_task(
                    client.clone(),
                    source.topic.clone(),
                    url,
                ));
            }
        }

        //wait while all queries will be done,
        //but ignore errors, because they are already logged
        let _ = tasks.join_all().await;

        tokio::time::sleep(config.refresh_timeout).await;
    }
}

pub async fn purge_task(_state: crate::NewsState) {}
