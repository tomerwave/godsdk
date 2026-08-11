use godsdk_core::{GenerationRequest, Target, generate};
use std::collections::BTreeMap;
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
    assert!(rust.contains("form_request_body(request_body)"));
    let typescript = read(&request, "sdk/typescript/src/schemas.ts");
    assert!(typescript.contains("z.literal(\"ok\")") && typescript.contains("z.enum"));
    let python = read(
        &request,
        "sdk/python/cross_language_conformance_api/models.py",
    );
    assert!(python.contains("Literal[\"ok\"]") && python.contains("state: str"));
    assert_generated_rust_compiles(&request);

    assert_generated_targets_are_deterministic(&fixture, &request);
}

fn assert_generated_targets_are_deterministic(
    fixture: &std::path::Path,
    first_request: &GenerationRequest,
) {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let request = GenerationRequest::new(fixture, output.path().join("generated")).with_targets([
        Target::Rust,
        Target::TypeScript,
        Target::Python,
    ]);
    generate(&request).unwrap_or_else(|error| panic!("second generation succeeds: {error}"));
    let first = snapshot(first_request);
    let second = snapshot(&request);
    assert_eq!(
        first.keys().collect::<Vec<_>>(),
        second.keys().collect::<Vec<_>>()
    );
    for (path, contents) in first {
        assert_eq!(contents, second[&path], "generated target changed: {path}");
    }
}

fn snapshot(request: &GenerationRequest) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for target in ["sdk/rust", "sdk/typescript", "sdk/python"] {
        let path = request.output_path().join(target);
        collect_files(request.output_path(), &path, &mut files);
    }
    files
}

fn collect_files(
    root: &std::path::Path,
    path: &std::path::Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) {
    for entry in std::fs::read_dir(path)
        .unwrap_or_else(|error| panic!("generated directory is readable: {error}"))
    {
        let entry = entry.unwrap_or_else(|error| panic!("generated entry is readable: {error}"));
        let path = entry.path();
        if path.is_dir() && is_build_output(&path) {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, files);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or_else(|error| panic!("generated path is relative: {error}"));
        let contents = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("generated file is readable: {error}"));
        files.insert(relative.to_string_lossy().into_owned(), contents);
    }
}

fn is_build_output(path: &std::path::Path) -> bool {
    path.file_name().is_some_and(|name| name == "target")
}

fn assert_generated_rust_compiles(request: &GenerationRequest) {
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
