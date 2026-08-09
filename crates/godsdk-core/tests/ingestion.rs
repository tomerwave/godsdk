use std::path::PathBuf;

use godsdk_core::{ApiSpec, HttpMethod, IngestionError, ParameterLocation};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi")
        .join(name)
}

fn parse_fixture(name: &str) -> godsdk_core::ApiSpec {
    match ApiSpec::from_path(fixture(name)) {
        Ok(spec) => spec,
        Err(error) => panic!("fixture parses: {error}"),
    }
}

#[test]
fn normalizes_yaml_operations_in_stable_order() {
    let spec = parse_fixture("parameters-and-errors-3.1.yaml");

    assert_eq!(spec.openapi_version, "3.1.1");
    assert_eq!(spec.operations.len(), 1);
    let operation = &spec.operations[0];
    assert_eq!(operation.operation_id, "createDocument");
    assert_eq!(operation.method, HttpMethod::Post);
    assert!(operation.request_body);
    assert_eq!(operation.response_statuses, ["201", "400", "404"]);
    assert!(
        operation
            .parameters
            .iter()
            .any(|parameter| parameter.location == ParameterLocation::Cookie)
    );
}

#[test]
fn records_external_parameter_references_for_resolution() {
    let spec = parse_fixture("refs-3.1.yaml");

    assert_eq!(
        spec.references,
        ["./refs/models.yaml#/components/parameters/UserId"]
    );
}

#[test]
fn rejects_missing_path_parameters() {
    let error = match ApiSpec::from_path(fixture("invalid/missing-path-parameter-3.1.yaml")) {
        Ok(_) => panic!("invalid fixture must fail"),
        Err(error) => error,
    };

    assert_eq!(
        error,
        IngestionError::InvalidPathParameter {
            parameter: "user_id".to_string(),
            path: "/users/{user_id}".to_string(),
        }
    );
}

#[test]
fn rejects_duplicate_operation_ids() {
    let source = r#"
openapi: 3.1.1
info: {title: Duplicate IDs, version: 1.0.0}
paths:
  /a:
    get:
      operationId: duplicate
      responses: {"200": {description: ok}}
  /b:
    get:
      operationId: duplicate
      responses: {"200": {description: ok}}
"#;

    let error = match ApiSpec::parse(source) {
        Ok(_) => panic!("duplicate operation IDs must fail"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        IngestionError::DuplicateOperationId {
            operation_id: "duplicate".to_string()
        }
    );
}
