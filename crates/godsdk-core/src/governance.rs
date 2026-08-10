use std::path::{Path, PathBuf};

use crate::{GenerationError, write_file};

pub(super) fn write_files(
    root: &Path,
    generated: &mut Vec<PathBuf>,
) -> Result<(), GenerationError> {
    write_file(root, "godlint.yaml", &render_godlint(), generated)?;
    write_file(
        root,
        "godharness.yaml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/godharness.yaml"
        )),
        generated,
    )?;
    write_bundle_files(root, generated)?;
    write_adapter_files(root, generated)
}

fn write_bundle_files(root: &Path, generated: &mut Vec<PathBuf>) -> Result<(), GenerationError> {
    write_file(
        root,
        ".github/godsuite-versions.yml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/godsuite-versions.yml"
        )),
        generated,
    )?;
    write_file(
        root,
        "scripts/install_godlint.sh",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/install_godlint.sh"
        )),
        generated,
    )?;
    write_file(
        root,
        "scripts/install_godharness.sh",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/install_godharness.sh"
        )),
        generated,
    )
}

fn write_adapter_files(root: &Path, generated: &mut Vec<PathBuf>) -> Result<(), GenerationError> {
    write_file(
        root,
        ".codex/hooks.json",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/codex-hooks.json"
        )),
        generated,
    )?;
    write_file(
        root,
        ".claude/settings.json",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/claude-settings.json"
        )),
        generated,
    )?;
    write_file(
        root,
        ".agents/README.md",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/agents-readme.md"
        )),
        generated,
    )?;
    write_file(
        root,
        "docs/godharness/example.md",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/godharness-example.md"
        )),
        generated,
    )
}

fn render_godlint() -> String {
    format!(
        "{}\nexclude:\n  - sdk/typescript/native/index.js\n  - sdk/typescript/native/index.d.ts\n  - sdk/typescript/native/*.node\n  - sdk/typescript/native/target/**\n  - sdk/typescript/node_modules/**\n",
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/godlint.yaml"))
    )
}
