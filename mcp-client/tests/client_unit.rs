use mcp_client::empty_args_from_schema;

#[test]
fn empty_args_from_schema_required_only() {
    let schema: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "entry_name": { "type": "string" },
                "attempts": { "type": "integer" },
                "optional": { "type": "boolean" }
            },
            "required": ["entry_name", "attempts"]
        }))
        .unwrap();

    let args = empty_args_from_schema(&schema);
    assert_eq!(args["entry_name"], "");
    assert_eq!(args["attempts"], 0);
    assert!(args.get("optional").is_none());
}

#[test]
fn empty_args_no_required_lists_all_props() {
    let schema: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {
                "days": { "type": "integer" }
            }
        }))
        .unwrap();

    let args = empty_args_from_schema(&schema);
    assert_eq!(args["days"], 0);
}
