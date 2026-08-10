use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use super::typescript::render_typescript_files;
use super::{
    ApiIr, GenerationError, GenerationMode, GenerationRequest, GenerationResult, IngestionError,
    Target, digest, render_config, render_manifest, render_readme, render_rust_cargo,
    render_rust_files, render_rust_mock_test, write_file,
};

pub fn generate(request: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
    match request.mode {
        GenerationMode::Write => write_generation(request),
        GenerationMode::DryRun | GenerationMode::Check => plan_generation(request),
    }
}

fn write_generation(request: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
    let (source, spec) = load_spec(request)?;
    prepare_existing_output(request)?;
    let generated = generate_repository(request.output_path(), &source, &spec, &request.targets)?;
    Ok(GenerationResult { files: generated })
}

fn prepare_existing_output(request: &GenerationRequest) -> Result<(), GenerationError> {
    let existing_manifest = read_existing_manifest(request.output_path())?;
    prepare_output(request, existing_manifest.is_some())?;
    validate_existing_files(request.output_path(), existing_manifest.as_ref())
}

fn plan_generation(request: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
    let (source, spec) = load_spec(request)?;
    let (staging, planned) = staged_repository(&source, &spec, &request.targets)?;
    let changed = changed_files(request.output_path(), staging.path(), &planned)?;
    if request.mode == GenerationMode::Check {
        return check_changes(changed);
    }
    Ok(GenerationResult { files: changed })
}

fn staged_repository(
    source: &str,
    spec: &ApiIr,
    targets: &[Target],
) -> Result<(tempfile::TempDir, Vec<PathBuf>), GenerationError> {
    let staging =
        tempfile::tempdir().map_err(|error| GenerationError::CreateOutput(error.to_string()))?;
    let planned = generate_repository(staging.path(), source, spec, targets)?;
    Ok((staging, planned))
}

fn changed_files(
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

fn file_changed(output: &Path, staging: &Path, relative: &Path) -> Result<bool, GenerationError> {
    let expected = fs::read(staging.join(relative)).map_err(|error| GenerationError::Write {
        path: relative.to_path_buf(),
        message: error.to_string(),
    })?;
    let actual = fs::read(output.join(relative)).ok();
    Ok(actual.as_deref() != Some(expected.as_slice()))
}

fn check_changes(changed: Vec<PathBuf>) -> Result<GenerationResult, GenerationError> {
    if changed.is_empty() {
        Ok(GenerationResult { files: changed })
    } else {
        Err(GenerationError::OutOfDate(changed))
    }
}

#[derive(Debug, Deserialize)]
struct ExistingManifest {
    files: Vec<ExistingManifestFile>,
}

#[derive(Debug, Deserialize)]
struct ExistingManifestFile {
    path: PathBuf,
    sha256: String,
}

fn read_existing_manifest(root: &Path) -> Result<Option<ExistingManifest>, GenerationError> {
    let path = root.join(".godsdk/manifest.json");
    if !path.is_file() {
        return Ok(None);
    }
    let contents =
        fs::read_to_string(path).map_err(|error| GenerationError::Manifest(error.to_string()))?;
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|error| GenerationError::Manifest(error.to_string()))
}

fn validate_existing_files(
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

fn generate_repository(
    root: &Path,
    source: &str,
    spec: &ApiIr,
    targets: &[Target],
) -> Result<Vec<PathBuf>, GenerationError> {
    let mut generated = Vec::new();
    let output = GenerationOutput {
        targets,
        files: &mut generated,
    };
    write_generated_content(root, source, spec, output)?;
    generate_lockfile(root, targets, &mut generated)?;
    write_manifest(root, source, targets, &mut generated)?;
    Ok(generated)
}

struct GenerationOutput<'a> {
    targets: &'a [Target],
    files: &'a mut Vec<PathBuf>,
}

fn load_spec(request: &GenerationRequest) -> Result<(String, ApiIr), GenerationError> {
    let source = read_source(request)?;
    let spec = ApiIr::parse(&source)?;
    Ok((source, spec))
}

fn write_generated_content(
    root: &Path,
    source: &str,
    spec: &ApiIr,
    output: GenerationOutput<'_>,
) -> Result<(), GenerationError> {
    let targets = output.targets;
    let files = output.files;
    write_source_and_rust(root, source, spec, GenerationOutput { targets, files })?;
    write_metadata(root, spec, targets, files)
}

fn write_manifest(
    root: &Path,
    source: &str,
    targets: &[Target],
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    let manifest = render_manifest(root, source, targets, generated)?;
    write_file(root, ".godsdk/manifest.json", &manifest, generated)
}

fn read_source(request: &GenerationRequest) -> Result<String, GenerationError> {
    fs::read_to_string(request.source_path()).map_err(|error| {
        GenerationError::Ingestion(IngestionError::Read {
            path: request.source.clone(),
            message: error.to_string(),
        })
    })
}

fn prepare_output(
    request: &GenerationRequest,
    existing_manifest: bool,
) -> Result<(), GenerationError> {
    if request.output.exists()
        && fs::read_dir(request.output_path())
            .map_err(|error| GenerationError::CreateOutput(error.to_string()))?
            .next()
            .is_some()
        && !existing_manifest
    {
        return Err(GenerationError::OutputExists(request.output.clone()));
    }
    fs::create_dir_all(request.output_path())
        .map_err(|error| GenerationError::CreateOutput(error.to_string()))
}

