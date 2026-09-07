//! MCP-client-friendly JSON Schema transforms.
//!
//! schemars emits nullable `Option<T>` fields as JSON Schema type arrays, e.g.
//! `{"type": ["number", "null"]}`. That is valid JSON Schema, but many MCP
//! clients treat `type` as a single string and reject or drop the constraint.
//!
//! This module rewrites those arrays into `anyOf` branches with a single `type`
//! each, preserving nullability (absent ≠ null).

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use schemars::Schema;
use schemars::transform::{Transform, transform_subschemas};
use serde_json::{Map, Value};

/// schemars [`Transform`]: convert nullable `type` arrays into `anyOf`.
///
/// Use with `#[schemars(transform = crate::mcp::schema::nullable_type_arrays_to_anyof)]`
/// on containers, or call [`rewrite_value`] / [`rewrite_json_object`] on emitted
/// schemas (e.g. after rmcp `schema_for_type`).
pub fn nullable_type_arrays_to_anyof(schema: &mut Schema) {
    NullableTypeArraysToAnyOf.transform(schema);
}

#[derive(Debug, Clone, Default)]
struct NullableTypeArraysToAnyOf;

impl Transform for NullableTypeArraysToAnyOf {
    fn transform(&mut self, schema: &mut Schema) {
        transform_subschemas(self, schema);
        if let Some(obj) = schema.as_object_mut() {
            rewrite_type_array_null_in_map(obj);
        }
    }
}

/// Rewrite a JSON Schema object (MCP `inputSchema` / `outputSchema`) in place.
pub fn rewrite_json_object(schema: &mut Map<String, Value>) {
    let mut as_value = Value::Object(std::mem::take(schema));
    rewrite_value(&mut as_value);
    match as_value {
        Value::Object(rewritten) => *schema = rewritten,
        other => {
            // Schema roots are always objects; keep a defensive object wrapper.
            let mut wrapped = Map::new();
            wrapped.insert("anyOf".into(), Value::Array(vec![other]));
            *schema = wrapped;
        }
    }
}

/// Rewrite any JSON Schema value tree in place.
pub fn rewrite_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Snapshot keys so we can mutate children safely.
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                match key.as_str() {
                    "not"
                    | "if"
                    | "then"
                    | "else"
                    | "contains"
                    | "additionalProperties"
                    | "propertyNames"
                    | "additionalItems"
                    | "unevaluatedItems"
                    | "unevaluatedProperties" => {
                        if let Some(child) = map.get_mut(&key) {
                            rewrite_value(child);
                        }
                    }
                    "items" => {
                        if let Some(child) = map.get_mut(&key) {
                            match child {
                                Value::Array(arr) => {
                                    for v in arr {
                                        rewrite_value(v);
                                    }
                                }
                                other => rewrite_value(other),
                            }
                        }
                    }
                    "allOf" | "anyOf" | "oneOf" | "prefixItems" => {
                        if let Some(Value::Array(arr)) = map.get_mut(&key) {
                            for v in arr {
                                rewrite_value(v);
                            }
                        }
                    }
                    "properties" | "patternProperties" | "$defs"
                    | "definitions" | "dependentSchemas" => {
                        if let Some(Value::Object(props)) = map.get_mut(&key) {
                            for v in props.values_mut() {
                                rewrite_value(v);
                            }
                        }
                    }
                    _ => {}
                }
            }
            rewrite_type_array_null_in_map(map);
        }
        Value::Array(arr) => {
            for v in arr {
                rewrite_value(v);
            }
        }
        _ => {}
    }
}

/// Convert `{"type": ["T", "null"], ...}` into
/// `{"anyOf": [{"type": "T", ...}, {"type": "null"}]}`.
fn rewrite_type_array_null_in_map(map: &mut Map<String, Value>) {
    let Some(Value::Array(types)) = map.get("type") else {
        return;
    };

    let has_null = types.iter().any(|t| t.as_str() == Some("null"));
    if !has_null {
        return;
    }

    let non_null: Vec<Value> = types
        .iter()
        .filter(|t| t.as_str() != Some("null"))
        .cloned()
        .collect();

    if non_null.is_empty() {
        map.insert("type".into(), Value::String("null".into()));
        return;
    }

    if non_null.len() == 1 {
        let mut typed = map.clone();
        typed.insert("type".into(), non_null.into_iter().next().unwrap());
        let null_branch = serde_json::json!({ "type": "null" });
        map.clear();
        map.insert(
            "anyOf".into(),
            Value::Array(vec![Value::Object(typed), null_branch]),
        );
        return;
    }

    // Multiple non-null types plus null: one branch per concrete type + null.
    let mut branches: Vec<Value> = non_null
        .into_iter()
        .map(|ty| {
            let mut branch = map.clone();
            branch.insert("type".into(), ty);
            Value::Object(branch)
        })
        .collect();
    branches.push(serde_json::json!({ "type": "null" }));
    map.clear();
    map.insert("anyOf".into(), Value::Array(branches));
}

