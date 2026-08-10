use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ir::{ApiIr, HttpMethod, Operation, Parameter, ParameterLocation, Response};
use crate::schema::schema_from_value;
use crate::Schema;
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IngestionError {
    #[error("could not read OpenAPI document {path}: {message}")]
    Read { path: PathBuf, message: String },
    #[error("could not parse OpenAPI document: {0}")]
    Parse(String),
    #[error("unsupported OpenAPI version {0}; expected 3.1.x")]
    UnsupportedVersion(String),
    #[error("path {path} has no operation")]
    EmptyPath { path: String },
    #[error("operation at {method} {path} has no operationId")]
    MissingOperationId { method: String, path: String },
    #[error("duplicate operationId {operation_id}")]
    DuplicateOperationId { operation_id: String },
    #[error("path parameter {parameter} in {path} must be declared as a required path parameter")]
    InvalidPathParameter { parameter: String, path: String },
    #[error("unsupported HTTP method {method} at {path}")]
    UnsupportedMethod { method: String, path: String },
    #[error("unsupported schema at {path}: {detail}")]
    UnsupportedSchema { path: String, detail: String },
}

#[derive(Debug, Deserialize)]
struct RawDocument {
    openapi: String,
    info: RawInfo,
    paths: BTreeMap<String, RawPathItem>,
    #[serde(default)]
    components: RawComponents,
}

#[derive(Debug, Default, Deserialize)]
struct RawComponents {
    #[serde(default)]
    schemas: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawInfo {
    title: String,
    version: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawPathItem {
    #[serde(default)]
    parameters: Vec<RawParameter>,
    delete: Option<RawOperation>,
    get: Option<RawOperation>,
    head: Option<RawOperation>,
    options: Option<RawOperation>,
    patch: Option<RawOperation>,
    post: Option<RawOperation>,
    put: Option<RawOperation>,
    trace: Option<RawOperation>,
}

#[derive(Debug, Deserialize)]
struct RawOperation {
    #[serde(rename = "operationId")]
    operation_id: Option<String>,
    #[serde(default)]
    parameters: Vec<RawParameter>,
    #[serde(rename = "requestBody")]
    request_body: Option<serde_json::Value>,
    #[serde(default)]
    responses: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawParameter {
    Inline {
        name: String,
        #[serde(rename = "in")]
        location: String,
        #[serde(default)]
        required: bool,
    },
    Reference {
        #[serde(rename = "$ref")]
        reference: String,
    },
}

impl ApiIr {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, IngestionError> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|error| IngestionError::Read {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Self::parse(&source)
    }

    pub fn parse(source: &str) -> Result<Self, IngestionError> {
        let raw_value: serde_json::Value = yaml_serde::from_str(source)
            .map_err(|error| IngestionError::Parse(error.to_string()))?;
        let raw: RawDocument = serde_json::from_value(raw_value.clone())
            .map_err(|error| IngestionError::Parse(error.to_string()))?;

        if !raw.openapi.starts_with("3.1.") {
            return Err(IngestionError::UnsupportedVersion(raw.openapi));
        }

        normalize_document(raw)
    }
}

fn normalize_document(raw: RawDocument) -> Result<ApiIr, IngestionError> {
    let mut operations = Vec::new();
    let mut operation_ids = BTreeSet::new();
    let mut references = BTreeSet::new();
    let mut schemas = BTreeMap::new();

    for (name, value) in &raw.components.schemas {
        schemas.insert(
            name.clone(),
            schema_from_value(value, &format!("components.schemas.{name}"))?,
        );
    }

    for (path, item) in raw.paths {
        operations.extend(normalize_path(
            &path,
            item,
            &mut operation_ids,
            &mut references,
        )?);
    }
    operations.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.method.cmp(&right.method))
    });

    Ok(ApiIr {
        openapi_version: raw.openapi,
        title: raw.info.title,
        version: raw.info.version,
        operations,
        schemas,
        references: references.into_iter().collect(),
    })
}

fn normalize_path(
    path: &str,
    item: RawPathItem,
    operation_ids: &mut BTreeSet<String>,
    references: &mut BTreeSet<String>,
) -> Result<Vec<Operation>, IngestionError> {
    let methods = [
        ("delete", HttpMethod::Delete, item.delete),
        ("get", HttpMethod::Get, item.get),
        ("head", HttpMethod::Head, item.head),
        ("options", HttpMethod::Options, item.options),
        ("patch", HttpMethod::Patch, item.patch),
        ("post", HttpMethod::Post, item.post),
        ("put", HttpMethod::Put, item.put),
        ("trace", HttpMethod::Trace, item.trace),
    ];
    let mut operations = Vec::new();
    for (method_name, method, operation) in methods {
        let Some(operation) = operation else {
            continue;
        };
        let context = NormalizationContext {
            path,
            method_name,
            path_parameters: &item.parameters,
            operation_ids,
            references,
        };
        operations.push(normalize_operation(operation, method, context)?);
    }
    if operations.is_empty() {
        return Err(IngestionError::EmptyPath {
            path: path.to_string(),
        });
    }
    Ok(operations)
}

