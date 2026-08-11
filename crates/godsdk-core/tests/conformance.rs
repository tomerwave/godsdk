use godsdk_core::{GenerationRequest, Target, generate};
use std::path::PathBuf;
use std::process::Command;

#[test]
fn all_generated_targets_expose_the_same_conformance_contract() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi/conformance-3.1.yaml");
    let request = GenerationRequest::new(&fixture, output.path().join("generated")).with_targets([
        Target::Rust,
        Target::TypeScript,
        Target::Python,
    ]);
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));

    let rust = read(&request, "sdk/rust/src/operations/mod.rs");
    assert!(rust.contains("HealthResponse") && rust.contains("ListItemsLimit"));
    assert!(rust.contains("Multipart") && rust.contains("binary_fields"));
    assert!(rust.contains("serde_urlencoded::to_string(request_body)"));
    let typescript = read(&request, "sdk/typescript/src/schemas.ts");
    assert!(typescript.contains("z.literal(\"ok\")") && typescript.contains("z.enum"));
    let python = read(
        &request,
        "sdk/python/cross_language_conformance_api/models.py",
    );
    assert!(python.contains("Literal[\"ok\"]") && python.contains("state: str"));
    let status = Command::new("cargo")
        .args(["check", "--locked"])
        .current_dir(request.output_path().join("sdk/rust"))
        .status()
        .unwrap_or_else(|error| panic!("generated Rust cargo check starts: {error}"));
    assert!(status.success(), "generated conformance Rust SDK compiles");
}

fn read(request: &GenerationRequest, path: &str) -> String {
    std::fs::read_to_string(request.output_path().join(path))
        .unwrap_or_else(|error| panic!("generated file {path} is readable: {error}"))
}
