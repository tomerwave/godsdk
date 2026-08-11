use godsdk_core::{GenerationRequest, generate};
use std::path::PathBuf;

fn generate_source(source: &str) -> PathBuf {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let output_path = output.keep();
    let source_path = output_path.join("openapi.yaml");
    std::fs::write(&source_path, source)
        .unwrap_or_else(|error| panic!("source is writable: {error}"));
    let generated = output_path.join("generated");
    generate(&GenerationRequest::new(&source_path, &generated))
        .unwrap_or_else(|error| panic!("generation succeeds: {error}"));
    generated
}

#[test]
fn equivalent_openapi_ordering_generates_identical_rust_sources() {
    let first = generate_source(
        "openapi: 3.1.1\ninfo: {title: Ordered, version: 1.0.0}\npaths:\n  /pets/{z}/{a}:\n    get:\n      operationId: getPet\n      parameters:\n        - {name: a, in: path, required: true}\n        - {name: z, in: path, required: true}\n      responses: {\"200\": {description: ok}}\n",
    );
    let second = generate_source(
        "openapi: 3.1.1\ninfo: {title: Ordered, version: 1.0.0}\npaths:\n  /pets/{z}/{a}:\n    get:\n      operationId: getPet\n      parameters:\n        - {name: z, in: path, required: true}\n        - {name: a, in: path, required: true}\n      responses: {\"200\": {description: ok}}\n",
    );
    let read = |path: PathBuf| {
        std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("generated source is readable: {error}"))
    };
    assert_eq!(
        read(first.join("sdk/rust/src/operations/mod.rs")),
        read(second.join("sdk/rust/src/operations/mod.rs"))
    );
}

#[test]
fn generated_rust_sources_are_parseable_ast_files() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let request = GenerationRequest::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/openapi/parameters-and-errors-3.1.yaml"),
        output.path().join("generated"),
    );
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));
    for path in [
        "lib.rs",
        "client/mod.rs",
        "client/auth.rs",
        "client/builder.rs",
        "client/error.rs",
        "client/retry.rs",
        "client/transport.rs",
        "operations/mod.rs",
        "models/mod.rs",
        "models/document.rs",
        "models/problem.rs",
    ] {
        let source = std::fs::read_to_string(request.output_path().join("sdk/rust/src").join(path))
            .unwrap_or_else(|error| panic!("generated source is readable: {error}"));
        assert!(
            syn::parse_file(&source).is_ok(),
            "generated source {path} is valid Rust"
        );
    }
}
