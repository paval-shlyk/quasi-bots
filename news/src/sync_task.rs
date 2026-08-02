use futures::{StreamExt, stream::FuturesUnordered};

use crate::{articles, links};

pub fn spawn(state: crate::NewsState) {
    tokio::task::spawn(refresh_task(state.clone()));
    tokio::task::spawn(purge_task(state));
}

pub async fn refresh_task(state: crate::NewsState) {
    let client = reqwest::Client::new();

    loop {
        let config = state.config.clone();

        let sources = crate::links::fetch_active_sources(&state)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to fetch active sources: {e}");
                vec![]
            });

        let mut tasks = tokio::task::JoinSet::new();

        let next_task = |client, topic: String, url: reqwest::Url| {
            let gemini_api = state.gemini_api.clone();
            let pool = state.pool.clone();
            let state = state.clone();

            struct ArticleWithId {
                id: i64,
                article: articles::RawArticle,
            }

            let summarize = move |a: ArticleWithId| {
                let gemini_api = gemini_api.clone();

                async move {
                    if let Some(api) = gemini_api.as_ref() {
                        (a.id, a.article.summarize(api).await)
                    } else {
                        (a.id, Ok(a.article.into_article_unchecked()))
                    }
                }
            };

            async move {
                let articles =
                    match articles::fetch_raw_articles(&client, url.clone())
                        .await
                    {
                        Ok(articles) => {
                            links::restore_broken(&state, url.as_str())
                            .await
                            .inspect_err(|e| {
                                tracing::warn!(
                                    "Failed to restore broken link for {}: {}",
                                    url,
                                    e
                                );
                            }).ok()?;

                            articles
                                .into_iter()
                                .filter(|a| {
                                    let age =
                                        chrono::Utc::now() - a.published_at;

                                    let max_age = chrono::TimeDelta::from_std(
                                        state.config.article_max_age,
                                    )
                                    .expect("Invalid article max age");

                                    age < max_age
                                })
                                .collect::<Vec<_>>()
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to fetch feed {}: {}",
                                url,
                                e
                            );

                            crate::links::set_broken(&state, url.as_str())
                                .await
                                .inspect_err(|e| {
                                    tracing::warn!(
                                        "Failed to set broken link for {}: {}",
                                        url,
                                        e
                                    );
                                })
                                .ok()?;

                            return None;
                        }
                    };

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
                let source_url = url.as_str();

                let source_id = sqlx::query!(
                    r#"
                        INSERT INTO news_source (url) VALUES (?)
                        ON CONFLICT(url) DO UPDATE SET url = excluded.url
                        RETURNING id
                    "#,
                    source_url,
                )
                .fetch_one(&pool)
                .await
                .inspect_err(|e| {
                    tracing::warn!("Failed to insert news source: {e}");
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
                                (topic_id, source_id, title, authors, links, published_at)
                            VALUES
                                (?, ?, ?, ?, ?, ?)
                            RETURNING id as "id!"
                        "#,
                        topic_id,
                        source_id,
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
                            tracing::debug!(
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

        state.purge_notify.notify_one();

        tokio::time::sleep(config.refresh_timeout).await;
    }
}

pub async fn purge_task(state: crate::NewsState) {
    loop {
        tokio::select! {
            _ = state.purge_notify.notified() => {
                tracing::info!("Purge task triggered by refresh task");
            }
            _ = tokio::time::sleep(state.config.article_max_age) => {
                tracing::info!("Purge task triggered by timeout");
            }
        }
        let cutoff_time = chrono::Utc::now()
            - chrono::Duration::from_std(state.config.article_max_age)
                .expect("Invalid article max age");

        let _ = sqlx::query!(
            r#"
                DELETE FROM article
                WHERE published_at < ?
            "#,
            cutoff_time
        )
        .execute(&state.pool)
        .await
        .inspect_err(|e| {
            tracing::warn!("Failed to purge old articles: {e}");
        });
    }
}
