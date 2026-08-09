const MANIFEST_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/godsdk-manifest.schema.json"
));
const MINIMAL_SPEC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/openapi/minimal-3.1.yaml"
));
const CHANGED_SPEC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/openapi/minimal-3.1-changed-operation.yaml"
));

#[test]
fn manifest_schema_locks_required_repository_metadata() {
    for required in [
        "schema_version",
        "generator_version",
        "template_set_version",
        "input",
        "targets",
        "governance",
        "files",
    ] {
        assert!(MANIFEST_SCHEMA.contains(&format!("\"{required}\"")));
    }

    for target in ["rust", "python", "typescript"] {
        assert!(MANIFEST_SCHEMA.contains(&format!("\"{target}\"")));
    }
}

#[test]
fn fixtures_are_openapi_31_and_support_incremental_change_tests() {
    assert!(MINIMAL_SPEC.contains("openapi: 3.1.1"));
    assert!(MINIMAL_SPEC.contains("operationId: getPet"));
    assert!(CHANGED_SPEC.contains("operationId: getPet"));
    assert!(CHANGED_SPEC.contains("operationId: deletePet"));
}
