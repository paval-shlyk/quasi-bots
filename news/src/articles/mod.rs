mod model;
mod parser;
mod queries;

pub use model::*;
pub use queries::*;

pub async fn fetch_raw_articles(
    client: &reqwest::Client,
    url: reqwest::Url,
) -> anyhow::Result<Vec<RawArticle>> {
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

    tracing::debug!(
        "Took to fetch news: {:.2} ms",
        start.elapsed().as_millis()
    );

    Ok(raw_articles)
}
