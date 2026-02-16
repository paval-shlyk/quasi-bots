use crate::Article;

//not fully parsed
#[derive(Debug)]
pub struct RawArticle {
    pub title: String,
    pub authors: Vec<String>,
    pub links: Vec<String>,

    pub content: Option<String>,
    pub summary: Option<String>,
}

impl RawArticle {
    pub async fn summarize(
        self,
        api: &crate::GeminiApi,
    ) -> anyhow::Result<Article> {
        let RawArticle {
            title,
            authors,
            links,
            mut content,
            summary,
        } = self;

        let summarized_content: Option<String>;

        //by default, we are not believing
        //to the summary text
        if let Some(text) = content.take() {
            summarized_content = api.summarize(&text).await?.into();
        } else {
            //if no content available then use summary information
            summarized_content = summary;
        }

        Ok(Article {
            title,
            authors,
            links,
            content: summarized_content.unwrap_or_default(),
        })
    }
}

pub fn parse(raw_feed: &[u8]) -> anyhow::Result<Vec<RawArticle>> {
    //huge width to prevent line breaks in the middle of sentences
    const HTML_WIDTH: usize = 1_000_000;
    let feed = feed_rs::parser::parse(raw_feed)?;

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
            }
        })
        .collect();

    Ok(articles)
}
