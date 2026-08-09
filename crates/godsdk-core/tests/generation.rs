use std::path::PathBuf;
use std::process::Command;

use godsdk_core::{GenerationRequest, generate};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi")
        .join(name)
}

#[test]
fn generates_a_compiling_rust_repository_skeleton() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let request =
        GenerationRequest::new(fixture("minimal-3.1.yaml"), output.path().join("generated"));

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
    let client = std::fs::read_to_string(request.output_path().join("sdk/rust/src/lib.rs"))
        .unwrap_or_else(|error| panic!("generated client is readable: {error}"));
    assert!(client.contains("pub struct ClientBuilder"));
    assert!(client.contains("pub async fn"));
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
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let request =
        GenerationRequest::new(fixture("minimal-3.1.yaml"), output.path().join("generated"));
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
fn refuses_to_overwrite_a_non_empty_output_directory() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let generated = output.path().join("generated");
    std::fs::create_dir_all(&generated).unwrap_or_else(|error| panic!("output directory: {error}"));
    std::fs::write(generated.join("user.txt"), "keep me")
        .unwrap_or_else(|error| panic!("user file: {error}"));

    let request = GenerationRequest::new(fixture("minimal-3.1.yaml"), generated);
    assert!(generate(&request).is_err());
}
