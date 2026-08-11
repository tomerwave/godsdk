use godsdk_core::{GenerationRequest, generate};
use std::path::PathBuf;

#[test]
fn generated_rust_typed_scalars_validate_on_deserialization() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi/typed-enum-3.1.yaml");
    let request = GenerationRequest::new(&fixture, output.path().join("generated"));
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));
    let source =
        std::fs::read_to_string(request.output_path().join("sdk/rust/src/models/limit.rs"))
            .unwrap_or_else(|error| panic!("typed scalar source is readable: {error}"));
    assert!(source.contains("pub struct Limit(pub i64)"));
    assert!(source.contains("pub const VALUE_0: Self = Self(10)"));
    assert!(source.contains("impl TryFrom<i64> for Limit"));
    assert!(source.contains("impl<'de> serde::Deserialize<'de> for Limit"));
    let operation_source =
        std::fs::read_to_string(request.output_path().join("sdk/rust/src/operations/mod.rs"))
            .unwrap_or_else(|error| panic!("operation source is readable: {error}"));
    assert!(operation_source.contains("Limit"));
}
