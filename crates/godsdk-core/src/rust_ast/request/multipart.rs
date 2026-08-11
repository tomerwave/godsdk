use super::super::literal;
use crate::{ApiIr, Schema};

pub(super) fn binary_fields(schema: Option<&Schema>, spec: &ApiIr) -> Vec<syn::LitStr> {
    let Some(schema) = schema else {
        return Vec::new();
    };
    let schema = match schema {
        Schema::Reference(name) => spec.schemas.get(name).unwrap_or(schema),
        schema => schema,
    };
    let Schema::Object { properties, .. } = schema else {
        return Vec::new();
    };
    properties
        .iter()
        .filter(|(_, schema)| is_binary_schema(schema, spec))
        .map(|(name, _)| literal(name))
        .collect()
}

fn is_binary_schema(schema: &Schema, spec: &ApiIr) -> bool {
    match schema {
        Schema::String {
            format: Some(format),
        } => format == "binary",
        Schema::Array(item) => is_binary_schema(item, spec),
        Schema::Reference(name) => spec
            .schemas
            .get(name)
            .is_some_and(|schema| is_binary_schema(schema, spec)),
        Schema::Nullable(inner) => is_binary_schema(inner, spec),
        _ => false,
    }
}
