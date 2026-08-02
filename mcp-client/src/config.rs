/// Connection and auth options for an MCP Streamable HTTP session.
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// Full MCP endpoint URL, e.g. `http://127.0.0.1:8080/mcp`.
    pub url: String,
    /// Optional Bearer access token (without the `Bearer ` prefix).
    pub token: Option<String>,
    /// OAuth redirect URI used for the local PKCE callback server.
    pub oauth_redirect: String,
    /// OAuth scope advertised to the authorization server.
    pub scope: String,
    /// Dynamic-registration client name.
    pub client_name: String,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:8080/mcp".into(),
            token: None,
            oauth_redirect: "http://127.0.0.1:9876/callback".into(),
            scope: "mcp".into(),
            client_name: "mcp-client".into(),
        }
    }
}

impl ConnectOptions {
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }
}

/// Build a minimal JSON object from a JSON Schema `properties` map.
///
/// Required fields get empty-string / empty-array / null defaults; optional
/// fields are omitted so the user can fill them in the TUI.
pub fn empty_args_from_schema(
    schema: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let required: Vec<String> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    // If nothing is required, prefer `{}` for zero-arg tools.
    if required.is_empty() && properties.is_empty() {
        return serde_json::json!({});
    }

    let mut obj = serde_json::Map::new();
    let keys: Vec<String> = if required.is_empty() {
        // Seed all properties so the user sees the full shape.
        properties.keys().cloned().collect()
    } else {
        required
    };

    for key in keys {
        let default = properties
            .get(&key)
            .map(default_for_schema_prop)
            .unwrap_or(serde_json::Value::Null);
        obj.insert(key, default);
    }
    serde_json::Value::Object(obj)
}

fn default_for_schema_prop(prop: &serde_json::Value) -> serde_json::Value {
    let ty = prop
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("string");
    match ty {
        "string" => serde_json::Value::String(String::new()),
        "integer" | "number" => serde_json::json!(0),
        "boolean" => serde_json::Value::Bool(false),
        "array" => serde_json::json!([]),
        "object" => serde_json::json!({}),
        _ => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_args_no_props() {
        let schema = serde_json::Map::new();
        assert_eq!(empty_args_from_schema(&schema), serde_json::json!({}));
    }

    #[test]
    fn empty_args_required_string() {
        let schema: serde_json::Map<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer" }
                },
                "required": ["query"]
            }))
            .unwrap();
        let args = empty_args_from_schema(&schema);
        assert_eq!(args["query"], "");
        assert!(args.get("limit").is_none());
    }
}
