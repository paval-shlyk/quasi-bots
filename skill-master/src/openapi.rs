use utoipa::OpenApi;

use crate::{config, finance, news, quotes, search};
use knowledge;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Knowledge Bank
        knowledge::post_new_knowledge,
        knowledge::post_next_daily_question,
        knowledge::get_all_topics,
        knowledge::get_all_tags,
        knowledge::post_topic_affinity,
        knowledge::post_entry_affinity,
        knowledge::post_entry_review,
        knowledge::get_recent_reviews,
        
        // News Bank
        news::get_today_news,
        news::get_chosen_topics,
        news::post_chosen_topic,
        
        // Search
        search::get_search,
        
        // Quotes Bank
        quotes::get_known_authors,
        quotes::post_next_unused_quote,
        
        // Market Tracker
        finance::get_report,
        finance::get_market_recommendations,
    ),
    components(
        schemas(
            // Knowledge Schemas
            knowledge::HumanEntry,
            knowledge::NewKnowledge,
            knowledge::QuestionBody,
            knowledge::TopicWithStatistics,
            knowledge::Affinity,
            knowledge::Review,
            knowledge::ReviewAttempts,
            
            // News Schemas
            news::Article,
            config::RssSource,
            
            // Search Schemas
            search::FetchedArticle,
            search::KnowledgeGraph,
            search::SearchResult,
            
            // Quotes Schemas
            quotes::FamousQuote,
            quotes::QuoteAuthor,
            
            // Finance Schemas
            finance::metrics::RsiSignal,
            finance::metrics::Volatility,
            finance::metrics::TechnicalReport,
        )
    ),
    tags(
        (name = "knowledge", description = "Knowledge Bank API"),
        (name = "quotes", description = "Quotes Bank API"),

        (name = "news", description = "News Bank API"),
        (name = "search", description = "Search API"),
        (name = "finance", description = "Market Tracker API")
    )
)]
#[rustfmt::skip]
pub struct ApiDoc;
