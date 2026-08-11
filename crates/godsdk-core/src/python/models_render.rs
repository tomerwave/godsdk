use super::{
    ApiIr, Operation, Schema, inline_success_schema, operation_response_name, python_identifier,
    schema_model_name, type_identifier,
};
use crate::code_writer::{CodeWriter, concatenate};

pub(super) fn render_models(spec: &ApiIr) -> String {
    let mut lines = vec![
        "from __future__ import annotations".to_string(),
        String::new(),
        "from enum import Enum".to_string(),
        "from typing import Literal, TypeAlias".to_string(),
        "from pydantic import BaseModel, ConfigDict, Field".to_string(),
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
    [
        type_identifier(&operation.operation_id),
        "Request".to_string(),
    ]
    .concat()
}

fn model_lines(name: &str, schema: &Schema, spec: &ApiIr) -> Vec<String> {
    if let Some(lines) = alias_lines(name, schema) {
        return lines;
    }
    if let Some(lines) = enum_lines(name, schema) {
        return lines;
    }
    object_model_lines(name, schema, spec)
}

fn enum_lines(name: &str, schema: &Schema) -> Option<Vec<String>> {
    let Schema::Enum(values) = schema else {
        return None;
    };
    let mut lines = vec![["class ", name, "(str, Enum):"].concat()];
    lines.extend(values.iter().map(|value| {
        [
            "    ".to_string(),
            enum_identifier(value),
            " = ".to_string(),
            python_string_literal(value),
        ]
        .concat()
    }));
    lines.push(String::new());
    Some(lines)
}

fn alias_lines(name: &str, schema: &Schema) -> Option<Vec<String>> {
    let expression = match schema {
        Schema::TypedEnum { values, .. } => concatenate(&[
            "Literal[",
            &values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            "]",
        ]),
        Schema::Const { value, .. } => concatenate(&["Literal[", &value.to_string(), "]"]),
        Schema::OneOf(values) | Schema::AnyOf(values) => values
            .iter()
            .map(python_type)
            .collect::<Vec<_>>()
            .join(" | "),
        Schema::String { .. }
        | Schema::Integer { .. }
        | Schema::Number { .. }
        | Schema::Boolean
        | Schema::Null
        | Schema::Array(_)
        | Schema::Reference(_)
        | Schema::Nullable(_) => python_type(schema),
        _ => return None,
    };
    Some(vec![
        [name, ": TypeAlias = ", &expression].concat(),
        String::new(),
    ])
}

fn object_model_lines(name: &str, schema: &Schema, spec: &ApiIr) -> Vec<String> {
    let mut lines = vec![
        ["class ", name, "(BaseModel):"].concat(),
        "    model_config = ConfigDict(extra=\"forbid\", populate_by_name=True)".to_string(),
    ];
    lines.extend(object_fields(schema, spec).into_iter().map(render_field));
    lines.push(String::new());
    lines
}

fn render_field((property, property_schema, required): (String, Schema, bool)) -> String {
    let identifier = python_identifier(&property);
    let annotation = python_type(&property_schema);
    let field = (identifier != property).then(|| {
        concatenate(&[
            "Field(alias=",
            &serde_json::to_string(&property).unwrap_or_default(),
            ")",
        ])
    });
    let suffix = field.as_deref().map_or_else(
        || {
            if required {
                String::new()
            } else {
                " | None = None".to_string()
            }
        },
        |field| {
            if required {
                concatenate(&[" = ", field])
            } else {
                concatenate(&[" | None = ", field])
            }
        },
    );
    ["    ", &identifier, ": ", &annotation, &suffix].concat()
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
        Schema::Any => "JsonValue".to_string(),
        Schema::String {
            format: Some(format),
        } if format == "binary" => "bytes".to_string(),
        Schema::String { .. } => "str".to_string(),
        Schema::Integer { .. } => "int".to_string(),
        Schema::Number { .. } => "float".to_string(),
        Schema::Boolean => "bool".to_string(),
        Schema::Null => "None".to_string(),
        Schema::Array(item) => ["list[", python_type(item).as_str(), "]"].concat(),
        Schema::Object { .. } => "dict[str, JsonValue]".to_string(),
        Schema::Enum(_) | Schema::Reference(_) => {
            schema_model_name(schema).unwrap_or_else(|| "str".to_string())
        }
        Schema::TypedEnum { values, .. } => [
            "Literal[",
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
                .as_str(),
            "]",
        ]
        .concat(),
        Schema::Const { value, .. } => ["Literal[", &value.to_string(), "]"].concat(),
        Schema::Nullable(inner) => [python_type(inner), " | None".to_string()].concat(),
        Schema::OneOf(values) | Schema::AnyOf(values) | Schema::AllOf(values) => values
            .iter()
            .map(python_type)
            .collect::<Vec<_>>()
            .join(" | "),
    }
}

fn enum_identifier(value: &str) -> String {
    python_identifier(value).to_ascii_uppercase()
}

fn python_string_literal(value: &str) -> String {
    [
        "\"".to_string(),
        value.chars().flat_map(char::escape_default).collect(),
        "\"".to_string(),
    ]
    .concat()
}
