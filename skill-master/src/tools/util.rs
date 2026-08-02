use rmcp::handler::server::wrapper::Json;
use serde::Serialize;

pub fn json<T: Serialize>(value: T) -> Result<Json<serde_json::Value>, String> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|e| e.to_string())
}
