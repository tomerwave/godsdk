use std::process::Output;

use assert_cmd::Command;

fn run(args: &[&str]) -> Output {
    let mut command =
        Command::cargo_bin("godsdk").unwrap_or_else(|error| panic!("binary should build: {error}"));
    command
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("command should run: {error}"))
}

#[test]
fn version_reports_the_workspace_version() {
    let output = run(&["--version"]);

    assert!(output.status.success());
    let expected = format!("godsdk {}", env!("CARGO_PKG_VERSION"));
    assert!(String::from_utf8_lossy(&output.stdout).contains(&expected));
}

#[test]
fn help_lists_the_generate_command() {
    let output = run(&["--help"]);

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("generate"));
}

#[test]
fn generate_requires_source_and_output() {
    let output = run(&["generate"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("required arguments"));
}

#[test]
fn generate_creates_a_rust_repository() {
    let temp = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory should exist: {error}"));
    let output_path = temp.path().join("generated");
    let source = format!(
        "{}/../../fixtures/openapi/minimal-3.1.yaml",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = run(&[
        "generate",
        "--source",
        &source,
        "--output",
        output_path.to_str().unwrap_or("generated"),
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_path.join("sdk/rust/src/lib.rs").is_file());
}
