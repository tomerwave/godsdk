use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::Digest;

use crate::IngestionError;

pub(crate) fn external_references(value: &Value) -> BTreeSet<String> {
    let mut references = BTreeSet::new();
    collect_external_references(value, &mut references);
    references
}

pub(crate) fn resolve_external_references(
    root: &mut Value,
    base_directory: &Path,
    policy: &crate::ReferencePolicy,
) -> Result<(), IngestionError> {
    let base = ReferenceBase::Local(base_directory.to_path_buf());
    resolve_external_parameters(root, &base, &mut BTreeSet::new(), policy)?;
    import_external_schemas(root, &base, policy)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReferenceBase {
    Local(PathBuf),
    Remote(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReferenceTarget {
    Local(PathBuf),
    Remote(String),
}

fn collect_external_references(value: &Value, references: &mut BTreeSet<String>) {
    match value {
        Value::Array(values) => collect_from_array(values, references),
        Value::Object(object) => collect_from_object(object, references),
        _ => {}
    }
}

fn collect_from_array(values: &[Value], references: &mut BTreeSet<String>) {
    values
        .iter()
        .for_each(|value| collect_external_references(value, references));
}

fn collect_from_object(object: &serde_json::Map<String, Value>, references: &mut BTreeSet<String>) {
    if let Some(reference) = object.get("$ref").and_then(Value::as_str)
        && is_external_reference(reference)
    {
        references.insert(reference.to_string());
    }
    object
        .values()
        .for_each(|value| collect_external_references(value, references));
}

fn resolve_external_parameters(
    value: &mut Value,
    base: &ReferenceBase,
    visited: &mut BTreeSet<(String, String)>,
    policy: &crate::ReferencePolicy,
) -> Result<(), IngestionError> {
    if let Some(target) = external_parameter_target(value, base, visited, policy)? {
        *value = target;
    } else {
        recurse_external_parameters(value, base, visited, policy)?;
    }
    Ok(())
}

fn external_parameter_target(
    value: &Value,
    base: &ReferenceBase,
    visited: &mut BTreeSet<(String, String)>,
    policy: &crate::ReferencePolicy,
) -> Result<Option<Value>, IngestionError> {
    let Some(reference) = external_parameter_reference(value) else {
        return Ok(None);
    };
    let (target, fragment) = split_reference(reference, base, policy)?;
    let target = resolve_external_parameter_target(
        &target,
        &fragment,
        reference,
        ResolutionContext { visited, policy },
    )?;
    Ok(Some(target))
}

struct ResolutionContext<'a> {
    visited: &'a mut BTreeSet<(String, String)>,
    policy: &'a crate::ReferencePolicy,
}

fn resolve_external_parameter_target(
    target: &ReferenceTarget,
    fragment: &str,
    reference: &str,
    context: ResolutionContext<'_>,
) -> Result<Value, IngestionError> {
    ensure_unvisited(context.visited, target, fragment, reference)?;
    let next_base = target.base();
    let mut value = load_target(target, fragment, reference, context.policy)?;
    resolve_external_parameters(&mut value, &next_base, context.visited, context.policy)?;
    Ok(value)
}

impl ReferenceTarget {
    fn base(&self) -> ReferenceBase {
        match self {
            Self::Local(path) => ReferenceBase::Local(
                path.parent()
                    .map_or_else(|| path.to_path_buf(), Path::to_path_buf),
            ),
            Self::Remote(url) => ReferenceBase::Remote(url.clone()),
        }
    }

    fn key(&self) -> String {
        match self {
            Self::Local(path) => path.to_string_lossy().into_owned(),
            Self::Remote(url) => url.clone(),
        }
    }
}

fn ensure_unvisited(
    visited: &mut BTreeSet<(String, String)>,
    target: &ReferenceTarget,
    fragment: &str,
    reference: &str,
) -> Result<(), IngestionError> {
    if visited.insert((target.key(), fragment.to_string())) {
        Ok(())
    } else {
        Err(reference_error(reference, "reference cycle detected"))
    }
}

fn external_parameter_reference(value: &Value) -> Option<&str> {
    let reference = value
        .as_object()
        .and_then(|object| object.get("$ref"))
        .and_then(Value::as_str)?;
    (is_external_reference(reference)
        && reference_fragment(reference).starts_with("/components/parameters/"))
    .then_some(reference)
}

fn recurse_external_parameters(
    value: &mut Value,
    base: &ReferenceBase,
    visited: &mut BTreeSet<(String, String)>,
    policy: &crate::ReferencePolicy,
) -> Result<(), IngestionError> {
    match value {
        Value::Array(values) => values
            .iter_mut()
            .try_for_each(|value| resolve_external_parameters(value, base, visited, policy)),
        Value::Object(object) => object
            .values_mut()
            .try_for_each(|value| resolve_external_parameters(value, base, visited, policy)),
        _ => Ok(()),
    }
}

fn import_external_schemas(
    root: &mut Value,
    base: &ReferenceBase,
    policy: &crate::ReferencePolicy,
) -> Result<(), IngestionError> {
    let imported = collect_imported_schemas(root, base, policy)?;
    let schemas = schemas_object(root)?;
    for (name, target) in imported {
        schemas.entry(name).or_insert(target);
    }
    Ok(())
}

fn collect_imported_schemas(
    root: &Value,
    base: &ReferenceBase,
    policy: &crate::ReferencePolicy,
) -> Result<BTreeMap<String, Value>, IngestionError> {
    let mut imported = BTreeMap::new();
    let mut queue = external_references(root)
        .into_iter()
        .filter(|reference| reference_fragment(reference).starts_with("/components/schemas/"))
        .map(|reference| (reference, base.clone()))
        .collect::<Vec<_>>();
    while let Some((reference, current_base)) = queue.pop() {
        let schema = load_external_schema(&reference, &current_base, policy)?;
        if imported.contains_key(&schema.name) {
            continue;
        }
        queue.extend(
            schema
                .references
                .iter()
                .cloned()
                .map(|nested| (nested, schema.base.clone())),
        );
        imported.insert(schema.name, schema.value);
    }
    Ok(imported)
}

struct ImportedSchema {
    name: String,
    value: Value,
    references: BTreeSet<String>,
    base: ReferenceBase,
}

fn load_external_schema(
    reference: &str,
    base: &ReferenceBase,
    policy: &crate::ReferencePolicy,
) -> Result<ImportedSchema, IngestionError> {
    let (target, fragment) = split_reference(reference, base, policy)?;
    let name = schema_name(&fragment, reference)?;
    let value = load_target(&target, &fragment, reference, policy)?;
    let references = external_references(&value)
        .into_iter()
        .filter(|nested| reference_fragment(nested).starts_with("/components/schemas/"))
        .collect();
    Ok(ImportedSchema {
        name,
        value,
        references,
        base: target.base(),
    })
}

fn schemas_object(root: &mut Value) -> Result<&mut serde_json::Map<String, Value>, IngestionError> {
    let document = root
        .as_object_mut()
        .ok_or_else(|| IngestionError::Parse("OpenAPI document must be an object".to_string()))?;
    let components = document
        .entry("components")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let components = components
        .as_object_mut()
        .ok_or_else(|| IngestionError::Parse("components must be an object".to_string()))?;
    let schemas = components
        .entry("schemas")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    schemas
        .as_object_mut()
        .ok_or_else(|| IngestionError::Parse("components.schemas must be an object".to_string()))
}

fn load_target(
    target: &ReferenceTarget,
    fragment: &str,
    reference: &str,
    policy: &crate::ReferencePolicy,
) -> Result<Value, IngestionError> {
    let source = match target {
        ReferenceTarget::Local(path) => fs::read_to_string(path).map_err(|error| {
            reference_error(reference, &format!("could not read file: {error}"))
        })?,
        ReferenceTarget::Remote(url) => fetch_remote_document(url, reference, policy)?,
    };
    let document: Value = yaml_serde::from_str(&source)
        .map_err(|error| reference_error(reference, &format!("could not parse file: {error}")))?;
    fragment_value(&document, fragment)
        .cloned()
        .ok_or_else(|| reference_error(reference, "fragment does not exist"))
}

fn fragment_value<'a>(document: &'a Value, fragment: &str) -> Option<&'a Value> {
    fragment
        .strip_prefix('/')?
        .split('/')
        .try_fold(document, |value, segment| {
            value.get(segment.replace("~1", "/").replace("~0", "~"))
        })
}

