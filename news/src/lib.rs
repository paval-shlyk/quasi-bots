mod article_parser;
mod config;
mod llm;
mod model;
mod state;
mod sync_task;

use axum::{Json, extract::State, response::IntoResponse};
use reqwest::StatusCode;

pub use config::*;
pub use model::*;
pub use state::*;

pub use llm::*;

pub async fn connect(config: Config) -> anyhow::Result<NewsState> {
    Ok(NewsState {
        gemini_api: llm::GeminiApi::connect(config.gemini_config.clone())
            .await?,
        config: std::sync::Arc::new(config),
    })
}

#[derive(Debug, Clone, Hash)]
pub struct Entry {
    pub title: String,
    pub source: String,
    pub authors: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/news-bank/topics",
    responses(
        (status = 501, description = "Not implemented")
    )
)]
pub async fn post_chosen_topic(
    State(_state): State<NewsState>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, "Topic added")
}

#[utoipa::path(
    get,
    path = "/news-bank/topics",
    responses(
        (status = 200, description = "Topics retrieved successfully", body = Vec<RssSource>)
    )
)]
pub async fn get_chosen_topics(
    State(state): State<NewsState>,
) -> impl IntoResponse {
    let topics = state.config.rss_sources.to_vec();

    Json(topics)
}

#[utoipa::path(
    get,
    path = "/news-bank/today",
    responses(
        (status = 200, description = "Today's news retrieved successfully", body = Vec<FetchedArticle>)
    )
)]
pub async fn get_today_news(
    State(state): State<NewsState>,
) -> impl IntoResponse {
    //todo: metric to estimate time
    let time = std::time::Instant::now();

    let client = reqwest::Client::new();

    let sources = state.config.rss_sources.clone();

    let mut tasks = tokio::task::JoinSet::new();

    let gemini_api = state.gemini_api.clone();
    let next_task = move |client, topic: String, url: reqwest::Url| {
        let gemini_api = gemini_api.clone();

        async move {
            match fetch_feed(&client, url.clone(), gemini_api).await {
                Ok(articles) => Some(FetchedArticle { topic, articles }),
                Err(e) => {
                    tracing::warn!("Error fetching feed {}: {}", url, e);
                    None
                }
            }
        }
    };

    for source in sources {
        for url in source.urls {
            tasks.spawn(next_task(client.clone(), source.topic.clone(), url));
        }
    }

    let articles = tasks
        .join_all()
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    tracing::info!(
        "Elapsed to fetch all articles: {:.2} ms",
        time.elapsed().as_millis()
    );

    Json(articles)
}

async fn fetch_feed(
    client: &reqwest::Client,
    url: reqwest::Url,
    api: llm::GeminiApi,
) -> anyhow::Result<Vec<Article>> {
    let resp = client.get(url).send().await?;
    let content = resp.bytes().await?;

    let (tx, rx) = tokio::sync::oneshot::channel();

    let start = std::time::Instant::now();

    rayon::spawn(move || {
        match article_parser::parse(content.as_ref()) {
            Ok(articles) => tx.send(Ok(articles)),
            Err(e) => tx.send(Err(e)),
        }
        .expect("Rx drops later");
    });

    let articles = rx.await.expect("Tx cannot die")?;

    let mut tasks = tokio::task::JoinSet::new();

    for a in articles.into_iter() {
        tasks.spawn((|| {
            let api = api.clone();
            async move { a.summarize(&api).await }
        })());
    }

    let news = tasks
        .join_all()
        .await
        .into_iter()
        .map(|a| a.unwrap())
        .collect();

    tracing::info!("Took to fetch news: {:.2} ms", start.elapsed().as_millis());

    Ok(news)
}
