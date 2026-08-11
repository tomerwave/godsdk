use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ReferencePolicy;
use crate::Schema;
use crate::ingestion_contracts::normalize_operation_contract;
use crate::ingestion_refs::resolve_external_references;
use crate::ingestion_security::{
    RawSecurityScheme, normalize_security_requirements, normalize_security_schemes,
};
use crate::ir::{
    ApiIr, HttpMethod, Operation, Parameter, ParameterLocation, RequestBody, Response,
    SecurityScheme,
};
use crate::schema::schema_from_value;

#[path = "ingestion/parameter_serialization.rs"]
mod parameter_serialization;
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IngestionError {
    #[error("could not read OpenAPI document {path}: {message}")]
    Read { path: PathBuf, message: String },
    #[error("could not parse OpenAPI document: {0}")]
    Parse(String),
    #[error("unsupported OpenAPI version {0}; expected 3.0.x or 3.1.x")]
    UnsupportedVersion(String),
    #[error("could not resolve external reference {reference}: {message}")]
    ExternalReference { reference: String, message: String },
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
    #[error("unsupported security scheme {name}: {detail}")]
    UnsupportedSecurityScheme { name: String, detail: String },
    #[error("security requirement references unknown scheme {name}")]
    UnknownSecurityScheme { name: String },
    #[error("security requirement for {scheme} references unknown scope {scope}")]
    UnknownSecurityScope { scheme: String, scope: String },
    #[error("unsupported parameter serialization style {style} for {location} parameter at {path}")]
    UnsupportedParameterStyle {
        style: String,
        location: String,
        path: String,
    },
}

#[derive(Debug, Deserialize)]
struct RawDocument {
    openapi: String,
    info: RawInfo,
    paths: BTreeMap<String, RawPathItem>,
    #[serde(default)]
    components: RawComponents,
    #[serde(default)]
    security: Option<Vec<BTreeMap<String, Vec<String>>>>,
}

#[derive(Debug, Default, Deserialize)]
struct RawComponents {
    #[serde(default)]
    schemas: BTreeMap<String, serde_json::Value>,
    #[serde(rename = "securitySchemes", default)]
    security_schemes: BTreeMap<String, RawSecurityScheme>,
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
    #[serde(default)]
    security: Option<Vec<BTreeMap<String, Vec<String>>>>,
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
        #[serde(default)]
        style: Option<String>,
        #[serde(default)]
        explode: Option<bool>,
        #[serde(default)]
        schema: Option<serde_json::Value>,
    },
    Reference {
        #[serde(rename = "$ref")]
        reference: String,
    },
}

impl ApiIr {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, IngestionError> {
        Self::from_path_with_policy(path, &ReferencePolicy::default())
    }

    pub fn from_path_with_policy(
        path: impl AsRef<Path>,
        reference_policy: &ReferencePolicy,
    ) -> Result<Self, IngestionError> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|error| IngestionError::Read {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Self::parse_source(&source, path.parent(), reference_policy)
    }

    pub fn parse(source: &str) -> Result<Self, IngestionError> {
        Self::parse_with_policy(source, None, &ReferencePolicy::default())
    }

    pub fn parse_with_policy(
        source: &str,
        base_directory: Option<&Path>,
        reference_policy: &ReferencePolicy,
    ) -> Result<Self, IngestionError> {
        Self::parse_source(source, base_directory, reference_policy)
    }

    fn parse_source(
        source: &str,
        base_directory: Option<&Path>,
        reference_policy: &ReferencePolicy,
    ) -> Result<Self, IngestionError> {
        let (raw_value, references) = parse_document_value(source)?;
        let raw_value = resolve_document_value(raw_value, base_directory, reference_policy)?;
        let raw = parse_raw_document(raw_value)?;
        let mut spec = normalize_raw_document(raw)?;
        spec.references = references.into_iter().collect();
        Ok(spec)
    }
}

fn parse_raw_document(value: serde_json::Value) -> Result<RawDocument, IngestionError> {
    serde_json::from_value(value).map_err(|error| IngestionError::Parse(error.to_string()))
}

fn normalize_raw_document(raw: RawDocument) -> Result<ApiIr, IngestionError> {
    validate_openapi_version(&raw.openapi)?;
    normalize_document(raw)
}

fn validate_openapi_version(version: &str) -> Result<(), IngestionError> {
    if version.starts_with("3.0.") || version.starts_with("3.1.") {
        Ok(())
    } else {
        Err(IngestionError::UnsupportedVersion(version.to_string()))
    }
}

