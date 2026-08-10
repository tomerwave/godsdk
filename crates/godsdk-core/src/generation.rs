use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::typescript::render_typescript_files;
use super::{
    ApiSpec, GenerationError, GenerationRequest, GenerationResult, IngestionError, render_config,
    render_manifest, render_readme, render_rust_cargo, render_rust_client, render_rust_mock_test,
    render_rust_models, write_file,
};

pub fn generate(request: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
    let (source, spec) = load_spec(request)?;
    prepare_output(request)?;
    let generated = generate_repository(request.output_path(), &source, &spec)?;
    Ok(GenerationResult { files: generated })
}

fn generate_repository(
    root: &Path,
    source: &str,
    spec: &ApiSpec,
) -> Result<Vec<PathBuf>, GenerationError> {
    let mut generated = Vec::new();
    write_generated_content(root, source, spec, &mut generated)?;
    generate_lockfile(root, &mut generated)?;
    write_manifest(root, source, &mut generated)?;
    Ok(generated)
}

fn load_spec(request: &GenerationRequest) -> Result<(String, ApiSpec), GenerationError> {
    let source = read_source(request)?;
    let spec = ApiSpec::parse(&source)?;
    Ok((source, spec))
}

fn write_generated_content(
    root: &Path,
    source: &str,
    spec: &ApiSpec,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_source_and_rust(root, source, spec, generated)?;
    write_metadata(root, spec, generated)
}

fn write_manifest(
    root: &Path,
    source: &str,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    let manifest = render_manifest(root, source, generated)?;
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

fn prepare_output(request: &GenerationRequest) -> Result<(), GenerationError> {
    if request.output.exists()
        && fs::read_dir(request.output_path())
            .map_err(|error| GenerationError::CreateOutput(error.to_string()))?
            .next()
            .is_some()
    {
        return Err(GenerationError::OutputExists(request.output.clone()));
    }
    fs::create_dir_all(request.output_path())
        .map_err(|error| GenerationError::CreateOutput(error.to_string()))
}

fn write_source_and_rust(
    root: &Path,
    source: &str,
    spec: &ApiSpec,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_source_file(root, source, generated)?;
    write_rust_files(root, spec, generated)?;
    write_typescript_files(root, spec, generated)
}

fn write_typescript_files(
    root: &Path,
    spec: &ApiSpec,
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
    spec: &ApiSpec,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_rust_manifests(root, spec, generated)?;
    write_rust_sources(root, spec, generated)?;
    format_rust_sources(root)
}

fn write_rust_manifests(
    root: &Path,
    spec: &ApiSpec,
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
    spec: &ApiSpec,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(
        root,
        "sdk/rust/src/lib.rs",
        &render_rust_client(spec),
        generated,
    )?;
    write_file(
        root,
        "sdk/rust/src/models.rs",
        &render_rust_models(spec),
        generated,
    )?;
    write_file(
        root,
        "sdk/rust/tests/mock_server.rs",
        &render_rust_mock_test(spec),
        generated,
    )
}

fn format_rust_sources(root: &Path) -> Result<(), GenerationError> {
    format_rust_file(&root.join("sdk/rust/src/lib.rs"))?;
    format_rust_file(&root.join("sdk/rust/src/models.rs"))?;
    format_rust_file(&root.join("sdk/rust/tests/mock_server.rs"))
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

fn generate_lockfile(root: &Path, generated: &mut Vec<PathBuf>) -> Result<(), GenerationError> {
    generate_lockfile_at(root, "sdk/rust", "sdk/rust/Cargo.lock", generated)?;
    generate_lockfile_at(
        root,
        "sdk/typescript/native",
        "sdk/typescript/native/Cargo.lock",
        generated,
    )
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
    spec: &ApiSpec,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(root, ".godsdk/config.yaml", &render_config(spec), generated)?;
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
    write_file(root, "README.md", &render_readme(spec), generated)?;
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
