use std::path::PathBuf;

use godsdk_core::{GenerationRequest, Target, generate};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi")
        .join(name)
}

fn assert_file_contains(root: &std::path::Path, relative: &str, needles: &[&str]) {
    let content = std::fs::read_to_string(root.join(relative))
        .unwrap_or_else(|error| panic!("generated file is readable: {relative}: {error}"));
    for needle in needles {
        assert!(
            content.contains(needle),
            "generated {relative} is missing {needle:?}"
        );
    }
}

#[test]
fn generated_targets_propagate_typed_request_contracts() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let request = GenerationRequest::new(
        fixture("parameters-and-errors-3.1.yaml"),
        output.path().join("generated"),
    )
    .with_targets([Target::Rust, Target::Python, Target::TypeScript]);
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));
    let root = request.output_path();
    assert!(root.is_dir());
    assert_file_contains(
        root,
        "sdk/rust/src/operations/mod.rs",
        &[
            "pub struct CreateDocumentRequest",
            "pub async fn create_document(",
            "dry_run: Option<bool>",
            "x_request_id: String",
            "request_body: DocumentInput",
            "serialize_parameter_value(",
            "serialize_cookie_value(",
            "serde_json::to_vec(&request_body)",
            "RequestBody::Bytes",
        ],
    );
    assert_file_contains(
        root,
        "sdk/typescript/src/index.ts",
        &[
            "requestBody: DocumentInput",
            "dryRun?: boolean",
            "DocumentInputSchema.parse(requestBody)",
        ],
    );
    assert_file_contains(
        root,
        "sdk/python/parameters_and_errors_fixture_api/client.py",
        &[
            "request_body: DocumentInput",
            "dry_run: bool | None = None",
            "request_body.model_dump_json()",
        ],
    );
}
