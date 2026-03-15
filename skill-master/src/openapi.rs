use utoipa::OpenApi;

use crate::{quotes, search};

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

        // Expenses
        finance::expenses::list_categories,
        finance::expenses::create_category,
        finance::expenses::list_entries,
        finance::expenses::create_entry,
        finance::expenses::update_entry,
        finance::expenses::delete_entry,
        finance::expenses::monthly_report,
        finance::expenses::yearly_report,
        finance::expenses::weekly_report,
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
            news::FeedArticle,
            news::RssSource,
            
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

            // Expenses Schemas
            finance::expenses::Category,
            finance::expenses::ExpenseEntry,
            finance::expenses::ExpenseEntryWithCategory,
            finance::expenses::report::MonthlyReport,
            finance::expenses::report::YearReport,
            finance::expenses::report::WeeklyReport,
            finance::expenses::report::CategoryTotal,
            finance::expenses::report::MonthData,
            finance::expenses::report::WeekData,
        )
    ),
    tags(
        (name = "quotes", description = "Quotes Bank API"),
        (name = "knowledge", description = "Knowledge Bank API"),
        (name = "News", description = "News Bank API"),

        (name = "search", description = "Search API"),

        (name = "Finance", description = "Market Tracker & Expenses API")
    )
)]
#[rustfmt::skip]
pub struct ApiDoc;