fn split_reference(
    reference: &str,
    base: &ReferenceBase,
    policy: &crate::ReferencePolicy,
) -> Result<(ReferenceTarget, String), IngestionError> {
    let (file, fragment) = reference.split_once('#').unwrap_or((reference, ""));
    let target = reference_target(file, reference, base, policy)?;
    Ok((target, reference_fragment_value(fragment)))
}

fn reference_target(
    file: &str,
    reference: &str,
    base: &ReferenceBase,
    policy: &crate::ReferencePolicy,
) -> Result<ReferenceTarget, IngestionError> {
    if file.contains("://") {
        validate_remote_reference(file, reference, policy)?;
        return Ok(ReferenceTarget::Remote(file.to_string()));
    }
    match base {
        ReferenceBase::Local(base_directory) => {
            local_reference_target(file, reference, base_directory)
        }
        ReferenceBase::Remote(base_url) => {
            remote_reference_target(file, reference, base_url, policy)
        }
    }
}

fn local_reference_target(
    file: &str,
    reference: &str,
    base_directory: &Path,
) -> Result<ReferenceTarget, IngestionError> {
    let relative = Path::new(file);
    if relative.is_absolute() || !is_safe_relative_path(relative) {
        return Err(reference_error(
            reference,
            "only relative local files are supported",
        ));
    }
    Ok(ReferenceTarget::Local(base_directory.join(relative)))
}

