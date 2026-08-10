use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::generation_transaction::{
    ApplyOptions, ExistingManifest, apply_staged_repository, changed_files, check_changes,
    read_existing_manifest, stale_paths, validate_existing_files,
};
use super::python::render_python_files;
use super::typescript::render_typescript_files;
use super::{
    ApiIr, GenerationError, GenerationMode, GenerationRequest, GenerationResult, IngestionError,
    Target, render_config, render_manifest, render_readme, render_rust_cargo, render_rust_files,
    render_rust_mock_test, write_file,
};

pub fn generate(request: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
    match request.mode {
        GenerationMode::Write => write_generation(request),
        GenerationMode::DryRun | GenerationMode::Check => plan_generation(request),
    }
}

fn write_generation(request: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
    let (existing_manifest, staging, planned) = staged_request(request)?;
    let changed = apply_staged_repository(
        request.output_path(),
        staging.path(),
        &planned,
        ApplyOptions {
            existing_manifest: existing_manifest.as_ref(),
            enabled: request.prune,
        },
    )?;
    Ok(GenerationResult { files: changed })
}

fn plan_generation(request: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
    let (existing_manifest, staging, planned) = staged_request(request)?;
    let mut changed = changed_files(request.output_path(), staging.path(), &planned)?;
    if request.prune {
        changed.extend(stale_paths(
            existing_manifest.as_ref(),
            &planned,
            request.output_path(),
        ));
        changed.sort();
        changed.dedup();
    }
    changed.retain(|path| path != Path::new("sdk/typescript/native/index.d.ts"));
    if request.mode == GenerationMode::Check {
        return check_changes(changed);
    }
    Ok(GenerationResult { files: changed })
}

fn staged_request(
    request: &GenerationRequest,
) -> Result<(Option<ExistingManifest>, tempfile::TempDir, Vec<PathBuf>), GenerationError> {
    let existing_manifest = prepare_existing_generation(request)?;
    let (source, spec) = load_spec(request)?;
    let context = GenerationContext {
        targets: &request.targets,
        reference_policy: &request.reference_policy,
    };
    let (staging, planned) = staged_repository(&source, &spec, context)?;
    Ok((existing_manifest, staging, planned))
}

fn prepare_existing_generation(
    request: &GenerationRequest,
) -> Result<Option<ExistingManifest>, GenerationError> {
    let existing_manifest = read_existing_manifest(request.output_path())?;
    prepare_output(request, existing_manifest.is_some())?;
    validate_existing_files(request.output_path(), existing_manifest.as_ref())?;
    Ok(existing_manifest)
}

fn staged_repository(
    source: &str,
    spec: &ApiIr,
    context: GenerationContext<'_>,
) -> Result<(tempfile::TempDir, Vec<PathBuf>), GenerationError> {
    let staging =
        tempfile::tempdir().map_err(|error| GenerationError::CreateOutput(error.to_string()))?;
    let planned = generate_repository(staging.path(), source, spec, context)?;
    Ok((staging, planned))
}

fn generate_repository(
    root: &Path,
    source: &str,
    spec: &ApiIr,
    context: GenerationContext<'_>,
) -> Result<Vec<PathBuf>, GenerationError> {
    let mut generated = Vec::new();
    let output = GenerationOutput {
        context,
        files: &mut generated,
    };
    write_generated_content(root, source, spec, output)?;
    generate_lockfile(root, context.targets, &mut generated)?;
    write_manifest(root, source, context.targets, &mut generated)?;
    Ok(generated)
}

struct GenerationOutput<'a> {
    context: GenerationContext<'a>,
    files: &'a mut Vec<PathBuf>,
}

#[derive(Clone, Copy)]
struct GenerationContext<'a> {
    targets: &'a [Target],
    reference_policy: &'a super::ReferencePolicy,
}

fn load_spec(request: &GenerationRequest) -> Result<(String, ApiIr), GenerationError> {
    let source = read_source(request)?;
    let spec = ApiIr::parse_with_policy(
        &source,
        request.source_path().parent(),
        &request.reference_policy,
    )?;
    Ok((source, spec))
}

fn write_generated_content(
    root: &Path,
    source: &str,
    spec: &ApiIr,
    output: GenerationOutput<'_>,
) -> Result<(), GenerationError> {
    let context = output.context;
    let files = output.files;
    write_source_and_rust(root, source, spec, GenerationOutput { context, files })?;
    write_metadata(root, spec, context, files)
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
    if let Some(parent) = request.output_path().parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| GenerationError::CreateOutput(error.to_string()))?;
    }
    Ok(())
}

fn write_source_and_rust(
    root: &Path,
    source: &str,
    spec: &ApiIr,
    output: GenerationOutput<'_>,
) -> Result<(), GenerationError> {
    write_source_file(root, source, output.files)?;
    write_rust_files(root, spec, output.files)?;
    write_binding_files(root, spec, output.context.targets, output.files)?;
    Ok(())
}

