use std::path::PathBuf;

use godsdk_core::{GenerationRequest, Target, generate};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi")
        .join(name)
}

#[test]
fn rust_form_bodies_use_urlencoded_serialization() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let request = GenerationRequest::new(
        fixture("form-urlencoded-3.1.yaml"),
        output.path().join("generated"),
    )
    .with_targets([Target::Rust]);
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));
    let source =
        std::fs::read_to_string(request.output_path().join("sdk/rust/src/operations/mod.rs"))
            .unwrap_or_else(|error| panic!("generated operations are readable: {error}"));
    assert!(source.contains("form_request_body(request_body)"));
    let client = std::fs::read_to_string(request.output_path().join("sdk/rust/src/client/mod.rs"))
        .unwrap_or_else(|error| panic!("generated client is readable: {error}"));
    assert!(client.contains("application/x-www-form-urlencoded"));
}