fn write_source_and_rust(
    root: &Path,
    source: &str,
    spec: &ApiIr,
    output: GenerationOutput<'_>,
) -> Result<(), GenerationError> {
    write_source_file(root, source, output.files)?;
    write_rust_files(root, spec, output.files)?;
    if output.targets.contains(&Target::TypeScript) {
        write_typescript_files(root, spec, output.files)?;
    }
    Ok(())
}

fn write_typescript_files(
    root: &Path,
    spec: &ApiIr,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    for (path, contents) in render_typescript_files(spec) {
        write_file(root, path, &contents, generated)?;
    }
    Ok(())
}

fn write_source_file(
    root: &Path,
    source: &str,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(root, "api/openapi.yaml", source, generated)
}

fn write_rust_files(
    root: &Path,
    spec: &ApiIr,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_rust_manifests(root, spec, generated)?;
    write_rust_sources(root, spec, generated)?;
    format_rust_sources(root)
}

fn write_rust_manifests(
    root: &Path,
    spec: &ApiIr,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(
        root,
        "sdk/rust/Cargo.toml",
        &render_rust_cargo(spec),
        generated,
    )
}

fn write_rust_sources(
    root: &Path,
    spec: &ApiIr,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    for (path, contents) in render_rust_files(spec) {
        write_file(root, &path, &contents, generated)?;
    }
    write_file(
        root,
        "sdk/rust/tests/mock_server.rs",
        &render_rust_mock_test(spec),
        generated,
    )
}

fn format_rust_sources(root: &Path) -> Result<(), GenerationError> {
    for directory in ["sdk/rust/src", "sdk/rust/tests"] {
        format_rust_directory(&root.join(directory))?;
    }
    Ok(())
}

fn format_rust_directory(path: &Path) -> Result<(), GenerationError> {
    fs::read_dir(path)
        .map_err(|error| GenerationError::Format(error.to_string()))?
        .map(|entry| entry.map_err(|error| GenerationError::Format(error.to_string())))
        .try_for_each(|entry| format_rust_entry(&entry?.path()))
}

fn format_rust_entry(path: &Path) -> Result<(), GenerationError> {
    if path.is_dir() {
        return format_rust_directory(path);
    }
    if path.extension().is_some_and(|extension| extension == "rs") {
        return format_rust_file(path);
    }
    Ok(())
}

fn format_rust_file(path: &Path) -> Result<(), GenerationError> {
    let result = Command::new("rustfmt")
        .args(["--edition", "2024"])
        .arg(path)
        .output()
        .map_err(|error| GenerationError::Format(error.to_string()))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(GenerationError::Format(
            String::from_utf8_lossy(&result.stderr).trim().to_string(),
        ))
    }
}

fn generate_lockfile(
    root: &Path,
    targets: &[Target],
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    generate_lockfile_at(root, "sdk/rust", "sdk/rust/Cargo.lock", generated)?;
    if targets.contains(&Target::TypeScript) {
        generate_lockfile_at(
            root,
            "sdk/typescript/native",
            "sdk/typescript/native/Cargo.lock",
            generated,
        )?;
    }
    Ok(())
}

fn generate_lockfile_at(
    root: &Path,
    relative_directory: &str,
    relative_lockfile: &str,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    let result = Command::new("cargo")
        .arg("generate-lockfile")
        .current_dir(root.join(relative_directory))
        .output()
        .map_err(|error| GenerationError::Lockfile(error.to_string()))?;
    if !result.status.success() {
        return Err(GenerationError::Lockfile(
            String::from_utf8_lossy(&result.stderr).trim().to_string(),
        ));
    }
    generated.push(PathBuf::from(relative_lockfile));
    Ok(())
}

fn write_metadata(
    root: &Path,
    spec: &ApiIr,
    targets: &[Target],
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_workflows(root, generated)?;
    write_project_metadata(root, spec, targets, generated)?;
    write_attention_document(root, generated)
}

fn write_workflows(root: &Path, generated: &mut Vec<PathBuf>) -> Result<(), GenerationError> {
    write_file(
        root,
        ".github/workflows/godlint.yml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/godlint-workflow.yml"
        )),
        generated,
    )?;
    write_file(
        root,
        ".github/workflows/release.yml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/release-workflow.yml"
        )),
        generated,
    )?;
    Ok(())
}

fn write_project_metadata(
    root: &Path,
    spec: &ApiIr,
    targets: &[Target],
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(
        root,
        ".godsdk/config.yaml",
        &render_config(spec, targets),
        generated,
    )?;
    write_file(root, "godlint.yaml", &render_generated_godlint(), generated)?;
    write_file(
        root,
        "godharness.yaml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/godharness.yaml"
        )),
        generated,
    )?;
    write_file(root, "README.md", &render_readme(spec), generated)
}

fn write_attention_document(
    root: &Path,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(
        root,
        "NEEDS-YOUR-ATTENTION.md",
        "# Needs your attention\n\nThe generator completed all repository-local setup. Manual actions remain only at external services:\n\n- [ ] Reserve the generated crates.io package name and configure its GitHub trusted publisher.\n- [ ] Reserve the generated npm root and platform package names and configure npm trusted publishing for the release workflow.\n- [ ] If enabling Homebrew, grant the release environment write access to the selected tap repository.\n",
        generated,
    )
}

fn render_generated_godlint() -> String {
    format!(
        "{}\nexclude:\n  - sdk/typescript/native/index.js\n  - sdk/typescript/native/index.d.ts\n  - sdk/typescript/native/*.node\n  - sdk/typescript/native/target/**\n  - sdk/typescript/node_modules/**\n",
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/godlint.yaml"))
    )
}
