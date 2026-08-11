use godsdk_core::{GenerationRequest, Target, generate};
use std::path::PathBuf;

#[test]
fn generated_targets_validate_const_values() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi/const-schema-3.1.yaml");
    let request = GenerationRequest::new(&fixture, output.path().join("generated")).with_targets([
        Target::Rust,
        Target::TypeScript,
        Target::Python,
    ]);
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));
    let typescript =
        std::fs::read_to_string(request.output_path().join("sdk/typescript/src/schemas.ts"))
            .unwrap_or_else(|error| panic!("TypeScript schema source is readable: {error}"));
    assert!(typescript.contains("z.literal(\"ok\")"));
    let rust =
        std::fs::read_to_string(request.output_path().join("sdk/rust/src/models/version.rs"))
            .unwrap_or_else(|error| panic!("Rust schema source is readable: {error}"));
    assert!(rust.contains("pub struct Version(pub i64)"));
    assert!(rust.contains("impl TryFrom<i64> for Version"));
    let python_path = std::fs::read_dir(request.output_path().join("sdk/python"))
        .unwrap_or_else(|error| panic!("Python package directory is readable: {error}"))
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("models.py"))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("generated Python models are present"));
    let python = std::fs::read_to_string(python_path)
        .unwrap_or_else(|error| panic!("Python schema source is readable: {error}"));
    assert!(python.contains("Literal[1]"));
    assert!(python.contains("Literal[\"ok\"]"));
}
