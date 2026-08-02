use anyhow::Context;

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct SearchQuery {
    pub query: String,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct FetchedArticle {
    pub title: String,
    pub link: String,
    /// short description of the article, e.g. the snippet from google news
    pub snippet: String,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct KnowledgeGraph {
    pub description: String,
    pub source_url: String,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct SearchResult {
    pub answer: Option<String>,
    /// knowledge graph information about the search query, e.g. a short description of the topic
    /// or a list of related topics
    pub knowledge_graph: Option<KnowledgeGraph>,
    pub articles: Vec<FetchedArticle>,
}

pub async fn perform_search(
    client: &reqwest::Client,
    api_key: &str,
    query: &str,
) -> anyhow::Result<SearchResult> {
    //todo: support other search engines, e.g. bing, duckduckgo, etc.

    let url = format!(
        "https://serpapi.com/search?q={query}&engine={engine}&api_key={api_key}&num=10",
        engine = "google"
    );

    let results = client
        .get(&url)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let organic_results = results["organic_results"]
        .as_array()
        .with_context(|| "organic_results is not an array")?
        .clone();

    let articles = organic_results
        .into_iter()
        .map(|result| {
            let title =
                result["title"].as_str().unwrap_or_default().to_string();
            let link = result["link"].as_str().unwrap_or_default().to_string();
            let snippet =
                result["snippet"].as_str().unwrap_or_default().to_string();

            FetchedArticle {
                title,
                link,
                snippet,
            }
        })
        .collect();

    let parse_knowledge_graph = || {
        let graph = results["knowledge_graph"].as_object()?;

        let description = graph.get("description")?.as_str()?.to_string();
        let source_url = graph.get("source")?.as_str()?.to_string();

        Some(KnowledgeGraph {
            description,
            source_url,
        })
    };

    let parse_answer = || {
        let answer_box = results["answer_box"].as_object()?;
        let answer = answer_box.get("answer")?.as_str()?.to_string();
        Some(answer)
    };

    Ok(SearchResult {
        answer: parse_answer(),
        knowledge_graph: parse_knowledge_graph(),
        articles,
    })
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("Failed to build HTTP client")
}