fn resolve_document_value(
    mut value: serde_json::Value,
    base_directory: Option<&Path>,
    reference_policy: &ReferencePolicy,
) -> Result<serde_json::Value, IngestionError> {
    let base_directory = base_directory.unwrap_or_else(|| Path::new("."));
    resolve_external_references(&mut value, base_directory, reference_policy)?;
    Ok(value)
}

fn parse_document_value(
    source: &str,
) -> Result<(serde_json::Value, BTreeSet<String>), IngestionError> {
    let value: serde_json::Value =
        yaml_serde::from_str(source).map_err(|error| IngestionError::Parse(error.to_string()))?;
    let references = crate::ingestion_refs::external_references(&value);
    Ok((value, references))
}

fn normalize_document(raw: RawDocument) -> Result<ApiIr, IngestionError> {
    let RawDocument {
        openapi,
        info,
        paths,
        components,
        security,
    } = raw;
    let normalized = normalize_components(components)?;
    let schemas = normalized.schemas;
    let security_schemes = normalized.security_schemes;
    let security = security
        .as_deref()
        .map(|requirements| normalize_security_requirements(requirements, &security_schemes))
        .transpose()?;
    let (mut operations, references) = normalize_operations(paths, &security_schemes)?;
    operations.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.method.cmp(&right.method))
    });

    Ok(ApiIr {
        openapi_version: openapi,
        title: info.title,
        version: info.version,
        operations,
        schemas,
        security,
        security_schemes,
        references,
    })
}

struct NormalizedComponents {
    schemas: BTreeMap<String, Schema>,
    security_schemes: BTreeMap<String, SecurityScheme>,
}

fn normalize_components(components: RawComponents) -> Result<NormalizedComponents, IngestionError> {
    let schemas = components
        .schemas
        .iter()
        .map(|(name, value)| {
            schema_from_value(value, &format!("components.schemas.{name}"))
                .map(|schema| (name.clone(), schema))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let security_schemes = normalize_security_schemes(components.security_schemes)?;
    Ok(NormalizedComponents {
        schemas,
        security_schemes,
    })
}

fn normalize_operations(
    paths: BTreeMap<String, RawPathItem>,
    security_schemes: &BTreeMap<String, SecurityScheme>,
) -> Result<(Vec<Operation>, Vec<String>), IngestionError> {
    let mut state = NormalizationState {
        operation_ids: BTreeSet::new(),
        references: BTreeSet::new(),
        security_schemes,
    };
    let operations = paths
        .into_iter()
        .map(|(path, item)| normalize_path(&path, item, &mut state))
        .collect::<Result<Vec<_>, _>>()
        .map(|groups| groups.into_iter().flatten().collect())?;
    let references = state.references.into_iter().collect();
    Ok((operations, references))
}

fn normalize_path<'path, 'scheme, 'state>(
    path: &'path str,
    item: RawPathItem,
    state: &'state mut NormalizationState<'scheme>,
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
            operation_ids: &mut state.operation_ids,
            references: &mut state.references,
            security_schemes: state.security_schemes,
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

struct NormalizationState<'a> {
    operation_ids: BTreeSet<String>,
    references: BTreeSet<String>,
    security_schemes: &'a BTreeMap<String, SecurityScheme>,
}

struct NormalizationContext<'path, 'item, 'security> {
    path: &'path str,
    method_name: &'path str,
    path_parameters: &'item [RawParameter],
    operation_ids: &'item mut BTreeSet<String>,
    references: &'item mut BTreeSet<String>,
    security_schemes: &'security BTreeMap<String, SecurityScheme>,
}

fn normalize_operation<'path, 'item, 'security>(
    operation: RawOperation,
    method: HttpMethod,
    context: NormalizationContext<'path, 'item, 'security>,
) -> Result<Operation, IngestionError> {
    let operation_id = operation_id(
        &operation,
        context.operation_ids,
        context.path,
        context.method_name,
    )?;
    let parameters = normalize_parameters(
        context.path,
        context.path_parameters,
        &operation.parameters,
        context.references,
    )?;
    validate_path_parameters(context.path, &parameters, context.references)?;
    let data = normalize_operation_data(&operation, context.path, context.security_schemes)?;
    Ok(Operation {
        operation_id,
        method,
        path: context.path.to_string(),
        parameters,
        request_body: data.request_body,
        response_statuses: data.response_statuses,
        request_body_schema: data.request_body_schema,
        request_body_details: data.request_body_details,
        responses: data.responses,
        security: data.security,
    })
}

