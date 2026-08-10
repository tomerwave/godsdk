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
    if file.path == Path::new("api/openapi.yaml")
        || file.path == Path::new("sdk/typescript/native/index.d.ts")
    {
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
    let (changed, stale, output_was_missing) = prepare_apply(output, staging, planned, &options)?;
    let backup = create_backup(output)?;
    let mut transaction = FileTransaction {
        output,
        backup,
        backed_up: Vec::new(),
        created: Vec::new(),
        output_was_missing,
    };
    transaction.finish(changed, stale, staging)
}

fn prepare_apply(
    output: &Path,
    staging: &Path,
    planned: &[PathBuf],
    options: &ApplyOptions<'_>,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>, bool), GenerationError> {
    let changed = changed_files(output, staging, planned)?;
    let stale = if options.enabled {
        stale_paths(options.existing_manifest, planned, output)
    } else {
        Vec::new()
    };
    let output_was_missing = !output.exists();
    if output_was_missing {
        fs::create_dir_all(output)
            .map_err(|error| GenerationError::CreateOutput(error.to_string()))?;
    }
    Ok((changed, stale, output_was_missing))
}

fn create_backup(output: &Path) -> Result<tempfile::TempDir, GenerationError> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    tempfile::tempdir_in(parent).map_err(|error| {
        GenerationError::CreateOutput(format!("could not create generation backup: {error}"))
    })
}

pub(super) struct ApplyOptions<'a> {
    pub(super) existing_manifest: Option<&'a ExistingManifest>,
    pub(super) enabled: bool,
}

struct FileTransaction<'a> {
    output: &'a Path,
    backup: tempfile::TempDir,
    backed_up: Vec<PathBuf>,
    created: Vec<PathBuf>,
    output_was_missing: bool,
}

impl FileTransaction<'_> {
    fn finish(
        &mut self,
        changed: Vec<PathBuf>,
        stale: Vec<PathBuf>,
        staging: &Path,
    ) -> Result<Vec<PathBuf>, GenerationError> {
        let result = self.apply(&changed, &stale, staging);
        if let Err(error) = result {
            self.rollback();
            return Err(error);
        }
        if let Err(error) = self.remove_stale_parents(&stale) {
            self.rollback();
            return Err(error);
        }
        let mut changed = changed;
        changed.extend(stale);
        changed.sort();
        changed.dedup();
        Ok(changed)
    }

    fn remove_stale_parents(&self, stale: &[PathBuf]) -> Result<(), GenerationError> {
        stale
            .iter()
            .try_for_each(|relative| remove_file_and_empty_parents(self.output, relative))
    }

    fn apply(
        &mut self,
        changed: &[PathBuf],
        stale: &[PathBuf],
        staging: &Path,
    ) -> Result<(), GenerationError> {
        let mut to_backup = changed.to_vec();
        to_backup.extend(stale.iter().cloned());
        to_backup.sort();
        to_backup.dedup();
        for relative in to_backup {
            self.backup_existing(&relative)?;
        }
        for relative in changed {
            copy_file_atomically(staging, self.output, relative)?;
            self.created.push(relative.clone());
        }
        Ok(())
    }

    fn backup_existing(&mut self, relative: &Path) -> Result<(), GenerationError> {
        let source = self.output.join(relative);
        if !source.exists() {
            return Ok(());
        }
        let destination = self.backup.path().join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| GenerationError::Write {
                path: parent.to_path_buf(),
                message: error.to_string(),
            })?;
        }
        fs::rename(&source, &destination).map_err(|error| GenerationError::Write {
            path: relative.to_path_buf(),
            message: error.to_string(),
        })?;
        self.backed_up.push(relative.to_path_buf());
        Ok(())
    }

    fn rollback(&mut self) {
        if self.output_was_missing {
            let _ = fs::remove_dir_all(self.output);
            return;
        }
        for relative in &self.created {
            let _ = remove_file_and_empty_parents(self.output, relative);
        }
        for relative in self.backed_up.iter().rev() {
            let source = self.backup.path().join(relative);
            let destination = self.output.join(relative);
            if let Some(parent) = destination.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::rename(source, destination);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_multi_file_apply_restores_the_repository() {
        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary root: {error}"));
        let output = root.path().join("output");
        let staging = root.path().join("staging");
        fs::create_dir_all(&staging).unwrap_or_else(|error| panic!("staging directory: {error}"));
        fs::create_dir_all(&output).unwrap_or_else(|error| panic!("output directory: {error}"));
        fs::write(staging.join("first.txt"), "new")
            .unwrap_or_else(|error| panic!("first staged file: {error}"));
        fs::create_dir_all(staging.join("blocked"))
            .unwrap_or_else(|error| panic!("blocked staging directory: {error}"));
        fs::write(staging.join("blocked/file.txt"), "new")
            .unwrap_or_else(|error| panic!("blocked staged file: {error}"));
        fs::write(output.join("blocked"), "user file")
            .unwrap_or_else(|error| panic!("blocking output file: {error}"));

        let error = match apply_staged_repository(
            &output,
            &staging,
            &[
                PathBuf::from("first.txt"),
                PathBuf::from("blocked/file.txt"),
            ],
            ApplyOptions {
                existing_manifest: None,
                enabled: false,
            },
        ) {
            Ok(_) => panic!("a file must block the second staged write"),
            Err(error) => error,
        };

        assert!(matches!(error, GenerationError::Write { .. }));
        assert!(!output.join("first.txt").exists());
        assert_eq!(
            fs::read_to_string(output.join("blocked"))
                .unwrap_or_else(|error| panic!("restored blocking file: {error}")),
            "user file"
        );
    }
}