fn remote_reference_target(
    file: &str,
    reference: &str,
    base_url: &str,
    policy: &crate::ReferencePolicy,
) -> Result<ReferenceTarget, IngestionError> {
    let base_url = reqwest::Url::parse(base_url).map_err(|error| {
        reference_error(reference, &format!("invalid remote base URL: {error}"))
    })?;
    let url = base_url
        .join(file)
        .map_err(|error| reference_error(reference, &format!("invalid remote reference: {error}")))?
        .to_string();
    validate_remote_reference(&url, reference, policy)?;
    Ok(ReferenceTarget::Remote(url))
}

fn reference_fragment_value(fragment: &str) -> String {
    if fragment.is_empty() {
        "/".to_string()
    } else {
        fragment.to_string()
    }
}

fn validate_remote_reference(
    url: &str,
    reference: &str,
    policy: &crate::ReferencePolicy,
) -> Result<(), IngestionError> {
    let parsed = parse_remote_url(url, reference)?;
    let host = remote_host(&parsed, reference)?;
    validate_remote_policy(
        RemoteReference {
            parsed: &parsed,
            host,
            url,
            reference,
        },
        policy,
    )
}

struct RemoteReference<'a> {
    parsed: &'a reqwest::Url,
    host: &'a str,
    url: &'a str,
    reference: &'a str,
}

fn validate_remote_policy(
    remote: RemoteReference<'_>,
    policy: &crate::ReferencePolicy,
) -> Result<(), IngestionError> {
    validate_remote_scheme(remote.parsed, remote.host, remote.reference)?;
    validate_remote_host(policy, remote.host, remote.reference)?;
    validate_remote_pin(policy, remote.url, remote.reference)?;
    Ok(())
}

fn remote_host<'a>(url: &'a reqwest::Url, reference: &str) -> Result<&'a str, IngestionError> {
    url.host_str()
        .ok_or_else(|| reference_error(reference, "remote URL has no host"))
}