struct NormalizationContext<'a> {
    path: &'a str,
    method_name: &'a str,
    path_parameters: &'a [RawParameter],
    operation_ids: &'a mut BTreeSet<String>,
    references: &'a mut BTreeSet<String>,
}

fn normalize_operation(
    operation: RawOperation,
    method: HttpMethod,
    mut context: NormalizationContext<'_>,
) -> Result<Operation, IngestionError> {
    let operation_id = operation_id(&operation, &mut context)?;
    let parameters = normalize_parameters(
        context.path,
        context.path_parameters,
        &operation.parameters,
        context.references,
    )?;
    validate_path_parameters(context.path, &parameters, context.references)?;
    let (request_body_schema, responses) = parse_operation_schemas(&operation, context.path)?;
    Ok(Operation {
        operation_id,
        method,
        path: context.path.to_string(),
        parameters,
        request_body: operation.request_body.is_some(),
        response_statuses: operation.responses.into_keys().collect(),
        request_body_schema,
        responses,
    })
}

fn operation_id(
    operation: &RawOperation,
    context: &mut NormalizationContext<'_>,
) -> Result<String, IngestionError> {
    let operation_id =
        operation
            .operation_id
            .clone()
            .ok_or_else(|| IngestionError::MissingOperationId {
                method: context.method_name.to_string(),
                path: context.path.to_string(),
            })?;
    if context.operation_ids.insert(operation_id.clone()) {
        Ok(operation_id)
    } else {
        Err(IngestionError::DuplicateOperationId { operation_id })
    }
}

fn parse_operation_schemas(
    operation: &RawOperation,
    path: &str,
) -> Result<(Option<Schema>, Vec<Response>), IngestionError> {
    let request_body_schema = operation
        .request_body
        .as_ref()
        .and_then(|body| body.get("content"))
        .and_then(first_content_schema)
        .map(|schema| schema_from_value(schema, &format!("{path}.requestBody")))
        .transpose()?;
    let responses = operation
        .responses
        .iter()
        .map(|(status, response)| {
            let schema = response
                .get("content")
                .and_then(first_content_schema)
                .map(|schema| schema_from_value(schema, &format!("{path} responses {status}")))
                .transpose()?;
            Ok(Response {
                status: status.clone(),
                schema,
            })
        })
        .collect::<Result<Vec<_>, IngestionError>>()?;
    Ok((request_body_schema, responses))
}

fn first_content_schema(content: &serde_json::Value) -> Option<&serde_json::Value> {
    content
        .as_object()?
        .iter()
        .min_by_key(|(media_type, _)| *media_type)
        .and_then(|(_, media)| media.get("schema"))
}

fn normalize_parameters(
    path: &str,
    path_parameters: &[RawParameter],
    operation_parameters: &[RawParameter],
    references: &mut BTreeSet<String>,
) -> Result<Vec<Parameter>, IngestionError> {
    let mut parameters = Vec::new();
    for parameter in path_parameters.iter().chain(operation_parameters) {
        match parameter {
            RawParameter::Inline {
                name,
                location,
                required,
            } => parameters.push(Parameter {
                name: name.clone(),
                location: parse_parameter_location(location, path)?,
                required: *required,
            }),
            RawParameter::Reference { reference } => {
                references.insert(reference.clone());
            }
        }
    }
    parameters.sort_by_key(|parameter| parameter_order(path, parameter));
    Ok(parameters)
}

fn parameter_order(path: &str, parameter: &Parameter) -> (u8, usize, String) {
    if parameter.location == ParameterLocation::Path {
        let position = path_parameters(path)
            .iter()
            .position(|name| name == &parameter.name)
            .unwrap_or(usize::MAX);
        return (0, position, parameter.name.clone());
    }
    let location = match parameter.location {
        ParameterLocation::Query => 1,
        ParameterLocation::Header => 2,
        ParameterLocation::Cookie => 3,
        ParameterLocation::Path => unreachable!("path parameters return above"),
    };
    (location, 0, parameter.name.clone())
}

fn validate_path_parameters(
    path: &str,
    parameters: &[Parameter],
    references: &BTreeSet<String>,
) -> Result<(), IngestionError> {
    for parameter_name in path_parameters(path) {
        let declared = parameters.iter().any(|parameter| {
            parameter.name == parameter_name
                && parameter.location == ParameterLocation::Path
                && parameter.required
        });
        let referenced = references
            .iter()
            .any(|reference| reference.contains("parameters"));
        if !declared && !referenced {
            return Err(IngestionError::InvalidPathParameter {
                parameter: parameter_name,
                path: path.to_string(),
            });
        }
    }
    Ok(())
}

fn parse_parameter_location(
    location: &str,
    path: &str,
) -> Result<ParameterLocation, IngestionError> {
    match location {
        "query" => Ok(ParameterLocation::Query),
        "header" => Ok(ParameterLocation::Header),
        "path" => Ok(ParameterLocation::Path),
        "cookie" => Ok(ParameterLocation::Cookie),
        _ => Err(IngestionError::Parse(format!(
            "unsupported parameter location {location} at {path}"
        ))),
    }
}

fn path_parameters(path: &str) -> Vec<String> {
    path.split('{')
        .skip(1)
        .filter_map(|segment| segment.split('}').next())
        .map(ToOwned::to_owned)
        .collect()
}