fn write_binding_files(
    root: &Path,
    spec: &ApiIr,
    targets: &[Target],
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    if targets.contains(&Target::TypeScript) {
        write_typescript_files(root, spec, generated)?;
    }
    if targets.contains(&Target::Python) {
        write_python_files(root, spec, generated)?;
    }
    Ok(())
}

fn write_python_files(
    root: &Path,
    spec: &ApiIr,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    for (path, contents) in render_python_files(spec) {
        write_file(root, &path, &contents, generated)?;
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
    for (target, directory, lockfile) in [
        (
            Target::TypeScript,
            "sdk/typescript/native",
            "sdk/typescript/native/Cargo.lock",
        ),
        (
            Target::Python,
            "sdk/python/native",
            "sdk/python/native/Cargo.lock",
        ),
    ] {
        if targets.contains(&target) {
            generate_lockfile_at(root, directory, lockfile, generated)?;
        }
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
    context: GenerationContext<'_>,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_workflows(root, context.targets, generated)?;
    write_project_metadata(root, spec, context, generated)?;
    write_attention_document(root, context.targets, generated)
}

fn write_workflows(
    root: &Path,
    targets: &[Target],
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(
        root,
        ".github/workflows/godlint.yml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/godlint-workflow.yml"
        )),
        generated,
    )?;
    let release = render_release_workflow(targets);
    write_file(root, ".github/workflows/release.yml", &release, generated)?;
    Ok(())
}

fn render_release_workflow(targets: &[Target]) -> String {
    let typescript = targets.contains(&Target::TypeScript);
    let python = targets.contains(&Target::Python);
    let mut release = filter_target_block(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/release-workflow.yml"
        )),
        "typescript",
        typescript,
    );
    let package_needs = [
        Some("crates"),
        typescript.then_some("npm"),
        python.then_some("pypi"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ");
    release = release.replace(
        "needs: [__GODSDK_GITHUB_NEEDS__]",
        &format!("needs: [{package_needs}]"),
    );
    if python {
        release.push_str(&filter_target_block(
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/python-release-workflow.yml"
            )),
            "python",
            true,
        ));
    }
    release
}

fn filter_target_block(source: &str, target: &str, enabled: bool) -> String {
    let mut filter = TargetBlockFilter::new(target, enabled);
    source
        .lines()
        .filter_map(|line| filter.accept(line))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

struct TargetBlockFilter {
    start: String,
    end: String,
    enabled: bool,
    inside: bool,
}

impl TargetBlockFilter {
    fn new(target: &str, enabled: bool) -> Self {
        Self {
            start: format!("# GODSDK_TARGET: {target}:start"),
            end: format!("# GODSDK_TARGET: {target}:end"),
            enabled,
            inside: false,
        }
    }

    fn accept<'a>(&mut self, line: &'a str) -> Option<&'a str> {
        if line.trim() == self.start {
            self.inside = true;
            return None;
        }
        if line.trim() == self.end {
            self.inside = false;
            return None;
        }
        (!self.inside || self.enabled).then_some(line)
    }
}

fn write_project_metadata(
    root: &Path,
    spec: &ApiIr,
    context: GenerationContext<'_>,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(
        root,
        ".godsdk/config.yaml",
        &render_config(spec, context.targets, context.reference_policy),
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
    write_file(
        root,
        "README.md",
        &render_readme(spec, context.targets),
        generated,
    )
}

fn write_attention_document(
    root: &Path,
    targets: &[Target],
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    let mut actions = vec![
        "- [ ] Reserve the generated crates.io package name and configure its GitHub trusted publisher.".to_string(),
    ];
    if targets.contains(&Target::TypeScript) {
        actions.push(
            "- [ ] Reserve the generated npm root and platform package names and configure npm trusted publishing for the release workflow.".to_string(),
        );
    }
    if targets.contains(&Target::Python) {
        actions.push(
            "- [ ] Reserve the generated PyPI package name and configure its GitHub trusted publisher.".to_string(),
        );
    }
    write_file(
        root,
        "NEEDS-YOUR-ATTENTION.md",
        &format!(
            "# Needs your attention\n\nThe generator completed all repository-local setup. Manual actions remain only at external services:\n\n{}\n",
            actions.join("\n")
        ),
        generated,
    )
}

fn render_generated_godlint() -> String {
    format!(
        "{}\nexclude:\n  - sdk/typescript/native/index.js\n  - sdk/typescript/native/index.d.ts\n  - sdk/typescript/native/*.node\n  - sdk/typescript/native/target/**\n  - sdk/typescript/node_modules/**\n",
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/godlint.yaml"))
    )
}
