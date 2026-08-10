use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use godsdk_core::{ApiSpec, GenerationRequest, IngestionError, ReferencePolicy, Schema, generate};
use sha2::{Digest, Sha256};

fn remote_document(body: &str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .unwrap_or_else(|error| panic!("test server binds: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("test server address is available: {error}"));
    let body = body.to_string();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener
            .accept()
            .unwrap_or_else(|error| panic!("test server accepts: {error}"));
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/yaml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .unwrap_or_else(|error| panic!("test server responds: {error}"));
    });
    (format!("http://{address}/models.yaml"), handle)
}

fn openapi_source(reference: &str) -> String {
    format!(
        "openapi: 3.1.1\ninfo: {{title: Remote, version: 1.0.0}}\npaths:\n  /pets:\n    get:\n      operationId: listPets\n      responses:\n        '200':\n          content:\n            application/json:\n              schema:\n                $ref: '{reference}#/components/schemas/Pet'\n"
    )
}

fn digest(body: &str) -> String {
    Sha256::digest(body.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn remote_references_require_an_allowlisted_host_and_pin() {
    let url = "http://127.0.0.1:1/models.yaml";
    let error =
        match ApiSpec::parse_with_policy(&openapi_source(url), None, &ReferencePolicy::default()) {
            Ok(_) => panic!("unallowlisted remote references must fail"),
            Err(error) => error,
        };
    assert!(matches!(
        error,
        IngestionError::ExternalReference { ref message, .. }
            if message.contains("not allowlisted")
    ));
}

#[test]
fn remote_references_resolve_with_a_matching_policy() {
    let body =
        "components:\n  schemas:\n    Pet: {type: object, properties: {id: {type: string}}}\n";
    let (url, server) = remote_document(body);
    let policy = ReferencePolicy::default()
        .allow_remote_host("127.0.0.1")
        .pin_remote_reference(&url, digest(body));
    let spec = ApiSpec::parse_with_policy(&openapi_source(&url), None, &policy)
        .unwrap_or_else(|error| panic!("pinned remote reference resolves: {error}"));
    server
        .join()
        .unwrap_or_else(|_| panic!("test server thread joins"));

    assert!(matches!(
        spec.schemas.get("Pet"),
        Some(Schema::Object { .. })
    ));
}

#[test]
fn remote_references_reject_a_checksum_mismatch() {
    let body =
        "components:\n  schemas:\n    Pet: {type: object, properties: {id: {type: string}}}\n";
    let (url, server) = remote_document(body);
    let policy = ReferencePolicy::default()
        .allow_remote_host("127.0.0.1")
        .pin_remote_reference(&url, "0".repeat(64));
    let error = match ApiSpec::parse_with_policy(&openapi_source(&url), None, &policy) {
        Ok(_) => panic!("checksum mismatches must fail"),
        Err(error) => error,
    };
    server
        .join()
        .unwrap_or_else(|_| panic!("test server thread joins"));

    assert!(matches!(
        error,
        IngestionError::ExternalReference { ref message, .. }
            if message.contains("does not match")
    ));
}

#[test]
fn generated_config_records_remote_reference_policy() {
    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory should exist: {error}"));
    let source = directory.path().join("openapi.yaml");
    std::fs::write(
        &source,
        "openapi: 3.1.1\ninfo: {title: Policy, version: 1.0.0}\npaths:\n  /pets:\n    get:\n      operationId: listPets\n      responses: {'200': {description: ok}}\n",
    )
    .unwrap_or_else(|error| panic!("source is writable: {error}"));
    let output = directory.path().join("generated");
    let policy = ReferencePolicy::default()
        .allow_remote_host("schemas.example.test")
        .pin_remote_reference("https://schemas.example.test/models.yaml", "a".repeat(64));
    generate(&GenerationRequest::new(&source, &output).with_reference_policy(policy))
        .unwrap_or_else(|error| panic!("generation succeeds: {error}"));

    let config = std::fs::read_to_string(output.join(".godsdk/config.yaml"))
        .unwrap_or_else(|error| panic!("generated config is readable: {error}"));
    assert!(config.contains("allow_remote_refs: true"));
    assert!(config.contains("schemas.example.test"));
    assert!(config.contains("https://schemas.example.test/models.yaml"));
    assert!(config.contains(&"a".repeat(64)));
}
