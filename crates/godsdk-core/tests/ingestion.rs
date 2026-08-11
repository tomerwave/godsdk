use std::path::PathBuf;

use godsdk_core::{
    ApiIr, ApiSpec, HttpMethod, IngestionError, ParameterLocation, ParameterStyle, Schema,
    SecuritySchemeKind,
};

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
fn accepts_openapi_30_and_normalizes_nullable_schema_values() {
    let spec = parse_fixture("minimal-3.0.yaml");

    assert_eq!(spec.openapi_version, "3.0.3");
    assert!(matches!(
        spec.schemas.get("Pet"),
        Some(Schema::Object { properties, .. })
            if matches!(properties.get("nickname"), Some(Schema::Nullable(_)))
    ));
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
fn normalizes_typed_request_and_response_contracts() {
    let spec = parse_fixture("parameters-and-errors-3.1.yaml");
    let operation = &spec.operations[0];
    let request_body = operation
        .request_body_details
        .as_ref()
        .unwrap_or_else(|| panic!("request body details are present"));

    assert!(request_body.required);
    assert_eq!(request_body.content_type, "application/json");
    assert!(matches!(
        request_body.schema,
        Some(Schema::Reference(ref name)) if name == "DocumentInput"
    ));
    assert_eq!(
        operation.responses[0].content_type.as_deref(),
        Some("application/json")
    );
    assert_eq!(operation.responses[0].headers.len(), 1);
    assert_eq!(
        operation.responses[0].headers[0].name,
        "X-RateLimit-Remaining"
    );
    assert!(operation.responses[0].headers[0].required);
    assert!(matches!(
        operation.responses[1].schema,
        Some(Schema::Reference(ref name)) if name == "Problem"
    ));
}

#[test]
fn normalizes_named_schemas_and_typed_operation_shapes() {
    let spec = parse_fixture("parameters-and-errors-3.1.yaml");

    assert!(matches!(spec.schemas["Document"], Schema::AllOf(_)));
    assert!(matches!(spec.schemas["Problem"], Schema::Object { .. }));
    let operation = &spec.operations[0];
    assert!(
        matches!(operation.request_body_schema, Some(Schema::Reference(ref name)) if name == "DocumentInput")
    );
    assert!(matches!(
        operation
            .parameters
            .iter()
            .find(|parameter| parameter.name == "dry_run")
            .map(|parameter| &parameter.schema),
        Some(Schema::Boolean)
    ));
    assert!(matches!(
        operation
            .parameters
            .iter()
            .find(|parameter| parameter.name == "account_id")
            .map(|parameter| &parameter.schema),
        Some(Schema::String { .. })
    ));
    assert!(
        matches!(operation.responses[0].schema, Some(Schema::Reference(ref name)) if name == "Document")
    );
}

#[test]
fn normalizes_security_schemes_and_operation_requirements() {
    let spec = parse_fixture("security-3.1.yaml");

    assert!(matches!(
        spec.security_schemes["bearerAuth"].kind,
        SecuritySchemeKind::Http { ref scheme, ref bearer_format }
            if scheme == "bearer" && bearer_format.is_none()
    ));
    assert!(matches!(
        spec.security_schemes["apiKeyAuth"].kind,
        SecuritySchemeKind::ApiKey { ref name, location: ParameterLocation::Header }
            if name == "X-API-Key"
    ));
    assert!(matches!(
        spec.security_schemes["basicAuth"].kind,
        SecuritySchemeKind::Http { ref scheme, .. } if scheme == "basic"
    ));
    assert!(matches!(
        spec.security_schemes["oauth2"].kind,
        SecuritySchemeKind::OAuth2 { ref flows }
            if flows.len() == 1
                && flows[0].flow == "authorizationCode"
                && flows[0].scopes.get("read:resource").map(String::as_str) == Some("Read resource")
    ));

    let admin = &spec.operations[0];
    assert_eq!(admin.operation_id, "getAdmin");
    assert_eq!(admin.security.as_ref().map(Vec::len), Some(2));
    assert_eq!(
        admin
            .security
            .as_ref()
            .unwrap_or_else(|| panic!("admin security requirements are present"))[0]
            .schemes[0]
            .name,
        "apiKeyAuth"
    );
    assert!(
        spec.operations
            .iter()
            .find(|operation| operation.operation_id == "getPrivate")
            .unwrap_or_else(|| panic!("private operation exists"))
            .security
            .as_ref()
            .unwrap_or_else(|| panic!("private operation security is present"))[0]
            .schemes
            .iter()
            .any(|scheme| scheme.name == "bearerAuth")
    );
}

#[test]
fn preserves_explicitly_public_operations_and_root_security_defaults() {
    let spec = ApiSpec::parse(
        r#"
openapi: 3.1.1
info: {title: Security defaults, version: 1.0.0}
security:
  - bearerAuth: []
paths:
  /public:
    get:
      operationId: publicOperation
      security: []
      responses: {"200": {description: ok}}
  /inherited:
    get:
      operationId: inheritedOperation
      responses: {"200": {description: ok}}
components:
  securitySchemes:
    bearerAuth: {type: http, scheme: bearer}
"#,
    )
    .unwrap_or_else(|error| panic!("security defaults parse: {error}"));

    assert_eq!(
        spec.security
            .as_ref()
            .unwrap_or_else(|| panic!("root security is present"))
            .len(),
        1
    );
    assert_eq!(
        spec.operations
            .iter()
            .find(|operation| operation.operation_id == "publicOperation")
            .unwrap_or_else(|| panic!("public operation exists"))
            .security,
        Some(Vec::new())
    );
    assert_eq!(
        spec.operations
            .iter()
            .find(|operation| operation.operation_id == "inheritedOperation")
            .unwrap_or_else(|| panic!("inherited operation exists"))
            .security,
        None
    );
}

#[test]
fn rejects_security_requirements_for_unknown_schemes() {
    let error = match ApiSpec::parse(
        r#"
openapi: 3.1.1
info: {title: Invalid security, version: 1.0.0}
paths:
  /private:
    get:
      operationId: privateOperation
      security: [{missingAuth: []}]
      responses: {"200": {description: ok}}
"#,
    ) {
        Ok(_) => panic!("unknown security schemes must fail"),
        Err(error) => error,
    };

    assert!(
        matches!(error, IngestionError::UnknownSecurityScheme { ref name } if name == "missingAuth")
    );
}

#[test]
fn preserves_nullable_arrays_and_discriminated_unions() {
    let spec = parse_fixture("schemas-composition-3.1.yaml");

    assert!(matches!(spec.schemas["Page"], Schema::Object { .. }));
    assert!(matches!(spec.schemas["Item"], Schema::OneOf(_)));
    assert!(matches!(spec.schemas["ItemBase"], Schema::Object { .. }));
}

#[test]
fn records_external_parameter_references_for_resolution() {
    let spec = parse_fixture("refs-3.1.yaml");

    assert_eq!(
        spec.references,
        [
            "./refs/models.yaml#/components/parameters/UserId",
            "./refs/models.yaml#/components/schemas/User"
        ]
    );
}

#[test]
fn resolves_external_parameters_and_schemas_when_loading_from_a_file() {
    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory should exist: {error}"));
    std::fs::write(
        directory.path().join("models.yaml"),
        "components:\n  parameters:\n    UserId:\n      name: user_id\n      in: path\n      required: true\n      schema: {type: string}\n  schemas:\n    User: {type: object, properties: {id: {type: string}, address: {$ref: './nested.yaml#/components/schemas/Address'}}}\n",
    )
    .unwrap_or_else(|error| panic!("models document is writable: {error}"));
    std::fs::write(
        directory.path().join("nested.yaml"),
        "components:\n  schemas:\n    Address: {type: object, properties: {city: {type: string}}}\n",
    )
    .unwrap_or_else(|error| panic!("nested document is writable: {error}"));
    let entry = directory.path().join("openapi.yaml");
    std::fs::write(
        &entry,
        "openapi: 3.1.1\ninfo: {title: External, version: 1.0.0}\npaths:\n  /users/{user_id}:\n    get:\n      operationId: getUser\n      parameters:\n        - $ref: './models.yaml#/components/parameters/UserId'\n      responses:\n        '200':\n          content:\n            application/json:\n              schema:\n                $ref: './models.yaml#/components/schemas/User'\n",
    )
    .unwrap_or_else(|error| panic!("entry document is writable: {error}"));

    let spec = ApiSpec::from_path(&entry)
        .unwrap_or_else(|error| panic!("external references resolve: {error}"));

    assert!(spec.schemas.contains_key("User"));
    assert!(spec.schemas.contains_key("Address"));
    assert_eq!(spec.operations[0].parameters[0].name, "user_id");
    assert!(matches!(
        spec.operations[0].responses[0].schema,
        Some(Schema::Reference(ref name)) if name == "User"
    ));
}

#[test]
fn rejects_remote_external_references_when_loading_from_a_file() {
    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory should exist: {error}"));
    let entry = directory.path().join("openapi.yaml");
    std::fs::write(
        &entry,
        "openapi: 3.1.1\ninfo: {title: Remote, version: 1.0.0}\npaths:\n  /pets:\n    get:\n      operationId: listPets\n      responses:\n        '200':\n          content:\n            application/json:\n              schema:\n                $ref: 'https://example.test/models.yaml#/components/schemas/Pet'\n",
    )
    .unwrap_or_else(|error| panic!("entry document is writable: {error}"));

    let error = match ApiSpec::from_path(&entry) {
        Ok(_) => panic!("remote references must be rejected"),
        Err(error) => error,
    };

    assert!(matches!(error, IngestionError::ExternalReference { .. }));
}

#[test]
fn normalized_documents_expose_a_language_neutral_ir_boundary() {
    let spec: ApiIr = parse_fixture("minimal-3.1.yaml");

    let parsed = match ApiSpec::from_path(fixture("minimal-3.1.yaml")) {
        Ok(parsed) => parsed,
        Err(error) => panic!("fixture parses: {error}"),
    };
    assert_eq!(spec, parsed);
    assert_eq!(spec.operations[0].canonical_key(), "GET /pets/{pet_id}");
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

#[test]
fn canonicalizes_parameter_order_independently_of_document_order() {
    let first = ApiSpec::parse(
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
    )
    .unwrap_or_else(|error| panic!("first document parses: {error}"));
    let second = ApiSpec::parse(
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
    )
    .unwrap_or_else(|error| panic!("second document parses: {error}"));

    assert_eq!(
        first.operations[0].parameters,
        second.operations[0].parameters
    );
}

#[test]
fn normalizes_parameter_serialization_defaults_and_explicit_styles() {
    let spec = ApiSpec::parse(
        r#"
openapi: 3.1.1
info: {title: Serialization, version: 1.0.0}
paths:
  /pets/{pet_id}:
    get:
      operationId: getPet
      parameters:
        - {name: pet_id, in: path, required: true, schema: {type: string}}
        - {name: tags, in: query, schema: {type: array, items: {type: string}}, style: pipeDelimited, explode: false}
        - {name: filter, in: query, schema: {type: object, properties: {name: {type: string}}}, style: deepObject}
      responses: {"200": {description: ok}}
"#,
    )
    .unwrap_or_else(|error| panic!("document parses: {error}"));

    let parameters = &spec.operations[0].parameters;
    let parameter = |name| {
        parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .unwrap_or_else(|| panic!("parameter {name} is present"))
    };
    assert_eq!(
        parameter("pet_id").serialization.style,
        ParameterStyle::Simple
    );
    assert!(!parameter("pet_id").serialization.explode);
    assert_eq!(
        parameter("tags").serialization.style,
        ParameterStyle::PipeDelimited
    );
    assert!(!parameter("tags").serialization.explode);
    assert_eq!(
        parameter("filter").serialization.style,
        ParameterStyle::DeepObject
    );
    assert!(parameter("filter").serialization.explode);
}

#[test]
fn rejects_parameter_serialization_styles_for_their_location() {
    let error = match ApiSpec::parse(
        r#"
openapi: 3.1.1
info: {title: Invalid Serialization, version: 1.0.0}
paths:
  /pets:
    get:
      operationId: listPets
      parameters:
        - {name: filter, in: header, schema: {type: string}, style: deepObject}
      responses: {"200": {description: ok}}
"#,
    ) {
        Ok(_) => panic!("invalid parameter serialization was accepted"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        IngestionError::UnsupportedParameterStyle { .. }
    ));
}
