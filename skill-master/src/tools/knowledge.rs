use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::mcp::server::SkillMasterMcpServer;

use super::util::json;

#[derive(Debug, Deserialize, JsonSchema)]
struct NewKnowledge {
    topic_id: u64,
    question: String,
    truth: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ReviewAttempts {
    entry_name: String,
    attempts: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RecentReviewsQuery {
    #[serde(default)]
    days: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TopicAffinity {
    topic_id: u64,
    days: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EntryAffinity {
    entry_name: String,
    days: u32,
}

#[tool_router(router = knowledge_tool_router, vis = "pub")]
impl SkillMasterMcpServer {
    #[tool(description = "Fetch the next daily knowledge question")]
    async fn knowledge_next_question(
        &self,
    ) -> Result<Json<serde_json::Value>, String> {
        knowledge::fetch_random_entry(&self.state.pool)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }

    #[tool(description = "List all knowledge topics with statistics")]
    async fn knowledge_list_topics(
        &self,
    ) -> Result<Json<serde_json::Value>, String> {
        knowledge::fetch_topics(&self.state.pool)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }

    #[tool(description = "List all knowledge tags")]
    async fn knowledge_list_tags(
        &self,
    ) -> Result<Json<serde_json::Value>, String> {
        knowledge::fetch_tags(&self.state.pool)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }

    #[tool(
        description = "Set review affinity for a topic (days until next review, 0 to clear)"
    )]
    async fn knowledge_set_topic_affinity(
        &self,
        Parameters(TopicAffinity { topic_id, days }): Parameters<TopicAffinity>,
    ) -> Result<String, String> {
        knowledge::set_topic_affinity(topic_id, days, &self.state.pool)
            .await
            .map(|_| "ok".to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Set review affinity for an entry (days until next review, 0 to clear)"
    )]
    async fn knowledge_set_entry_affinity(
        &self,
        Parameters(EntryAffinity { entry_name, days }): Parameters<
            EntryAffinity,
        >,
    ) -> Result<String, String> {
        knowledge::set_entry_affinity(entry_name, days, &self.state.pool)
            .await
            .map(|_| "ok".to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Add a new knowledge entry")]
    async fn knowledge_add_entry(
        &self,
        Parameters(NewKnowledge {
            topic_id,
            question,
            truth,
            tags,
        }): Parameters<NewKnowledge>,
    ) -> Result<String, String> {
        knowledge::add_new_entry(
            &self.state.pool,
            topic_id,
            question,
            truth,
            tags,
        )
        .await
        .map(|_| "ok".to_string())
        .map_err(|e| e.to_string())
    }

    #[tool(description = "Record a review attempt for a knowledge entry")]
    async fn knowledge_update_review(
        &self,
        Parameters(ReviewAttempts {
            entry_name,
            attempts,
        }): Parameters<ReviewAttempts>,
    ) -> Result<String, String> {
        knowledge::update_review(&self.state.pool, entry_name, attempts as i32)
            .await
            .map(|_| "ok".to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Fetch recent knowledge reviews")]
    async fn knowledge_recent_reviews(
        &self,
        Parameters(RecentReviewsQuery { days }): Parameters<RecentReviewsQuery>,
    ) -> Result<Json<serde_json::Value>, String> {
        knowledge::fetch_recent_reviews(&self.state.pool, days)
            .await
            .map_err(|e| e.to_string())
            .and_then(json)
    }
}
