use futures::{StreamExt, stream::FuturesUnordered};

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

            struct ArticleWithId {
                id: i64,
                article: article::RawArticle,
            }

            let summarize = move |a: ArticleWithId| {
                let gemini_api = gemini_api.clone();
                async move { (a.id, a.article.summarize(&gemini_api).await) }
            };

            async move {
                let articles =
                    article::fetch_raw_articles(&client, url.clone())
                        .await
                        .inspect_err(|e| {
                            tracing::warn!(
                                "Error fetching feed {}: {}",
                                url,
                                e
                            );
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

                let mut new_articles = vec![];

                for a in articles {
                    use sqlx::types::Json;
                    let authors = Json(a.authors.clone());
                    let links = Json(a.links.clone());
                    let title = a.title.clone();

                    let maybe_id = sqlx::query!(
                        r#"
                            INSERT INTO article
                                (topic_id, title, authors, links, published_at)
                            VALUES
                                (?, ?, ?, ?, ?)
                            RETURNING id as "id!"
                        "#,
                        topic_id,
                        title,
                        authors,
                        links,
                        a.published_at
                    )
                    .fetch_one(&pool)
                    .await;

                    let article_id = match maybe_id {
                        Ok(r) => r.id,
                        Err(sqlx::Error::Database(e))
                            if e.is_unique_violation() =>
                        {
                            tracing::debug!(
                                "Article '{}' already exists, skipping",
                                a.title
                            );

                            continue;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to insert article: {e}");
                            continue;
                        }
                    };

                    new_articles.push(ArticleWithId {
                        id: article_id,
                        article: a,
                    });
                }

                let articles = new_articles
                    .into_iter()
                    .map(summarize)
                    .collect::<FuturesUnordered<_>>()
                    .collect::<Vec<_>>()
                    .await;

                for (a_id, maybe_a) in articles {
                    match maybe_a {
                        Ok(a) => {
                            tracing::info!(
                                "Article with id {} summarized successfully",
                                a_id
                            );

                            sqlx::query!(
                                r#"
                                    UPDATE article
                                    SET content = ?
                                    WHERE id = ?
                                "#,
                                a.content,
                                a_id
                            )
                            .execute(&pool)
                            .await
                            .inspect_err(|e| {
                                tracing::warn!(
                                    "Failed to update article with summary: {e}"
                                );
                            })
                            .ok()?;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to summarize article with id {}: {}",
                                a_id,
                                e
                            );
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

        tracing::info!(
            "Refresh task completed, sleeping for {} seconds",
            config.refresh_timeout.as_secs()
        );

        tokio::time::sleep(config.refresh_timeout).await;
    }
}

pub async fn purge_task(_state: crate::NewsState) {}
