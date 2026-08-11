use godsdk_core::Schema;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi")
        .join(name)
}
fn parse(name: &str) -> godsdk_core::ApiSpec {
    godsdk_core::ApiSpec::from_path(fixture(name))
        .unwrap_or_else(|error| panic!("fixture parses: {error}"))
}

#[test]
fn accepts_yaml_null_schema_types_from_openapi_31() {
    let spec = parse("null-schema-3.1.yaml");
    assert!(
        matches!(spec.operations.iter().find(|operation| operation.operation_id == "getNullable").and_then(|operation| operation.responses[0].schema.as_ref()), Some(Schema::Nullable(inner)) if matches!(inner.as_ref(), Schema::String { .. }))
    );
    assert!(matches!(
        spec.operations
            .iter()
            .find(|operation| operation.operation_id == "createNull")
            .and_then(|operation| operation.responses[0].schema.as_ref()),
        Some(Schema::Null)
    ));
}

#[test]
fn infers_stable_operation_ids_when_openapi_omits_them() {
    let spec = parse("missing-operation-id-3.1.yaml");
    assert_eq!(
        spec.operations
            .iter()
            .map(|operation| operation.operation_id.as_str())
            .collect::<Vec<_>>(),
        ["postUsers", "deleteUser", "getUsersUserId"]
    );
}

#[test]
fn preserves_untyped_json_schemas_as_explicit_any_values() {
    assert!(matches!(
        parse("untyped-schema-3.1.yaml").operations[0].responses[0].schema,
        Some(Schema::Any)
    ));
}

#[test]
fn preserves_typed_non_string_enums() {
    assert!(
        matches!(&parse("typed-enum-3.1.yaml").operations[0].parameters[0].schema, Schema::TypedEnum { base, values } if matches!(base.as_ref(), Schema::Integer { .. }) && values.len() == 3)
    );
}

#[test]
fn preserves_multipart_requests_and_binary_responses() {
    let operation = &parse("multipart-binary-3.1.yaml").operations[0];
    assert_eq!(
        operation
            .request_body_details
            .as_ref()
            .map(|body| body.content_type.as_str()),
        Some("multipart/form-data")
    );
    assert_eq!(
        operation.responses[0].content_type.as_deref(),
        Some("application/octet-stream")
    );
    assert!(
        matches!(operation.responses[0].schema, Some(Schema::String { format: Some(ref format) }) if format == "binary")
    );
}
