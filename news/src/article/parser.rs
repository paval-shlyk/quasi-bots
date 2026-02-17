use crate::RawArticle;

pub fn parse_feed(feed_content: &[u8]) -> anyhow::Result<Vec<RawArticle>> {
    //huge width to prevent line breaks in the middle of sentences
    const HTML_WIDTH: usize = 1_000_000;
    let feed = feed_rs::parser::parse(feed_content)?;

    let articles = feed
        .entries
        .into_iter()
        .map(|entry| {
            let title = entry.title.map(|t| t.content).unwrap_or_default();
            let links = entry.links.into_iter().map(|l| l.href).collect::<Vec<_>>();

            let parse_body = |content_type: mediatype::MediaTypeBuf, body: String| -> Option<String> {
                //fixme: process other content types, e.g. markdown
                if content_type.subty().as_str() == "html" {
                    match html2text::from_read(body.as_bytes(), HTML_WIDTH) {
                        Ok(text) => Some(text),
                        Err(e) => {
                            tracing::error!("Failed to convert HTML to text for entry '{}': {}", title, e);
                            Some(body.clone())
                        }
                    }
                } else {
                    Some(body)
                }
            };

            let parse_summary = || entry.summary.and_then(|s| parse_body(s.content_type, s.content));
            let parse_content = || entry.content.and_then(|c| parse_body(c.content_type, c.body?));

            let authors = entry.authors.into_iter().map(|a| a.name).collect();

            RawArticle {
                summary: parse_summary(),
                content: parse_content(),
                title,
                links,
                authors,
                published_at: entry.published.unwrap_or_else(|| chrono::Utc::now()),
            }
        })
        .collect();

    Ok(articles)
}
