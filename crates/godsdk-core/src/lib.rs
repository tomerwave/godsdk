use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

mod generation;
mod render;
mod schema;
mod typescript;
pub use generation::generate;
pub(crate) use render::{
    http_method_name, render_rust_mock_test, render_rust_models, rust_identifier,
    rust_response_type,
};
pub use schema::Schema;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationRequest {
    pub source: PathBuf,
    pub output: PathBuf,
}

impl GenerationRequest {
    pub fn new(source: impl Into<PathBuf>, output: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            output: output.into(),
        }
    }

    pub fn source_path(&self) -> &Path {
        &self.source
    }

    pub fn output_path(&self) -> &Path {
        &self.output
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationResult {
    pub files: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GenerationError {
    #[error("could not read source document: {0}")]
    Ingestion(#[from] IngestionError),
    #[error("could not create generated repository: {0}")]
    CreateOutput(String),
    #[error("generated output already exists and is not empty: {0}")]
    OutputExists(PathBuf),
    #[error("could not write generated file {path}: {message}")]
    Write { path: PathBuf, message: String },
    #[error("could not generate Cargo.lock: {0}")]
    Lockfile(String),
    #[error("could not format generated Rust source: {0}")]
    Format(String),
}

pub(crate) fn write_file(
    root: &Path,
    relative: &str,
    contents: &str,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| GenerationError::Write {
            path: parent.to_path_buf(),
            message: error.to_string(),
        })?;
    }
    fs::write(&path, contents).map_err(|error| GenerationError::Write {
        path: path.clone(),
        message: error.to_string(),
    })?;
    generated.push(PathBuf::from(relative));
    Ok(())
}

fn digest(contents: &str) -> String {
    Sha256::digest(contents.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn render_rust_cargo(spec: &ApiSpec) -> String {
    format!(
        "[package]\nname = \"{}-sdk\"\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.97\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[features]\ndefault = [\"rustls-tls\"]\nrustls-tls = [\"reqwest/rustls-tls\"]\nnative-tls = [\"reqwest/native-tls\"]\ntracing = [\"dep:tracing\"]\n\n[dependencies]\npercent-encoding = \"2\"\nreqwest = {{ version = \"0.12\", default-features = false, features = [\"json\"] }}\nserde = {{ version = \"1\", features = [\"derive\"] }}\nserde_json = \"1\"\nthiserror = \"2\"\ntokio = {{ version = \"1\", features = [\"macros\", \"rt\", \"time\"] }}\ntracing = {{ version = \"0.1\", optional = true }}\nurl = \"2\"\n",
        slug(&spec.title)
    )
}

pub(crate) fn render_rust_client(spec: &ApiSpec) -> String {
    render_rust_client_template(
        &spec
            .operations
            .iter()
            .map(render_rust_method)
            .collect::<String>(),
    )
}

fn render_rust_client_template(methods: &str) -> String {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/templates/rust_client.template"
    ))
    .replace("__GODSDK_METHODS__", methods)
}

fn render_rust_method(operation: &Operation) -> String {
    let parameters = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|parameter| format!(", {}: &str", rust_identifier(&parameter.name)))
        .collect::<String>();
    let mut path_format = operation.path.clone();
    let mut path_arguments = Vec::new();
    for parameter in operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
    {
        path_format = path_format.replace(&format!("{{{}}}", parameter.name), "{}");
        path_arguments.push(format!(
            "encode_path_segment({})",
            rust_identifier(&parameter.name)
        ));
    }
    let path = if path_arguments.is_empty() {
        format!("let path = {:?}.to_string();", path_format)
    } else {
        format!(
            "let path = format!({:?}, {});",
            path_format,
            path_arguments.join(", ")
        )
    };
    let response_type = rust_response_type(operation);
    let decode = if response_type == "String" {
        "Ok(body)".to_string()
    } else {
        format!(
            "serde_json::from_str::<{response_type}>(&body).map_err(|error| SdkError::Serialization(error.to_string()))"
        )
    };
    format!(
        "    pub async fn {}(&self{} ) -> Result<{}, SdkError> {{\n        {}\n        let body = self.request(Method::{} , &path).await?;\n        {}\n    }}\n\n",
        rust_identifier(&operation.operation_id),
        parameters,
        response_type,
        path,
        http_method_name(operation.method),
        decode
    )
}

pub(crate) fn render_config(spec: &ApiSpec) -> String {
    format!(
        "project:\n  name: {}-sdk\n  version: 0.1.0\n\nspec:\n  path: api/openapi.yaml\n  allow_remote_refs: false\n\ntargets: [rust, typescript]\n\nrelease:\n  enabled: true\n  crates_io:\n    enabled: true\n    package: {}-sdk\n  pypi:\n    enabled: false\n    package: {}-sdk\n  npm:\n    enabled: true\n    package: {}-sdk\n    publish_provenance: true\n  github:\n    enabled: true\n    workflow: release.yml\n",
        slug(&spec.title),
        slug(&spec.title),
        slug(&spec.title),
        slug(&spec.title),
    )
}

pub(crate) fn render_readme(spec: &ApiSpec) -> String {
    format!(
        "# {} SDK\n\nGenerated by GodSDK from OpenAPI {}.\n\nThe Rust SDK is async-first and uses Tokio. Run its integration tests with:\n\n```sh\ncargo test --manifest-path sdk/rust/Cargo.toml --locked\n```\n\nThe TypeScript SDK validates every response with Zod:\n\n```sh\ncd sdk/typescript\nnpm install\nnpm test\n```\n",
        spec.title, spec.openapi_version
    )
}

pub(crate) fn render_manifest(
    root: &Path,
    source: &str,
    files: &[PathBuf],
) -> Result<String, GenerationError> {
    let entries = files
        .iter()
        .filter(|path| path.to_string_lossy() != ".godsdk/manifest.json")
        .map(|path| {
            let contents = fs::read_to_string(root.join(path)).map_err(|error| {
                GenerationError::Write {
                    path: root.join(path),
                    message: error.to_string(),
                }
            })?;
            Ok(format!(
                "    {{\"path\":\"{}\",\"target\":\"{}\",\"template\":\"rust/vertical-slice\",\"sha256\":\"{}\"}}",
                path.to_string_lossy(),
                if path.starts_with("sdk/rust") { "rust" } else if path.starts_with("sdk/typescript") { "typescript" } else { "shared" },
                digest(&contents)
            ))
        })
        .collect::<Result<Vec<_>, GenerationError>>()?
        .join(",\n");
    Ok(format!(
        "{{\n  \"schema_version\": 1,\n  \"generator_version\": \"0.1.0\",\n  \"template_set_version\": \"0.1.0\",\n  \"input\": {{\"path\": \"api/openapi.yaml\", \"sha256\": \"{}\", \"resolved_refs\": []}},\n  \"targets\": [\"rust\", \"typescript\"],\n  \"governance\": {{\"godlint\": \"0.7.0\", \"godharness\": \"0.1.6\", \"bundle_version\": \"0.1.0\"}},\n  \"files\": [\n{}\n  ]\n}}\n",
        digest(source),
        entries
    ))
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
mod ingestion;
pub use ingestion::*;
