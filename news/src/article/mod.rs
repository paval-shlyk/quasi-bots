mod model;
mod parser;
mod queries;

pub use model::*;
pub use queries::*;

pub async fn fetch_feed_articles(
    client: &reqwest::Client,
    url: reqwest::Url,
    api: crate::llm::GeminiApi,
) -> anyhow::Result<Vec<FeedArticle>> {
    let resp = client.get(url).send().await?;
    let content = resp.bytes().await?;

    let (tx, rx) = tokio::sync::oneshot::channel();

    let start = std::time::Instant::now();

    rayon::spawn(move || {
        match parser::parse_feed(content.as_ref()) {
            Ok(articles) => tx.send(Ok(articles)),
            Err(e) => tx.send(Err(e)),
        }
        .expect("Rx drops later");
    });

    let raw_articles = rx.await.expect("Tx cannot die")?;

    let mut tasks = tokio::task::JoinSet::new();

    for a in raw_articles.into_iter() {
        tasks.spawn((|| {
            let api = api.clone();
            async move { a.summarize(&api).await }
        })());
    }

    let articles = tasks
        .join_all()
        .await
        .into_iter()
        .map(|a| a.unwrap())
        .collect();

    tracing::info!("Took to fetch news: {:.2} ms", start.elapsed().as_millis());

    Ok(articles)
}
