use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Schema {
    String {
        format: Option<String>,
    },
    Integer {
        format: Option<String>,
    },
    Number {
        format: Option<String>,
    },
    Boolean,
    Null,
    Array(Box<Schema>),
    Object {
        properties: BTreeMap<String, Schema>,
        required: BTreeSet<String>,
        additional_properties: Option<Box<Schema>>,
    },
    Enum(Vec<String>),
    Reference(String),
    Nullable(Box<Schema>),
    OneOf(Vec<Schema>),
    AnyOf(Vec<Schema>),
    AllOf(Vec<Schema>),
}

pub(crate) fn schema_from_value(
    value: &serde_json::Value,
    path: &str,
) -> Result<Schema, crate::IngestionError> {
    let object = value
        .as_object()
        .ok_or_else(|| unsupported(path, "schema must be an object"))?;
    schema_from_object(object, path)
}

fn schema_from_object(
    object: &serde_json::Map<String, serde_json::Value>,
    path: &str,
) -> Result<Schema, crate::IngestionError> {
    match object.get("$ref").and_then(serde_json::Value::as_str) {
        Some(reference) => Ok(Schema::Reference(reference_name(reference).to_string())),
        None => schema_from_non_reference(object, path),
    }
}

fn schema_from_non_reference(
    object: &serde_json::Map<String, serde_json::Value>,
    path: &str,
) -> Result<Schema, crate::IngestionError> {
    if let Some(schema) = composition_schema(object, path)? {
        return Ok(schema);
    }
    if let Some(schema) = enum_schema(object, path)? {
        return Ok(schema);
    }
    typed_schema(object, path)
}

fn composition_schema(
    object: &serde_json::Map<String, serde_json::Value>,
    path: &str,
) -> Result<Option<Schema>, crate::IngestionError> {
    for (keyword, constructor) in [
        ("oneOf", Schema::OneOf as fn(Vec<Schema>) -> Schema),
        ("anyOf", Schema::AnyOf),
        ("allOf", Schema::AllOf),
    ] {
        let Some(values) = object.get(keyword).and_then(serde_json::Value::as_array) else {
            continue;
        };
        let schemas = values
            .iter()
            .enumerate()
            .map(|(index, value)| schema_from_value(value, &format!("{path}.{keyword}[{index}]")))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Some(constructor(schemas)));
    }
    Ok(None)
}

fn enum_schema(
    object: &serde_json::Map<String, serde_json::Value>,
    path: &str,
) -> Result<Option<Schema>, crate::IngestionError> {
    let Some(values) = object.get("enum").and_then(serde_json::Value::as_array) else {
        return Ok(None);
    };
    let values = values
        .iter()
        .map(|value| value.as_str().map(ToOwned::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| unsupported(path, "only string enums are supported"))?;
    Ok(Some(Schema::Enum(values)))
}

fn typed_schema(
    object: &serde_json::Map<String, serde_json::Value>,
    path: &str,
) -> Result<Schema, crate::IngestionError> {
    let types = schema_types(object);
    let nullable = types.iter().any(|value| value == "null");
    let type_name = types.iter().find(|value| value.as_str() != "null");
    let schema = match type_name.map(String::as_str) {
        Some("string") => scalar_schema(object, |format| Schema::String { format }),
        Some("integer") => scalar_schema(object, |format| Schema::Integer { format }),
        Some("number") => scalar_schema(object, |format| Schema::Number { format }),
        Some("boolean") => Schema::Boolean,
        Some("array") => array_schema(object, path)?,
        Some("object") => object_schema(object, path)?,
        Some("null") => Schema::Null,
        None => return Err(unsupported(path, "schema is missing a type or composition")),
        Some(other) => {
            return Err(unsupported(
                path,
                &format!("unsupported schema type {other}"),
            ));
        }
    };
    Ok(if nullable && !matches!(schema, Schema::Null) {
        Schema::Nullable(Box::new(schema))
    } else {
        schema
    })
}

fn schema_types(object: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    match object.get("type") {
        Some(serde_json::Value::String(value)) => vec![value.clone()],
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn scalar_schema(
    object: &serde_json::Map<String, serde_json::Value>,
    constructor: fn(Option<String>) -> Schema,
) -> Schema {
    constructor(
        object
            .get("format")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
    )
}

fn array_schema(
    object: &serde_json::Map<String, serde_json::Value>,
    path: &str,
) -> Result<Schema, crate::IngestionError> {
    let items = object
        .get("items")
        .ok_or_else(|| unsupported(path, "array schema is missing items"))?;
    Ok(Schema::Array(Box::new(schema_from_value(
        items,
        &format!("{path}.items"),
    )?)))
}

fn object_schema(
    object: &serde_json::Map<String, serde_json::Value>,
    path: &str,
) -> Result<Schema, crate::IngestionError> {
    let properties = object
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .map(|values| parse_properties(values, path))
        .transpose()?
        .unwrap_or_default();
    let required = object
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let additional_properties = parse_additional_properties(object, path)?;
    Ok(Schema::Object {
        properties,
        required,
        additional_properties,
    })
}

fn parse_properties(
    values: &serde_json::Map<String, serde_json::Value>,
    path: &str,
) -> Result<BTreeMap<String, Schema>, crate::IngestionError> {
    values
        .iter()
        .map(|(name, value)| {
            schema_from_value(value, &format!("{path}.properties.{name}"))
                .map(|schema| (name.clone(), schema))
        })
        .collect()
}

fn parse_additional_properties(
    object: &serde_json::Map<String, serde_json::Value>,
    path: &str,
) -> Result<Option<Box<Schema>>, crate::IngestionError> {
    match object.get("additionalProperties") {
        Some(serde_json::Value::Object(value)) => Ok(Some(Box::new(schema_from_value(
            &serde_json::Value::Object(value.clone()),
            &format!("{path}.additionalProperties"),
        )?))),
        Some(serde_json::Value::Bool(false)) => Ok(None),
        _ => Ok(None),
    }
}

fn unsupported(path: &str, detail: &str) -> crate::IngestionError {
    crate::IngestionError::UnsupportedSchema {
        path: path.to_string(),
        detail: detail.to_string(),
    }
}

fn reference_name(reference: &str) -> &str {
    reference.rsplit('/').next().unwrap_or(reference)
}
