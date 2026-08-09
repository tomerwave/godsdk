use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_reports_the_workspace_version() {
    let mut command = Command::cargo_bin("godsdk").expect("binary should build");

    command
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("godsdk 0.1.0"));
}

#[test]
fn help_lists_the_generate_command() {
    let mut command = Command::cargo_bin("godsdk").expect("binary should build");

    command
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("generate"));
}

#[test]
fn generate_requires_source_and_output() {
    let mut command = Command::cargo_bin("godsdk").expect("binary should build");

    command
        .arg("generate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required arguments"));
}

#[test]
fn generate_reports_not_implemented_without_creating_output() {
    let temp = tempfile::tempdir().expect("temporary directory should exist");
    let output = temp.path().join("generated");
    let mut command = Command::cargo_bin("godsdk").expect("binary should build");

    command
        .args(["generate", "--source", "spec.yaml", "--output"])
        .arg(&output)
        .assert()
        .failure()
        .stderr(predicate::eq("SDK generation is not implemented yet\n"));

    assert!(!output.exists());
}
