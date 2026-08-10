use std::path::PathBuf;
use std::process::Command;

use godsdk_core::{GenerationRequest, Target, generate};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi")
        .join(name)
}

fn generated_fixture(name: &str) -> (tempfile::TempDir, GenerationRequest) {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let request = GenerationRequest::new(fixture(name), output.path().join("generated"));
    (output, request)
}

#[test]
fn generates_a_compiling_rust_repository_skeleton() {
    let (_output, request) = generated_fixture("minimal-3.1.yaml");

    let result = match generate(&request) {
        Ok(result) => result,
        Err(error) => panic!("generation succeeds: {error}"),
    };

    assert!(
        result
            .files
            .iter()
            .any(|path| path == "sdk/rust/src/lib.rs")
    );
    assert!(request.output_path().join("sdk/rust/Cargo.toml").is_file());
    let cargo = std::fs::read_to_string(request.output_path().join("sdk/rust/Cargo.toml"))
        .unwrap_or_else(|error| panic!("generated cargo manifest is readable: {error}"));
    assert!(cargo.contains("reqwest"));
    assert!(cargo.contains("tokio"));
    let builder =
        std::fs::read_to_string(request.output_path().join("sdk/rust/src/client/builder.rs"))
            .unwrap_or_else(|error| panic!("generated builder is readable: {error}"));
    let operations =
        std::fs::read_to_string(request.output_path().join("sdk/rust/src/operations/mod.rs"))
            .unwrap_or_else(|error| panic!("generated operations are readable: {error}"));
    assert!(builder.contains("pub struct ClientBuilder"));
    assert!(operations.contains("pub async fn"));
    assert!(
        request
            .output_path()
            .join(".godsdk/manifest.json")
            .is_file()
    );
    let manifest = std::fs::read_to_string(request.output_path().join(".godsdk/manifest.json"))
        .unwrap_or_else(|error| panic!("manifest is readable: {error}"));
    assert!(serde_json::from_str::<serde_json::Value>(&manifest).is_ok());
}

#[test]
fn generated_client_calls_a_real_mock_server() {
    let (_output, request) = generated_fixture("minimal-3.1.yaml");
    match generate(&request) {
        Ok(_) => {}
        Err(error) => panic!("generation succeeds: {error}"),
    }

    let result = Command::new("cargo")
        .args(["test", "--manifest-path", "sdk/rust/Cargo.toml", "--locked"])
        .current_dir(request.output_path())
        .output()
        .unwrap_or_else(|error| panic!("generated cargo test runs: {error}"));
    assert!(
        result.status.success(),
        "generated test failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn generated_typed_fixture_contains_rust_models_and_typed_response() {
    let (_output, request) = generated_fixture("parameters-and-errors-3.1.yaml");
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));

    let models = std::fs::read_to_string(
        request
            .output_path()
            .join("sdk/rust/src/models/document.rs"),
    )
    .unwrap_or_else(|error| panic!("generated models are readable: {error}"));
    let problem =
        std::fs::read_to_string(request.output_path().join("sdk/rust/src/models/problem.rs"))
            .unwrap_or_else(|error| panic!("generated problem model is readable: {error}"));
    let client =
        std::fs::read_to_string(request.output_path().join("sdk/rust/src/operations/mod.rs"))
            .unwrap_or_else(|error| panic!("generated client is readable: {error}"));
    assert!(models.contains("pub struct Document"));
    assert!(problem.contains("pub struct Problem"));
    assert!(client.contains("Result<Document, SdkError>"));
}

#[test]
fn generated_typescript_uses_zod_and_typed_facade() {
    let (_output, request) = generated_fixture("minimal-3.1.yaml");
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));

    let root = request.output_path().join("sdk/typescript");
    let schemas = std::fs::read_to_string(root.join("src/schemas.ts"))
        .unwrap_or_else(|error| panic!("generated schemas are readable: {error}"));
    let types = std::fs::read_to_string(root.join("src/types.ts"))
        .unwrap_or_else(|error| panic!("generated types are readable: {error}"));
    let client = std::fs::read_to_string(root.join("src/index.ts"))
        .unwrap_or_else(|error| panic!("generated client is readable: {error}"));
    let package = std::fs::read_to_string(root.join("package.json"))
        .unwrap_or_else(|error| panic!("generated package metadata is readable: {error}"));
    assert!(
        root.join("native/Cargo.toml").is_file()
            && root.join("native/src/lib.rs").is_file()
            && root.join("tests/client.test.ts").is_file()
    );
    assert!(
        schemas.contains("import * as z from \"zod\";")
            && schemas.contains("PetSchema")
            && schemas.contains(".strict()")
    );
    assert!(types.contains("z.infer<typeof PetSchema>"));
    assert!(package.contains("\"targets\"") && package.contains("aarch64-apple-darwin"));
    assert!(
        client.contains("Promise<Pet>")
            && !client.contains("Promise<string>")
            && !client.contains("any")
    );
}

