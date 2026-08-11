use super::identifiers::{ts_property, type_identifier};
use super::type_alias_name;
use crate::code_writer::{CodeWriter, concatenate};
use crate::{ApiIr, Operation, Schema};

pub(super) fn render_schemas(spec: &ApiIr) -> String {
    let mut declarations = Vec::new();
    for (name, schema) in &spec.schemas {
        declarations.push(concatenate(&[
            "export const ",
            name,
            "Schema = ",
            &zod_schema(schema, spec),
            ";",
        ]));
    }
    for operation in &spec.operations {
        if let Some(schema) = inline_success_schema(operation, spec) {
            let name = operation_response_name(operation);
            declarations.push(concatenate(&[
                "export const ",
                &name,
                "Schema = ",
                &zod_schema(schema, spec),
                ";",
            ]));
        }
        if let Some(schema) = inline_request_schema(operation, spec) {
            let name = operation_request_name(operation);
            declarations.push(concatenate(&[
                "export const ",
                &name,
                "Schema = ",
                &zod_schema(schema, spec),
                ";",
            ]));
        }
    }
    let mut lines = vec!["import * as z from \"zod\";".to_string(), String::new()];
    lines.extend(
        declarations
            .into_iter()
            .flat_map(|declaration| [declaration, String::new()]),
    );
    CodeWriter::from_lines(lines)
}

pub(super) fn render_types(spec: &ApiIr) -> String {
    let schema_names = schema_names(spec);
    let schemas = schema_names.join(", ");
    let mut lines = vec![
        "import type * as z from \"zod\";".to_string(),
        concatenate(&["import { ", &schemas, " } from \"./schemas.js\";"]),
        String::new(),
    ];
    lines.extend(type_alias_lines(spec));
    CodeWriter::from_lines(lines)
}

fn schema_names(spec: &ApiIr) -> Vec<String> {
    let mut names = spec
        .schemas
        .keys()
        .map(|name| concatenate(&[name, "Schema"]))
        .collect::<Vec<_>>();
    names.extend(
        spec.operations
            .iter()
            .flat_map(|operation| {
                [
                    inline_success_schema(operation, spec)
                        .is_some()
                        .then(|| operation_response_name(operation)),
                    inline_request_schema(operation, spec)
                        .is_some()
                        .then(|| operation_request_name(operation)),
                ]
            })
            .flatten()
            .map(|name| concatenate(&[&name, "Schema"])),
    );
    names.sort();
    names.dedup();
    names
}

fn type_alias_lines(spec: &ApiIr) -> Vec<String> {
    let mut lines = Vec::new();
    for name in spec.schemas.keys() {
        let alias = type_alias_name(name);
        lines.push(concatenate(&[
            "export type ",
            &alias,
            " = z.infer<typeof ",
            name,
            "Schema>;",
        ]));
    }
    for operation in &spec.operations {
        for name in [
            inline_success_schema(operation, spec)
                .is_some()
                .then(|| operation_response_name(operation)),
            inline_request_schema(operation, spec)
                .is_some()
                .then(|| operation_request_name(operation)),
        ]
        .into_iter()
        .flatten()
        {
            lines.push(concatenate(&[
                "export type ",
                &name,
                " = z.infer<typeof ",
                &name,
                "Schema>;",
            ]));
        }
    }
    lines
}

pub(super) fn schema_model_name(schema: &Schema, spec: &ApiIr) -> Option<String> {
    match schema {
        Schema::Reference(name) if spec.schemas.contains_key(name) => Some(name.clone()),
        _ => None,
    }
}

pub(super) fn inline_success_schema<'a>(
    operation: &'a Operation,
    spec: &ApiIr,
) -> Option<&'a Schema> {
    operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .and_then(|response| response.schema.as_ref())
        .filter(|schema| schema_model_name(schema, spec).is_none())
}

pub(super) fn inline_request_schema<'a>(
    operation: &'a Operation,
    spec: &ApiIr,
) -> Option<&'a Schema> {
    operation
        .request_body_details
        .as_ref()
        .and_then(|body| body.schema.as_ref())
        .filter(|schema| schema_model_name(schema, spec).is_none())
}

pub(super) fn operation_request_name(operation: &Operation) -> String {
    concatenate(&[&type_identifier(&operation.operation_id), "Request"])
}

pub(super) fn operation_response_name(operation: &Operation) -> String {
    concatenate(&[&type_identifier(&operation.operation_id), "Response"])
}

fn zod_schema(schema: &Schema, spec: &ApiIr) -> String {
    match schema {
        Schema::Any => "z.unknown()".to_string(),
        Schema::String {
            format: Some(format),
        } if format == "binary" => "z.instanceof(Uint8Array)".to_string(),
        Schema::String { .. } => "z.string()".to_string(),
        Schema::Integer { .. } => "z.number().int()".to_string(),
        Schema::Number { .. } => "z.number()".to_string(),
        Schema::Boolean => "z.boolean()".to_string(),
        Schema::Null => "z.null()".to_string(),
        Schema::Array(item) => concatenate(&["z.array(", &zod_schema(item, spec), ")"]),
        Schema::Object { .. } => zod_object_schema(schema, spec),
        Schema::Enum(values) => {
            let values = values
                .iter()
                .map(|value| serde_json::to_string(value).unwrap_or_default())
                .collect::<Vec<_>>()
                .join(", ");
            concatenate(&["z.enum([", &values, "])"])
        }
        Schema::TypedEnum { values, .. } => {
            let values = values
                .iter()
                .map(|value| concatenate(&["z.literal(", &value.to_string(), ")"]))
                .collect::<Vec<_>>()
                .join(", ");
            concatenate(&["z.union([", &values, "])"])
        }
        Schema::Const { value, .. } => concatenate(&["z.literal(", &value.to_string(), ")"]),
        Schema::Reference(name) => concatenate(&["z.lazy(() => ", name, "Schema)"]),
        Schema::Nullable(inner) => concatenate(&[&zod_schema(inner, spec), ".nullable()"]),
        Schema::OneOf(values) | Schema::AnyOf(values) => {
            let values = values
                .iter()
                .map(|value| zod_schema(value, spec))
                .collect::<Vec<_>>()
                .join(", ");
            concatenate(&["z.union([", &values, "])"])
        }
        Schema::AllOf(values) => values
            .iter()
            .map(|value| zod_schema(value, spec))
            .reduce(|left, right| concatenate(&["z.intersection(", &left, ", ", &right, ")"]))
            .unwrap_or_else(|| "z.never()".to_string()),
    }
}

fn zod_object_schema(schema: &Schema, spec: &ApiIr) -> String {
    let Schema::Object {
        properties,
        required,
        additional_properties,
    } = schema
    else {
        return "z.never()".to_string();
    };
    let fields = properties
        .iter()
        .map(|(name, value)| {
            let optional = if required.contains(name) {
                ""
            } else {
                ".optional()"
            };
            concatenate(&[
                "  ",
                &ts_property(name),
                ": ",
                &zod_schema(value, spec),
                optional,
                ",",
            ])
        })
        .collect::<Vec<_>>()
        .join("\n");
    let object = concatenate(&["z.object({\n", &fields, "\n})"]);
    match additional_properties.as_deref() {
        Some(value) => concatenate(&[&object, ".catchall(", &zod_schema(value, spec), ")"]),
        None => concatenate(&[&object, ".strict()"]),
    }
}
