use std::path::{Path, PathBuf};

use crate::{GenerationError, Target, write_file};

pub(super) fn write_workflows(
    root: &Path,
    targets: &[Target],
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(
        root,
        ".github/workflows/godlint.yml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/godlint-workflow.yml"
        )),
        generated,
    )?;
    write_file(
        root,
        ".github/workflows/godharness.yml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/godharness-workflow.yml"
        )),
        generated,
    )?;
    let test_workflow = render_test_workflow(targets);
    write_file(
        root,
        ".github/workflows/test-generated.yml",
        &test_workflow,
        generated,
    )?;
    let release = render_release_workflow(targets);
    write_file(root, ".github/workflows/release.yml", &release, generated)
}

fn render_test_workflow(targets: &[Target]) -> String {
    let mut workflow = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/test-generated-workflow.yml"
    ))
    .to_string();
    for (target, enabled) in [
        ("rust", targets.contains(&Target::Rust)),
        ("python", targets.contains(&Target::Python)),
        ("typescript", targets.contains(&Target::TypeScript)),
    ] {
        workflow = filter_target_block(&workflow, target, enabled);
    }
    workflow
}

fn render_release_workflow(targets: &[Target]) -> String {
    let typescript = targets.contains(&Target::TypeScript);
    let python = targets.contains(&Target::Python);
    let mut release = filter_target_block(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/release-workflow.yml"
        )),
        "typescript",
        typescript,
    );
    let package_needs = [
        Some("crates"),
        typescript.then_some("npm"),
        python.then_some("pypi"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ");
    release = release.replace(
        "needs: [__GODSDK_GITHUB_NEEDS__]",
        &format!("needs: [{package_needs}]"),
    );
    if python {
        release.push_str(&filter_target_block(
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/python-release-workflow.yml"
            )),
            "python",
            true,
        ));
    }
    release
}

fn filter_target_block(source: &str, target: &str, enabled: bool) -> String {
    let mut filter = TargetBlockFilter::new(target, enabled);
    source
        .lines()
        .filter_map(|line| filter.accept(line))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

struct TargetBlockFilter {
    start: String,
    end: String,
    enabled: bool,
    inside: bool,
}

impl TargetBlockFilter {
    fn new(target: &str, enabled: bool) -> Self {
        Self {
            start: format!("# GODSDK_TARGET: {target}:start"),
            end: format!("# GODSDK_TARGET: {target}:end"),
            enabled,
            inside: false,
        }
    }

    fn accept<'a>(&mut self, line: &'a str) -> Option<&'a str> {
        if line.trim() == self.start {
            self.inside = true;
            return None;
        }
        if line.trim() == self.end {
            self.inside = false;
            return None;
        }
        (!self.inside || self.enabled).then_some(line)
    }
}