#[test]
fn generated_clients_use_explicit_rust_modules_and_esm_native_loading() {
    let (_output, request) = generated_fixture("minimal-3.1.yaml");
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));

    for path in [
        "sdk/rust/src/client/mod.rs",
        "sdk/rust/src/client/auth.rs",
        "sdk/rust/src/client/error.rs",
        "sdk/rust/src/client/retry.rs",
        "sdk/rust/src/client/transport.rs",
        "sdk/rust/src/models/mod.rs",
        "sdk/rust/src/models/pet.rs",
        "sdk/rust/src/operations/mod.rs",
    ] {
        assert!(request.output_path().join(path).is_file(), "missing {path}");
    }

    let native =
        std::fs::read_to_string(request.output_path().join("sdk/typescript/src/native.ts"))
            .unwrap_or_else(|error| panic!("generated native loader is readable: {error}"));
    assert!(native.contains("import binding from \"../native/index.js\";"));
    assert!(!native.contains("createRequire"));

    let godlint = request.output_path().join(".github/workflows/godlint.yml");
    let release = request.output_path().join(".github/workflows/release.yml");
    assert!(godlint.is_file() && release.is_file());
    let release = std::fs::read_to_string(release)
        .unwrap_or_else(|error| panic!("generated release workflow is readable: {error}"));
    assert!(release.contains("Publish Rust SDK to crates.io"));
    assert!(release.contains("Publish TypeScript SDK to npm"));
    assert!(!release.contains("pypi"));
}

#[test]
fn refuses_to_overwrite_a_non_empty_output_directory() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let generated = output.path().join("generated");
    std::fs::create_dir_all(&generated).unwrap_or_else(|error| panic!("output directory: {error}"));
    std::fs::write(generated.join("user.txt"), "keep me")
        .unwrap_or_else(|error| panic!("user file: {error}"));

    let request = GenerationRequest::new(fixture("minimal-3.1.yaml"), generated);
    assert!(generate(&request).is_err());
}

#[test]
fn regenerates_an_existing_godsdk_repository_without_unrelated_changes() {
    let (_output, request) = generated_fixture("minimal-3.1.yaml");
    generate(&request).unwrap_or_else(|error| panic!("initial generation succeeds: {error}"));
    let before = std::fs::read_to_string(request.output_path().join("README.md"))
        .unwrap_or_else(|error| panic!("generated README is readable: {error}"));

    generate(&request).unwrap_or_else(|error| panic!("repeat generation succeeds: {error}"));

    assert_eq!(
        before,
        std::fs::read_to_string(request.output_path().join("README.md"))
            .unwrap_or_else(|error| panic!("generated README is readable: {error}"))
    );
}

#[test]
fn refuses_to_overwrite_a_modified_generated_file() {
    let (_output, request) = generated_fixture("minimal-3.1.yaml");
    generate(&request).unwrap_or_else(|error| panic!("initial generation succeeds: {error}"));
    std::fs::write(request.output_path().join("README.md"), "user edits")
        .unwrap_or_else(|error| panic!("user edit is writable: {error}"));

    let error = match generate(&request) {
        Ok(_) => panic!("modified generated file must conflict"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("README.md"));
}

#[test]
fn dry_run_reports_changes_without_writing_output() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let request =
        GenerationRequest::new(fixture("minimal-3.1.yaml"), output.path().join("generated"))
            .dry_run();

    let result = generate(&request).unwrap_or_else(|error| panic!("dry run succeeds: {error}"));

    assert!(!result.files.is_empty());
    assert!(!request.output_path().exists());
}

