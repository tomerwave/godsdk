use std::fs;
use std::path::{Path, PathBuf};

use super::{
    ApiSpec, GenerationError, GenerationRequest, GenerationResult, IngestionError, render_config,
    render_manifest, render_readme, render_rust_cargo, render_rust_client, render_rust_lock,
    write_file,
};

pub fn generate(request: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
    let (source, spec) = load_spec(request)?;
    prepare_output(request)?;
    let mut generated = Vec::new();
    write_generated_content(request.output_path(), &source, &spec, &mut generated)?;
    write_manifest(request.output_path(), &source, &mut generated)?;
    Ok(GenerationResult { files: generated })
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
    write_file(root, "api/openapi.yaml", source, generated)?;
    write_file(
        root,
        "sdk/rust/Cargo.toml",
        &render_rust_cargo(spec),
        generated,
    )?;
    write_file(
        root,
        "sdk/rust/Cargo.lock",
        &render_rust_lock(spec),
        generated,
    )?;
    write_file(
        root,
        "sdk/rust/src/lib.rs",
        &render_rust_client(spec),
        generated,
    )
}

fn write_metadata(
    root: &Path,
    spec: &ApiSpec,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(root, ".godsdk/config.yaml", &render_config(spec), generated)?;
    write_file(
        root,
        "godlint.yaml",
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../godlint.yaml")),
        generated,
    )?;
    write_file(
        root,
        "godharness.yaml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../godharness.yaml"
        )),
        generated,
    )?;
    write_file(root, "README.md", &render_readme(spec), generated)?;
    write_file(
        root,
        "NEEDS-YOUR-ATTENTION.md",
        "# Needs your attention\n\n- [ ] Configure the external crates.io publisher for this package.\n",
        generated,
    )
}