/// Apply the nullable→anyOf rewrite to every tool input/output schema on a router.
pub fn rewrite_tool_router_schemas<S>(router: &mut ToolRouter<S>) {
    for route in router.map.values_mut() {
        {
            let obj = Arc::make_mut(&mut route.attr.input_schema);
            rewrite_json_object(obj);
        }
        if let Some(output) = route.attr.output_schema.as_mut() {
            let obj = Arc::make_mut(output);
            rewrite_json_object(obj);
        }
    }
}

/// Return true if `value` still contains a `type` array that includes `"null"`.
pub fn contains_nullable_type_array(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(types)) = map.get("type")
                && types.iter().any(|t| t.as_str() == Some("null"))
            {
                return true;
            }
            map.values().any(contains_nullable_type_array)
        }
        Value::Array(arr) => arr.iter().any(contains_nullable_type_array),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::json;

    use super::*;

    #[allow(dead_code)]
    #[derive(Debug, Deserialize, JsonSchema)]
    struct SampleArgs {
        #[serde(default)]
        year: Option<i32>,
        #[serde(default)]
        symbols: Option<Vec<String>>,
        name: String,
    }

    fn schema_value_for<T: JsonSchema>() -> Value {
        // Match rmcp's generator settings (draft 2020-12, no AddNullable).
        let settings = schemars::generate::SchemaSettings::draft2020_12();
        let generator = settings.into_generator();
        let schema = generator.into_root_schema_for::<T>();
        serde_json::to_value(schema).expect("serialize schema")
    }

    #[test]
    fn rewrites_number_null_type_array_to_anyof() {
        let mut schema = json!({
            "type": ["number", "null"],
            "description": "optional amount"
        });
        rewrite_value(&mut schema);
        assert_eq!(
            schema,
            json!({
                "anyOf": [
                    { "type": "number", "description": "optional amount" },
                    { "type": "null" }
                ]
            })
        );
        assert!(!contains_nullable_type_array(&schema));
    }

    #[test]
    fn rewrites_array_null_type_array_to_anyof() {
        let mut schema = json!({
            "type": ["array", "null"],
            "items": { "type": "string" }
        });
        rewrite_value(&mut schema);
        assert_eq!(
            schema,
            json!({
                "anyOf": [
                    {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    { "type": "null" }
                ]
            })
        );
    }

    #[test]
    fn leaves_non_nullable_type_arrays_alone() {
        let mut schema = json!({
            "type": ["string", "number"]
        });
        rewrite_value(&mut schema);
        assert_eq!(schema, json!({ "type": ["string", "number"] }));
    }

    #[test]
    fn leaves_single_string_type_alone() {
        let mut schema = json!({ "type": "string" });
        rewrite_value(&mut schema);
        assert_eq!(schema, json!({ "type": "string" }));
    }

    #[test]
    fn schemars_option_fields_use_anyof_after_rewrite() {
        let mut schema = schema_value_for::<SampleArgs>();
        assert!(
            contains_nullable_type_array(&schema),
            "precondition: schemars should emit type arrays for Option; got {schema}"
        );
        rewrite_value(&mut schema);
        assert!(
            !contains_nullable_type_array(&schema),
            "nullable type arrays must be rewritten; got {schema}"
        );

        let props = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("object properties");

        let year = props.get("year").expect("year property");
        assert!(
            year.get("anyOf").is_some(),
            "year should use anyOf; got {year}"
        );
        assert!(year.get("type").is_none(), "year must not keep type array");

        let symbols = props.get("symbols").expect("symbols property");
        assert!(
            symbols.get("anyOf").is_some(),
            "symbols should use anyOf; got {symbols}"
        );

        let name = props.get("name").expect("name property");
        assert_eq!(name.get("type"), Some(&json!("string")));
    }

    #[test]
    fn schemars_transform_fn_rewrites_root_schema() {
        let mut schema = schemars::schema_for!(SampleArgs);
        nullable_type_arrays_to_anyof(&mut schema);
        let value = serde_json::to_value(schema).unwrap();
        assert!(!contains_nullable_type_array(&value));
    }
}