#[test]
fn check_detects_generated_repository_drift() {
    let (_output, request) = generated_fixture("minimal-3.1.yaml");
    generate(&request).unwrap_or_else(|error| panic!("initial generation succeeds: {error}"));
    std::fs::write(request.output_path().join("README.md"), "drift")
        .unwrap_or_else(|error| panic!("drift is writable: {error}"));

    let error = match generate(&request.clone().check()) {
        Ok(_) => panic!("check must detect drift"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("README.md"));
}

#[test]
fn target_selection_can_generate_the_rust_core_without_typescript() {
    let (_output, request) = generated_fixture("minimal-3.1.yaml");
    let request = request.with_targets([Target::Rust]);

    generate(&request).unwrap_or_else(|error| panic!("selected generation succeeds: {error}"));

    assert!(request.output_path().join("sdk/rust").is_dir());
    assert!(!request.output_path().join("sdk/typescript").exists());
    let config = std::fs::read_to_string(request.output_path().join(".godsdk/config.yaml"))
        .unwrap_or_else(|error| panic!("generated config is readable: {error}"));
    assert!(config.contains("targets: [rust]"));
}

#[test]
fn python_target_generates_typed_pydantic_models_and_pyo3_binding() {
    let (_output, request) = generated_fixture("minimal-3.1.yaml");
    let request = request.with_targets([Target::Python]);

    generate(&request).unwrap_or_else(|error| panic!("python generation succeeds: {error}"));

    let models = std::fs::read_to_string(
        request
            .output_path()
            .join("sdk/python/minimal_fixture_api/models.py"),
    )
    .unwrap_or_else(|error| panic!("python models are readable: {error}"));
    let client = std::fs::read_to_string(
        request
            .output_path()
            .join("sdk/python/minimal_fixture_api/client.py"),
    )
    .unwrap_or_else(|error| panic!("python client is readable: {error}"));
    let native =
        std::fs::read_to_string(request.output_path().join("sdk/python/native/src/lib.rs"))
            .unwrap_or_else(|error| panic!("python native binding is readable: {error}"));
    let release =
        std::fs::read_to_string(request.output_path().join(".github/workflows/release.yml"))
            .unwrap_or_else(|error| panic!("generated release workflow is readable: {error}"));

    assert!(models.contains("BaseModel") && models.contains("ConfigDict"));
    assert!(!models.contains("Any"));
    assert!(client.contains("model_validate"));
    assert!(native.contains("#[pymodule]") && native.contains("RustClient"));
    assert!(release.contains("Publish Python SDK to PyPI"));
}

#[test]
fn equivalent_openapi_ordering_generates_identical_rust_sources() {
    let first = generate_source(
        r#"
openapi: 3.1.1
info: {title: Ordered, version: 1.0.0}
paths:
  /pets/{z}/{a}:
    get:
      operationId: getPet
      parameters:
        - {name: a, in: path, required: true}
        - {name: z, in: path, required: true}
      responses: {"200": {description: ok}}
"#,
    );
    let second = generate_source(
        r#"
openapi: 3.1.1
info: {title: Ordered, version: 1.0.0}
paths:
  /pets/{z}/{a}:
    get:
      operationId: getPet
      parameters:
        - {name: z, in: path, required: true}
        - {name: a, in: path, required: true}
      responses: {"200": {description: ok}}
"#,
    );

    assert_eq!(
        std::fs::read_to_string(first.join("sdk/rust/src/operations/mod.rs"))
            .unwrap_or_else(|error| panic!("first operations are readable: {error}")),
        std::fs::read_to_string(second.join("sdk/rust/src/operations/mod.rs"))
            .unwrap_or_else(|error| panic!("second operations are readable: {error}")),
    );
}

#[test]
fn generated_rust_sources_are_parseable_ast_files() {
    let (_output, request) = generated_fixture("parameters-and-errors-3.1.yaml");
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));
    let source_root = request.output_path().join("sdk/rust/src");
    for path in [
        "lib.rs",
        "client/mod.rs",
        "client/auth.rs",
        "client/builder.rs",
        "client/error.rs",
        "client/retry.rs",
        "client/transport.rs",
        "operations/mod.rs",
        "models/mod.rs",
        "models/document.rs",
        "models/problem.rs",
    ] {
        let source = std::fs::read_to_string(source_root.join(path))
            .unwrap_or_else(|error| panic!("generated source {path} is readable: {error}"));
        let parsed = syn::parse_file(&source);
        assert!(parsed.is_ok(), "generated source {path} is valid Rust");
    }
}

fn generate_source(source: &str) -> PathBuf {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let output_path = output.keep();
    let source_path = output_path.join("openapi.yaml");
    std::fs::write(&source_path, source)
        .unwrap_or_else(|error| panic!("source is writable: {error}"));
    let generated = output_path.join("generated");
    generate(&GenerationRequest::new(&source_path, &generated))
        .unwrap_or_else(|error| panic!("generation succeeds: {error}"));
    generated
}
