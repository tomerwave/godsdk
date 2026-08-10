use std::path::PathBuf;

use godsdk_core::{GenerationRequest, Target, generate};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/openapi")
        .join(name)
}

#[test]
fn generated_repository_includes_self_contained_governance_and_test_workflows() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let request =
        GenerationRequest::new(fixture("minimal-3.1.yaml"), output.path().join("generated"))
            .with_targets([Target::Rust, Target::Python, Target::TypeScript]);
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));

    for path in [
        ".github/workflows/godharness.yml",
        ".github/workflows/test-generated.yml",
        ".github/godsuite-versions.yml",
        "scripts/install_godlint.sh",
        "scripts/install_godharness.sh",
    ] {
        assert!(request.output_path().join(path).is_file(), "missing {path}");
    }
    let godharness = std::fs::read_to_string(
        request
            .output_path()
            .join(".github/workflows/godharness.yml"),
    )
    .unwrap_or_else(|error| panic!("generated Godharness workflow is readable: {error}"));
    let tests = std::fs::read_to_string(
        request
            .output_path()
            .join(".github/workflows/test-generated.yml"),
    )
    .unwrap_or_else(|error| panic!("generated target workflow is readable: {error}"));
    assert!(godharness.contains("bash scripts/install_godharness.sh"));
    assert!(
        tests.contains("name: Test Rust SDK")
            && tests.contains("name: Test Python SDK")
            && tests.contains("name: Test TypeScript SDK")
    );
}

#[test]
fn generated_repository_includes_godharness_adapter_wiring() {
    let output = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
    let request =
        GenerationRequest::new(fixture("minimal-3.1.yaml"), output.path().join("generated"));
    generate(&request).unwrap_or_else(|error| panic!("generation succeeds: {error}"));

    let codex = std::fs::read_to_string(request.output_path().join(".codex/hooks.json"))
        .unwrap_or_else(|error| panic!("Codex hooks are readable: {error}"));
    let claude = std::fs::read_to_string(request.output_path().join(".claude/settings.json"))
        .unwrap_or_else(|error| panic!("Claude settings are readable: {error}"));
    assert!(codex.contains("adapter-hook codex"));
    assert!(claude.contains("adapter-hook claude-code"));
    assert!(request.output_path().join(".agents/README.md").is_file());
    assert!(
        request
            .output_path()
            .join("docs/godharness/example.md")
            .is_file()
    );
}