fn parse_remote_url(url: &str, reference: &str) -> Result<reqwest::Url, IngestionError> {
    reqwest::Url::parse(url)
        .map_err(|error| reference_error(reference, &format!("invalid remote URL: {error}")))
}

fn validate_remote_scheme(
    url: &reqwest::Url,
    host: &str,
    reference: &str,
) -> Result<(), IngestionError> {
    let loopback = host
        .parse::<std::net::IpAddr>()
        .is_ok_and(|address| address.is_loopback());
    if url.scheme() == "https" || loopback {
        Ok(())
    } else {
        Err(reference_error(
            reference,
            "remote references must use HTTPS",
        ))
    }
}

fn validate_remote_host(
    policy: &crate::ReferencePolicy,
    host: &str,
    reference: &str,
) -> Result<(), IngestionError> {
    if policy.allows_host(host) {
        Ok(())
    } else {
        Err(reference_error(
            reference,
            &format!("remote host {host} is not allowlisted"),
        ))
    }
}

fn validate_remote_pin(
    policy: &crate::ReferencePolicy,
    url: &str,
    reference: &str,
) -> Result<(), IngestionError> {
    if policy.checksum_for(url).is_some() {
        Ok(())
    } else {
        Err(reference_error(
            reference,
            "remote document requires a SHA-256 pin",
        ))
    }
}

fn fetch_remote_document(
    url: &str,
    reference: &str,
    policy: &crate::ReferencePolicy,
) -> Result<String, IngestionError> {
    validate_remote_reference(url, reference, policy)?;
    let body = fetch_remote_bytes(url, reference)?;
    verify_remote_checksum(url, reference, policy, &body)?;
    String::from_utf8(body).map_err(|error| {
        reference_error(reference, &format!("remote document is not UTF-8: {error}"))
    })
}

fn fetch_remote_bytes(url: &str, reference: &str) -> Result<Vec<u8>, IngestionError> {
    let response = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| {
            reference_error(reference, &format!("could not create HTTP client: {error}"))
        })?
        .get(url)
        .send()
        .map_err(|error| {
            reference_error(
                reference,
                &format!("could not fetch remote document: {error}"),
            )
        })?;
    if !response.status().is_success() {
        return Err(reference_error(
            reference,
            &format!("remote document returned HTTP {}", response.status()),
        ));
    }
    response.bytes().map(|body| body.to_vec()).map_err(|error| {
        reference_error(
            reference,
            &format!("could not read remote document: {error}"),
        )
    })
}

fn verify_remote_checksum(
    url: &str,
    reference: &str,
    policy: &crate::ReferencePolicy,
    body: &[u8],
) -> Result<(), IngestionError> {
    let digest = sha2::Sha256::digest(body)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    match policy.checksum_for(url) {
        Some(expected) if expected == digest => Ok(()),
        _ => Err(reference_error(
            reference,
            "remote document SHA-256 does not match its configured pin",
        )),
    }
}

fn schema_name(fragment: &str, reference: &str) -> Result<String, IngestionError> {
    fragment
        .strip_prefix("/components/schemas/")
        .filter(|name| !name.is_empty() && !name.contains('/'))
        .map(ToOwned::to_owned)
        .ok_or_else(|| reference_error(reference, "schema reference must name one component"))
}

fn reference_fragment(reference: &str) -> &str {
    reference
        .split_once('#')
        .map_or("", |(_, fragment)| fragment)
}

fn is_external_reference(reference: &str) -> bool {
    !reference.starts_with('#') && reference.contains('#')
}

fn is_safe_relative_path(path: &Path) -> bool {
    path.components()
        .all(|component| !matches!(component, std::path::Component::ParentDir))
}

fn reference_error(reference: &str, message: &str) -> IngestionError {
    IngestionError::ExternalReference {
        reference: reference.to_string(),
        message: message.to_string(),
    }
}
