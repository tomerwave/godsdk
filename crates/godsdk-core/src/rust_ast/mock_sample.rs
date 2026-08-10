use serde_json::{Map, Value};

use crate::{ApiIr, Operation, Schema};

const MAX_REFERENCE_DEPTH: usize = 6;

pub(crate) fn success_body(spec: &ApiIr, operation: &Operation) -> Vec<u8> {
    let schema = operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .and_then(|response| response.schema.as_ref());
    let value = schema.map_or_else(
        || Value::Object(Map::new()),
        |schema| sample_schema(schema, spec, 0),
    );
    serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec())
}

pub(super) fn marker(body: &[u8]) -> String {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(first_string)
        .unwrap_or_else(|| "response".to_string())
}

fn sample_schema(schema: &Schema, spec: &ApiIr, depth: usize) -> Value {
    if depth > MAX_REFERENCE_DEPTH {
        return Value::Null;
    }
    match schema {
        Schema::String { .. } => Value::String("example".to_string()),
        Schema::Integer { .. } => Value::from(1),
        Schema::Number { .. } => Value::from(1.0),
        Schema::Boolean => Value::Bool(true),
        Schema::Null => Value::Null,
        Schema::Array(item) => Value::Array(vec![sample_schema(item, spec, depth + 1)]),
        Schema::Object { properties, .. } => {
            let mut object = Map::new();
            for (name, property) in properties {
                object.insert(
                    name.clone(),
                    sample_property(name, property, spec, depth + 1),
                );
            }
            Value::Object(object)
        }
        Schema::Enum(values) => values
            .first()
            .map_or(Value::Null, |value| Value::String(value.clone())),
        Schema::Reference(name) => spec
            .schemas
            .get(name)
            .map_or(Value::Null, |schema| sample_schema(schema, spec, depth + 1)),
        Schema::Nullable(inner) => sample_schema(inner, spec, depth + 1),
        Schema::OneOf(values) | Schema::AnyOf(values) => values
            .first()
            .map_or(Value::Null, |schema| sample_schema(schema, spec, depth + 1)),
        Schema::AllOf(values) => merge_all_of(values, spec, depth + 1),
    }
}

fn sample_property(name: &str, schema: &Schema, spec: &ApiIr, depth: usize) -> Value {
    match name.to_ascii_lowercase().as_str() {
        "id" => Value::String("pet-1".to_string()),
        "name" => Value::String("Fluffy".to_string()),
        "species" => Value::String("cat".to_string()),
        "status" => Value::String("ok".to_string()),
        "service" => Value::String("mock-pets-api".to_string()),
        _ => sample_schema(schema, spec, depth),
    }
}

fn merge_all_of(values: &[Schema], spec: &ApiIr, depth: usize) -> Value {
    let mut merged = Map::new();
    for schema in values {
        if let Value::Object(object) = sample_schema(schema, spec, depth) {
            merged.extend(object);
        }
    }
    Value::Object(merged)
}

fn first_string(value: Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value),
        Value::Array(values) => values.into_iter().find_map(first_string),
        Value::Object(values) => values.into_values().find_map(first_string),
        Value::Bool(_) | Value::Null | Value::Number(_) => None,
    }
}
