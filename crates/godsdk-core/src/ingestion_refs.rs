use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::IngestionError;

pub(crate) fn external_references(value: &Value) -> BTreeSet<String> {
    let mut references = BTreeSet::new();
    collect_external_references(value, &mut references);
    references
}

pub(crate) fn resolve_external_references(
    root: &mut Value,
    base_directory: &Path,
) -> Result<(), IngestionError> {
    resolve_external_parameters(root, base_directory, &mut BTreeSet::new())?;
    import_external_schemas(root, base_directory)
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
    base_directory: &Path,
    visited: &mut BTreeSet<(PathBuf, String)>,
) -> Result<(), IngestionError> {
    if let Some(target) = external_parameter_target(value, base_directory, visited)? {
        *value = target;
    } else {
        recurse_external_parameters(value, base_directory, visited)?;
    }
    Ok(())
}

fn external_parameter_target(
    value: &Value,
    base_directory: &Path,
    visited: &mut BTreeSet<(PathBuf, String)>,
) -> Result<Option<Value>, IngestionError> {
    let Some(reference) = external_parameter_reference(value) else {
        return Ok(None);
    };
    let (path, fragment) = split_reference(reference, base_directory)?;
    let target = resolve_external_parameter_target(
        &path,
        &fragment,
        reference,
        ResolutionContext {
            base_directory,
            visited,
        },
    )?;
    Ok(Some(target))
}

struct ResolutionContext<'a> {
    base_directory: &'a Path,
    visited: &'a mut BTreeSet<(PathBuf, String)>,
}

fn resolve_external_parameter_target(
    path: &Path,
    fragment: &str,
    reference: &str,
    context: ResolutionContext<'_>,
) -> Result<Value, IngestionError> {
    ensure_unvisited(context.visited, path, fragment, reference)?;
    let mut target = load_target(path, fragment, reference)?;
    resolve_external_parameters(
        &mut target,
        path.parent().unwrap_or(context.base_directory),
        context.visited,
    )?;
    Ok(target)
}

fn ensure_unvisited(
    visited: &mut BTreeSet<(PathBuf, String)>,
    path: &Path,
    fragment: &str,
    reference: &str,
) -> Result<(), IngestionError> {
    if visited.insert((path.to_path_buf(), fragment.to_string())) {
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
    base_directory: &Path,
    visited: &mut BTreeSet<(PathBuf, String)>,
) -> Result<(), IngestionError> {
    match value {
        Value::Array(values) => values
            .iter_mut()
            .try_for_each(|value| resolve_external_parameters(value, base_directory, visited)),
        Value::Object(object) => object
            .values_mut()
            .try_for_each(|value| resolve_external_parameters(value, base_directory, visited)),
        _ => Ok(()),
    }
}

fn import_external_schemas(root: &mut Value, base_directory: &Path) -> Result<(), IngestionError> {
    let imported = collect_imported_schemas(root, base_directory)?;
    let schemas = schemas_object(root)?;
    for (name, target) in imported {
        schemas.entry(name).or_insert(target);
    }
    Ok(())
}

fn collect_imported_schemas(
    root: &Value,
    base_directory: &Path,
) -> Result<BTreeMap<String, Value>, IngestionError> {
    let mut imported = BTreeMap::new();
    let mut queue = external_references(root)
        .into_iter()
        .filter(|reference| reference_fragment(reference).starts_with("/components/schemas/"))
        .map(|reference| (reference, base_directory.to_path_buf()))
        .collect::<Vec<_>>();
    while let Some((reference, current_base)) = queue.pop() {
        let schema = load_external_schema(&reference, &current_base)?;
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
    base: PathBuf,
}

fn load_external_schema(
    reference: &str,
    base_directory: &Path,
) -> Result<ImportedSchema, IngestionError> {
    let (path, fragment) = split_reference(reference, base_directory)?;
    let name = schema_name(&fragment, reference)?;
    let value = load_target(&path, &fragment, reference)?;
    let references = external_references(&value)
        .into_iter()
        .filter(|nested| reference_fragment(nested).starts_with("/components/schemas/"))
        .collect();
    Ok(ImportedSchema {
        name,
        value,
        references,
        base: path.parent().unwrap_or(base_directory).to_path_buf(),
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

fn load_target(path: &Path, fragment: &str, reference: &str) -> Result<Value, IngestionError> {
    let source = fs::read_to_string(path)
        .map_err(|error| reference_error(reference, &format!("could not read file: {error}")))?;
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
    base_directory: &Path,
) -> Result<(PathBuf, String), IngestionError> {
    let (file, fragment) = reference.split_once('#').unwrap_or((reference, ""));
    let relative = Path::new(file);
    if relative.is_absolute() || file.contains("://") || !is_safe_relative_path(relative) {
        return Err(reference_error(
            reference,
            "only relative local files are supported",
        ));
    }
    let path = base_directory.join(relative);
    let fragment = if fragment.is_empty() {
        "/".to_string()
    } else {
        fragment.to_string()
    };
    Ok((path, fragment))
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