struct NormalizedOperationData {
    request_body: bool,
    request_body_schema: Option<Schema>,
    request_body_details: Option<RequestBody>,
    response_statuses: Vec<String>,
    responses: Vec<Response>,
    security: Option<Vec<crate::ir::SecurityRequirement>>,
}

fn normalize_operation_data(
    operation: &RawOperation,
    path: &str,
    security_schemes: &BTreeMap<String, SecurityScheme>,
) -> Result<NormalizedOperationData, IngestionError> {
    let (request_body_details, responses) =
        normalize_operation_contract(operation.request_body.as_ref(), &operation.responses, path)?;
    let security = operation
        .security
        .as_ref()
        .map(|requirements| normalize_security_requirements(requirements, security_schemes))
        .transpose()?;
    Ok(NormalizedOperationData {
        request_body: request_body_details.is_some(),
        response_statuses: operation.responses.keys().cloned().collect(),
        request_body_schema: request_body_details
            .as_ref()
            .and_then(|body| body.schema.clone()),
        request_body_details,
        responses,
        security,
    })
}

fn operation_id(
    operation: &RawOperation,
    operation_ids: &mut BTreeSet<String>,
    path: &str,
    method_name: &str,
) -> Result<String, IngestionError> {
    let operation_id = operation
        .operation_id
        .clone()
        .unwrap_or_else(|| inferred_operation_id(method_name, path));
    if operation_ids.insert(operation_id.clone()) {
        Ok(operation_id)
    } else {
        Err(IngestionError::DuplicateOperationId { operation_id })
    }
}

fn inferred_operation_id(method_name: &str, path: &str) -> String {
    let mut id = String::with_capacity(method_name.len() + path.len());
    id.push_str(method_name);
    path.split('/')
        .filter_map(operation_segment)
        .for_each(|segment| {
            append_operation_segment(&mut id, segment);
        });
    if id == method_name {
        id.push_str("Root");
    }
    id
}

fn operation_segment(segment: &str) -> Option<&str> {
    (!segment.is_empty()).then(|| segment.trim_matches(['{', '}']))
}

fn append_operation_segment(id: &mut String, segment: &str) {
    let mut uppercase = true;
    for character in segment.chars() {
        uppercase = append_operation_character(id, character, uppercase);
    }
}

fn append_operation_character(id: &mut String, character: char, uppercase: bool) -> bool {
    if !character.is_ascii_alphanumeric() {
        return true;
    }
    id.push(if uppercase {
        character.to_ascii_uppercase()
    } else {
        character
    });
    false
}

fn normalize_parameters(
    path: &str,
    path_parameters: &[RawParameter],
    operation_parameters: &[RawParameter],
    references: &mut BTreeSet<String>,
) -> Result<Vec<Parameter>, IngestionError> {
    let mut parameters = Vec::new();
    for parameter in path_parameters.iter().chain(operation_parameters) {
        if let Some(parameter) = normalize_parameter(parameter, path, references)? {
            parameters.push(parameter);
        }
    }
    parameters.sort_by_key(|parameter| parameter_serialization::order(path, parameter));
    Ok(parameters)
}

fn normalize_parameter(
    parameter: &RawParameter,
    path: &str,
    references: &mut BTreeSet<String>,
) -> Result<Option<Parameter>, IngestionError> {
    match parameter {
        RawParameter::Inline {
            name,
            location,
            required,
            style,
            explode,
            schema,
        } => {
            let location_kind = parameter_serialization::parse_location(location, path)?;
            let serialization =
                parameter_serialization::normalize(style.as_deref(), *explode, location, path)?;
            let schema = schema
                .as_ref()
                .map(|value| schema_from_value(value, &format!("{path}.parameter.{name}")))
                .transpose()?
                .unwrap_or(Schema::String { format: None });
            Ok(Some(Parameter {
                name: name.clone(),
                location: location_kind,
                required: *required,
                serialization,
                schema,
            }))
        }
        RawParameter::Reference { reference } => {
            references.insert(reference.clone());
            Ok(None)
        }
    }
}

fn validate_path_parameters(
    path: &str,
    parameters: &[Parameter],
    references: &BTreeSet<String>,
) -> Result<(), IngestionError> {
    for parameter_name in parameter_serialization::path_parameters(path) {
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
