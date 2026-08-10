use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{GenerationError, GenerationResult, digest};

#[derive(Debug, Deserialize)]
pub(super) struct ExistingManifest {
    schema_version: u32,
    files: Vec<ExistingManifestFile>,
}

#[derive(Debug, Deserialize)]
struct ExistingManifestFile {
    path: PathBuf,
    sha256: String,
}

pub(super) fn read_existing_manifest(
    root: &Path,
) -> Result<Option<ExistingManifest>, GenerationError> {
    let path = root.join(".godsdk/manifest.json");
    if !path.is_file() {
        return Ok(None);
    }
    let contents =
        fs::read_to_string(path).map_err(|error| GenerationError::Manifest(error.to_string()))?;
    let manifest: ExistingManifest = serde_json::from_str(&contents)
        .map_err(|error| GenerationError::Manifest(error.to_string()))?;
    validate_manifest(&manifest)?;
    Ok(Some(manifest))
}

fn validate_manifest(manifest: &ExistingManifest) -> Result<(), GenerationError> {
    if manifest.schema_version != 1 {
        return Err(GenerationError::Manifest(format!(
            "unsupported manifest schema version {}; expected 1",
            manifest.schema_version
        )));
    }
    if manifest
        .files
        .iter()
        .any(|file| !is_safe_relative_path(&file.path))
    {
        return Err(GenerationError::Manifest(
            "manifest contains an unsafe generated file path".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_existing_files(
    root: &Path,
    manifest: Option<&ExistingManifest>,
) -> Result<(), GenerationError> {
    let Some(manifest) = manifest else {
        return Ok(());
    };
    for file in &manifest.files {
        if let Some(error) = existing_file_conflict(root, file) {
            return Err(error);
        }
    }
    Ok(())
}

fn existing_file_conflict(root: &Path, file: &ExistingManifestFile) -> Option<GenerationError> {
    if file.path == Path::new("api/openapi.yaml") {
        return None;
    }
    let contents = fs::read_to_string(root.join(&file.path)).ok()?;
    (digest(&contents) != file.sha256)
        .then(|| GenerationError::GeneratedFileConflict(file.path.clone()))
}

pub(super) fn changed_files(
    output: &Path,
    staging: &Path,
    planned: &[PathBuf],
) -> Result<Vec<PathBuf>, GenerationError> {
    planned
        .iter()
        .filter_map(|relative| match file_changed(output, staging, relative) {
            Ok(true) => Some(Ok(relative.clone())),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

pub(super) fn apply_staged_repository(
    output: &Path,
    staging: &Path,
    planned: &[PathBuf],
    options: ApplyOptions<'_>,
) -> Result<Vec<PathBuf>, GenerationError> {
    if !output.exists() {
        fs::create_dir_all(output)
            .map_err(|error| GenerationError::CreateOutput(error.to_string()))?;
    }

    let mut changed = write_changed_files(output, staging, planned)?;
    changed.extend(prune_stale_files(
        output,
        planned,
        options.existing_manifest,
        options.enabled,
    )?);
    changed.sort();
    changed.dedup();
    Ok(changed)
}

pub(super) struct ApplyOptions<'a> {
    pub(super) existing_manifest: Option<&'a ExistingManifest>,
    pub(super) enabled: bool,
}

fn write_changed_files(
    output: &Path,
    staging: &Path,
    planned: &[PathBuf],
) -> Result<Vec<PathBuf>, GenerationError> {
    let changed = changed_files(output, staging, planned)?;
    for relative in &changed {
        copy_file_atomically(staging, output, relative)?;
    }
    Ok(changed)
}

fn prune_stale_files(
    output: &Path,
    planned: &[PathBuf],
    existing_manifest: Option<&ExistingManifest>,
    enabled: bool,
) -> Result<Vec<PathBuf>, GenerationError> {
    if !enabled {
        return Ok(Vec::new());
    }
    stale_paths(existing_manifest, planned, output)
        .into_iter()
        .map(|relative| {
            remove_file_and_empty_parents(output, &relative)?;
            Ok(relative)
        })
        .collect()
}

pub(super) fn stale_paths(
    existing_manifest: Option<&ExistingManifest>,
    planned: &[PathBuf],
    output: &Path,
) -> Vec<PathBuf> {
    let Some(manifest) = existing_manifest else {
        return Vec::new();
    };
    manifest
        .files
        .iter()
        .map(|file| file.path.clone())
        .filter(|path| !planned.iter().any(|planned| planned == path))
        .filter(|path| output.join(path).exists())
        .collect()
}

pub(super) fn check_changes(changed: Vec<PathBuf>) -> Result<GenerationResult, GenerationError> {
    if changed.is_empty() {
        Ok(GenerationResult { files: changed })
    } else {
        Err(GenerationError::OutOfDate(changed))
    }
}

fn file_changed(output: &Path, staging: &Path, relative: &Path) -> Result<bool, GenerationError> {
    let expected = fs::read(staging.join(relative)).map_err(|error| GenerationError::Write {
        path: relative.to_path_buf(),
        message: error.to_string(),
    })?;
    let actual = fs::read(output.join(relative)).ok();
    Ok(actual.as_deref() != Some(expected.as_slice()))
}

fn copy_file_atomically(
    staging: &Path,
    output: &Path,
    relative: &Path,
) -> Result<(), GenerationError> {
    let destination = output.join(relative);
    let parent = parent_directory(&destination)?;
    fs::create_dir_all(parent).map_err(|error| GenerationError::Write {
        path: parent.to_path_buf(),
        message: error.to_string(),
    })?;
    let contents = fs::read(staging.join(relative)).map_err(|error| GenerationError::Write {
        path: relative.to_path_buf(),
        message: error.to_string(),
    })?;
    replace_file_atomically(parent, &destination, &contents)
}

fn parent_directory(path: &Path) -> Result<&Path, GenerationError> {
    path.parent().ok_or_else(|| GenerationError::Write {
        path: path.to_path_buf(),
        message: "generated file has no parent directory".to_string(),
    })
}

fn replace_file_atomically(
    parent: &Path,
    destination: &Path,
    contents: &[u8],
) -> Result<(), GenerationError> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| GenerationError::Write {
            path: destination.to_path_buf(),
            message: error.to_string(),
        })?;
    temporary
        .write_all(contents)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| GenerationError::Write {
            path: destination.to_path_buf(),
            message: error.to_string(),
        })?;
    temporary
        .persist(destination)
        .map_err(|error| GenerationError::Write {
            path: destination.to_path_buf(),
            message: error.error.to_string(),
        })?;
    Ok(())
}

fn remove_file_and_empty_parents(root: &Path, relative: &Path) -> Result<(), GenerationError> {
    let path = root.join(relative);
    if path.exists() {
        fs::remove_file(&path).map_err(|error| GenerationError::Write {
            path: relative.to_path_buf(),
            message: error.to_string(),
        })?;
    }
    let mut parent = path.parent();
    while let Some(directory) = parent {
        if directory == root || fs::remove_dir(directory).is_err() {
            break;
        }
        parent = directory.parent();
    }
    Ok(())
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|component| !matches!(component, std::path::Component::ParentDir))
}
