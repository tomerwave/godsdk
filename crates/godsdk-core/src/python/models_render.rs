use super::{
    ApiIr, Operation, Schema, inline_success_schema, operation_response_name, python_identifier,
    schema_model_name, type_identifier,
};
use crate::code_writer::CodeWriter;

pub(super) fn render_models(spec: &ApiIr) -> String {
    let mut lines = vec![
        "from __future__ import annotations".to_string(),
        String::new(),
        "from enum import Enum".to_string(),
        "from typing import TypeAlias".to_string(),
        "from pydantic import BaseModel, ConfigDict".to_string(),
        String::new(),
        "JsonValue: TypeAlias = None | bool | int | float | str | list[\"JsonValue\"] | dict[str, \"JsonValue\"]".to_string(),
        String::new(),
    ];
    for (name, schema) in &spec.schemas {
        lines.extend(model_lines(name, schema, spec));
    }
    for operation in &spec.operations {
        if let Some(schema) = inline_success_schema(operation) {
            lines.extend(model_lines(
                &operation_response_name(operation),
                schema,
                spec,
            ));
        }
        if let Some(schema) = inline_request_schema(operation) {
            lines.extend(model_lines(
                &operation_request_name(operation),
                schema,
                spec,
            ));
        }
    }
    CodeWriter::from_lines(lines)
}

fn inline_request_schema(operation: &Operation) -> Option<&Schema> {
    operation
        .request_body_details
        .as_ref()
        .and_then(|body| body.schema.as_ref())
        .filter(|schema| schema_model_name(schema).is_none())
}

fn operation_request_name(operation: &Operation) -> String {
    format!("{}Request", type_identifier(&operation.operation_id))
}

fn model_lines(name: &str, schema: &Schema, spec: &ApiIr) -> Vec<String> {
    if let Schema::Enum(values) = schema {
        let mut lines = vec![format!("class {name}(str, Enum):")];
        lines.extend(
            values
                .iter()
                .map(|value| format!("    {} = {value:?}", enum_identifier(value))),
        );
        lines.push(String::new());
        return lines;
    }
    let mut lines = vec![
        format!("class {name}(BaseModel):"),
        "    model_config = ConfigDict(extra=\"forbid\")".to_string(),
    ];
    lines.extend(object_fields(schema, spec).into_iter().map(
        |(property, property_schema, required)| {
            let annotation = python_type(&property_schema);
            if required {
                format!("    {}: {annotation}", python_identifier(&property))
            } else {
                format!(
                    "    {}: {annotation} | None = None",
                    python_identifier(&property)
                )
            }
        },
    ));
    lines.push(String::new());
    lines
}

fn object_fields(schema: &Schema, spec: &ApiIr) -> Vec<(String, Schema, bool)> {
    match schema {
        Schema::Object {
            properties,
            required,
            ..
        } => properties
            .iter()
            .map(|(name, schema)| (name.clone(), schema.clone(), required.contains(name)))
            .collect(),
        Schema::AllOf(parts) => parts
            .iter()
            .flat_map(|part| object_fields(part, spec))
            .collect(),
        Schema::Reference(name) => spec
            .schemas
            .get(name)
            .map(|schema| object_fields(schema, spec))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn python_type(schema: &Schema) -> String {
    match schema {
        Schema::String { .. } => "str".to_string(),
        Schema::Integer { .. } => "int".to_string(),
        Schema::Number { .. } => "float".to_string(),
        Schema::Boolean => "bool".to_string(),
        Schema::Null => "None".to_string(),
        Schema::Array(item) => format!("list[{}]", python_type(item)),
        Schema::Object { .. } => "dict[str, JsonValue]".to_string(),
        Schema::Enum(_) | Schema::Reference(_) => {
            schema_model_name(schema).unwrap_or_else(|| "str".to_string())
        }
        Schema::Nullable(inner) => format!("{} | None", python_type(inner)),
        Schema::OneOf(values) | Schema::AnyOf(values) | Schema::AllOf(values) => values
            .iter()
            .map(python_type)
            .collect::<Vec<_>>()
            .join(" | "),
    }
}

fn enum_identifier(value: &str) -> String {
    type_identifier(value).to_ascii_uppercase()
}
