use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

mod code_writer;
mod generation;
mod generation_transaction;
mod governance;
mod ingestion_contracts;
mod ingestion_refs;
mod ingestion_security;
mod ir;
pub(crate) mod rust_ast;
mod schema;
mod typescript;
mod workflow;
pub use generation::generate;
pub use ir::{
    ApiIr, HttpMethod, OAuth2Flow, Operation, Parameter, ParameterLocation, ParameterSerialization,
    ParameterStyle, RequestBody, RequiredSecurityScheme, Response, ResponseHeader,
    SecurityRequirement, SecurityScheme, SecuritySchemeKind,
};
pub(crate) use rust_ast::render_files as render_rust_files;
pub(crate) use rust_ast::render_mock_test as render_rust_mock_test;
pub(crate) use rust_ast::rust_identifier;
pub use schema::Schema;
pub type ApiSpec = ApiIr;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationRequest {
    pub source: PathBuf,
    pub output: PathBuf,
    pub mode: GenerationMode,
    pub targets: Vec<Target>,
    pub prune: bool,
    pub reference_policy: ReferencePolicy,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferencePolicy {
    allowed_hosts: BTreeSet<String>,
    sha256_pins: BTreeMap<String, String>,
}

impl ReferencePolicy {
    pub fn allow_remote_host(mut self, host: impl Into<String>) -> Self {
        self.allowed_hosts.insert(host.into().to_ascii_lowercase());
        self
    }

    pub fn pin_remote_reference(
        mut self,
        url: impl Into<String>,
        sha256: impl Into<String>,
    ) -> Self {
        self.sha256_pins.insert(url.into(), sha256.into());
        self
    }

    pub(crate) fn allows_host(&self, host: &str) -> bool {
        self.allowed_hosts.contains(&host.to_ascii_lowercase())
    }

    pub(crate) fn checksum_for(&self, url: &str) -> Option<&str> {
        self.sha256_pins.get(url).map(String::as_str)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Rust,
    Python,
    TypeScript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationMode {
    Write,
    DryRun,
    Check,
}

impl GenerationRequest {
    pub fn new(source: impl Into<PathBuf>, output: impl Into<PathBuf>) -> Self {
        Self {
            source: source.into(),
            output: output.into(),
            mode: GenerationMode::Write,
            targets: vec![Target::Rust, Target::TypeScript],
            prune: false,
            reference_policy: ReferencePolicy::default(),
        }
    }

    pub fn source_path(&self) -> &Path {
        &self.source
    }

    pub fn output_path(&self) -> &Path {
        &self.output
    }

    pub fn dry_run(mut self) -> Self {
        self.mode = GenerationMode::DryRun;
        self
    }

    pub fn check(mut self) -> Self {
        self.mode = GenerationMode::Check;
        self
    }

    pub fn prune(mut self) -> Self {
        self.prune = true;
        self
    }

    pub fn with_targets<I>(mut self, targets: I) -> Self
    where
        I: IntoIterator<Item = Target>,
    {
        self.targets = targets.into_iter().collect();
        if !self.targets.contains(&Target::Rust) {
            self.targets.push(Target::Rust);
        }
        self.targets.sort();
        self.targets.dedup();
        self
    }

    pub fn with_reference_policy(mut self, reference_policy: ReferencePolicy) -> Self {
        self.reference_policy = reference_policy;
        self
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
    #[error("could not read existing GodSDK manifest: {0}")]
    Manifest(String),
    #[error("generated file was modified outside GodSDK: {0}")]
    GeneratedFileConflict(PathBuf),
    #[error("generated repository is out of date: {0:?}")]
    OutOfDate(Vec<PathBuf>),
    #[error("unknown generation target {0}; expected rust, python, or typescript")]
    InvalidTarget(String),
    #[error("invalid remote reference policy: {0}")]
    InvalidReferencePolicy(String),
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
    write_if_changed(&path, contents).map_err(|error| GenerationError::Write {
        path: path.clone(),
        message: error.to_string(),
    })?;
    generated.push(PathBuf::from(relative));
    Ok(())
}

fn write_if_changed(path: &Path, contents: &str) -> Result<(), std::io::Error> {
    let unchanged = fs::read(path)
        .map(|existing| existing == contents.as_bytes())
        .unwrap_or(false);
    if !unchanged {
        fs::write(path, contents)?;
    }
    Ok(())
}

fn digest(contents: &str) -> String {
    Sha256::digest(contents.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn render_rust_cargo(spec: &ApiIr) -> String {
    format!(
        "[package]\nname = \"{}-sdk\"\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.97\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[features]\ndefault = [\"rustls-tls\"]\nrustls-tls = [\"reqwest/rustls-tls\"]\nnative-tls = [\"reqwest/native-tls\"]\ntracing = [\"dep:tracing\"]\n\n[dependencies]\npercent-encoding = \"2\"\nreqwest = {{ version = \"0.12\", default-features = false, features = [\"json\", \"multipart\"] }}\nserde = {{ version = \"1\", features = [\"derive\"] }}\nserde_json = \"1\"\nserde_urlencoded = \"0.7\"\nthiserror = \"2\"\ntokio = {{ version = \"1\", features = [\"macros\", \"rt\", \"time\"] }}\ntracing = {{ version = \"0.1\", optional = true }}\nurl = \"2\"\n",
        slug(&spec.title)
    )
}

pub(crate) fn render_config(
    spec: &ApiIr,
    targets: &[Target],
    reference_policy: &ReferencePolicy,
) -> String {
    let target_names = targets
        .iter()
        .map(|target| target.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "project:\n  name: {}-sdk\n  version: 0.1.0\n\nspec:\n  path: api/openapi.yaml\n  allow_remote_refs: {}\n  remote_ref_hosts: [{}]\n  remote_ref_sha256: {}\ntargets: [{}]\n\nrelease:\n  enabled: true\n  crates_io:\n    enabled: true\n    package: {}-sdk\n  pypi:\n    enabled: {}\n    package: {}-sdk\n  npm:\n    enabled: {}\n    package: {}-sdk\n    publish_provenance: true\n  github:\n    enabled: true\n    workflow: release.yml\n",
        slug(&spec.title),
        !reference_policy.allowed_hosts.is_empty() && !reference_policy.sha256_pins.is_empty(),
        reference_policy
            .allowed_hosts
            .iter()
            .map(|host| format!("\"{host}\""))
            .collect::<Vec<_>>()
            .join(", "),
        render_reference_pins(reference_policy),
        target_names,
        slug(&spec.title),
        targets.contains(&Target::Python),
        slug(&spec.title),
        targets.contains(&Target::TypeScript),
        slug(&spec.title),
    )
}

fn render_reference_pins(policy: &ReferencePolicy) -> String {
    let pins = policy
        .sha256_pins
        .iter()
        .map(|(url, checksum)| format!("    \"{url}\": \"{checksum}\"\n"))
        .collect::<String>();
    if pins.is_empty() {
        "{}".to_string()
    } else {
        format!("\n{pins}")
    }
}

pub(crate) fn render_readme(spec: &ApiIr, targets: &[Target]) -> String {
    let mut readme = format!(
        "# {} SDK\n\nGenerated by GodSDK from OpenAPI {}.\n\nThe Rust SDK is async-first and uses Tokio. Run its integration tests with:\n\n```sh\ncargo test --manifest-path sdk/rust/Cargo.toml --locked\n```\n",
        spec.title, spec.openapi_version
    );
    if targets.contains(&Target::TypeScript) {
        readme.push_str(
            "\nThe TypeScript SDK validates every response with Zod:\n\n```sh\ncd sdk/typescript\nnpm install\nnpm test\n```\n",
        );
    }
    if targets.contains(&Target::Python) {
        readme.push_str(
            "\nThe Python SDK exposes typed Pydantic models over the Rust client:\n\n```sh\ncd sdk/python\npython -m pip install maturin\nmaturin develop --manifest-path native/Cargo.toml\n```\n",
        );
    }
    readme.push_str(
        "\n## Generated automation\n\n- `.github/workflows/godlint.yml` checks pull requests with Godlint.\n- `.github/workflows/release.yml` publishes the selected SDK targets and creates the GitHub release when a `v*` tag is pushed.\n\nConfigure the trusted publishers listed in `NEEDS-YOUR-ATTENTION.md`, then publish with:\n\n```sh\ngit tag v0.1.0\ngit push origin v0.1.0\n```\n",
    );
    readme
}

pub(crate) fn render_manifest(
    root: &Path,
    source: &str,
    targets: &[Target],
    files: &[PathBuf],
) -> Result<String, GenerationError> {
    let mut paths = files
        .iter()
        .filter(|path| path.to_string_lossy() != ".godsdk/manifest.json")
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    let entries = paths
        .iter()
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
                target_for_path(path),
                digest(&contents)
            ))
        })
        .collect::<Result<Vec<_>, GenerationError>>()?
        .join(",\n");
    Ok(format!(
        "{{\n  \"schema_version\": 1,\n  \"generator_version\": \"0.1.0\",\n  \"template_set_version\": \"0.1.0\",\n  \"input\": {{\"path\": \"api/openapi.yaml\", \"sha256\": \"{}\", \"resolved_refs\": []}},\n  \"targets\": [{}],\n  \"governance\": {{\"godlint\": \"0.7.0\", \"godharness\": \"0.1.6\", \"bundle_version\": \"0.1.0\"}},\n  \"files\": [\n{}\n  ]\n}}\n",
        digest(source),
        targets
            .iter()
            .map(|target| format!("\"{}\"", target.as_str()))
            .collect::<Vec<_>>()
            .join(", "),
        entries
    ))
}

impl Target {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
        }
    }
}

fn target_for_path(path: &Path) -> &'static str {
    if path.starts_with("sdk/rust") {
        "rust"
    } else if path.starts_with("sdk/python") {
        "python"
    } else if path.starts_with("sdk/typescript") {
        "typescript"
    } else {
        "shared"
    }
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

#[cfg(test)]
mod tests {
    use super::write_if_changed;

    #[test]
    fn write_if_changed_keeps_existing_bytes_for_stable_content() {
        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
        let path = directory.path().join("generated.rs");

        assert!(write_if_changed(&path, "generated").is_ok());
        assert!(write_if_changed(&path, "generated").is_ok());
        assert_eq!(
            std::fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("generated file is readable: {error}")),
            "generated"
        );
    }
}
mod ingestion;
mod python;
pub use ingestion::*;
