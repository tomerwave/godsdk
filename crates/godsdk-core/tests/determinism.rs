use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use godsdk_core::{GenerationRequest, Target, generate};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi")
        .join(name)
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let entries = fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("generated directory is readable: {error}"));
        for entry in entries {
            let path = entry
                .unwrap_or_else(|error| panic!("generated directory entry is readable: {error}"))
                .path();
            if path.is_dir() {
                visit(root, &path, files);
                continue;
            }
            snapshot_file(root, &path, files);
        }
    }

    fn snapshot_file(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let relative = path
            .strip_prefix(root)
            .unwrap_or_else(|error| panic!("generated path is relative: {error}"))
            .to_path_buf();
        if relative.starts_with("sdk") {
            let contents = fs::read(path)
                .unwrap_or_else(|error| panic!("generated file is readable: {error}"));
            files.insert(relative, contents);
        }
    }

    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn repeated_generation_is_byte_identical_for_all_targets() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let first = output.path().join("first");
    let second = output.path().join("second");
    let first_request = GenerationRequest::new(fixture("parameters-and-errors-3.1.yaml"), &first)
        .with_targets([Target::Rust, Target::Python, Target::TypeScript]);
    let second_request = GenerationRequest::new(fixture("parameters-and-errors-3.1.yaml"), &second)
        .with_targets([Target::Rust, Target::Python, Target::TypeScript]);

    generate(&first_request).unwrap_or_else(|error| panic!("first generation succeeds: {error}"));
    generate(&second_request).unwrap_or_else(|error| panic!("second generation succeeds: {error}"));

    assert_eq!(snapshot(&first), snapshot(&second));
}

#[test]
fn equivalent_openapi_map_orders_generate_identical_targets() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let first = generate_for_fixture("deterministic-order-a.yaml", output.path().join("first"));
    let second = generate_for_fixture("deterministic-order-b.yaml", output.path().join("second"));

    assert_eq!(snapshot(&first), snapshot(&second));
}

fn generate_for_fixture(name: &str, output: PathBuf) -> PathBuf {
    let request = GenerationRequest::new(fixture(name), &output).with_targets([
        Target::Rust,
        Target::Python,
        Target::TypeScript,
    ]);
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));
    output
}
