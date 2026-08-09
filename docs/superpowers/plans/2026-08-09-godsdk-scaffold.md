# GodSDK Scaffold Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a minimal, open-source-ready Rust workspace for GodSDK with a side-effect-free placeholder `godsdk generate` command and repository integrations for Godlint and Godharness.

**Architecture:** Use a two-crate Cargo workspace: `godsdk-core` owns the future generation boundary and `godsdk-cli` owns argument parsing and user-facing diagnostics. Keep all generation behavior absent and explicitly tested as not implemented. Mirror the sibling repositories’ documentation, configuration, and CI conventions without copying product-specific logic.

**Tech Stack:** Rust 1.97.1, Cargo workspace resolver 3, `clap` 4, `thiserror` 2, `assert_cmd` 2, `predicates` 3, GitHub Actions, Markdown/YAML repository configuration.

## Global Constraints

- The repository is MIT licensed and authored by Tomer Gal.
- The minimum Rust toolchain is `1.97.1`, edition `2024`, and workspace resolver is `3`.
- `godsdk generate --source <PATH> --output <PATH>` must not read, write, resolve, or network-access either path in the scaffold.
- The placeholder generate command exits non-zero and reports that generation is not implemented.
- No new generator or binding logic is included.
- `godlint.yaml` and `godharness.yaml` are repository integrations, not replacements for either product.
- CI must run formatting, Clippy with warnings denied, workspace tests, and a locked build.
- Test code remains under crate `tests/` directories, not under `src/`.
- Documentation must distinguish implemented scaffold behavior from future roadmap behavior.

---

### Task 1: Create the Cargo workspace and core contract

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `crates/godsdk-core/Cargo.toml`
- Create: `crates/godsdk-core/src/lib.rs`
- Create: `crates/godsdk-core/tests/generation.rs`

**Interfaces:**
- Produces `godsdk_core::GenerationRequest { source: PathBuf, output: PathBuf }`.
- Produces `godsdk_core::generate(&GenerationRequest) -> Result<GenerationResult, GenerationError>`.
- `GenerationError::NotImplemented` is the only result from `generate` and carries the exact message `SDK generation is not implemented yet`.
- `GenerationResult` is an empty, non-exhaustive-ready marker struct for the scaffold.

- [ ] **Step 1: Write the failing core contract test**

Create an integration test that constructs a request with `spec.yaml` and `out`, calls `generate`, and asserts the error string is exactly `SDK generation is not implemented yet`.

- [ ] **Step 2: Run the focused test and confirm it fails**

Run: `cargo test -p godsdk-core --test generation`

Expected: FAIL because the workspace, crate, and API do not exist.

- [ ] **Step 3: Add the workspace, toolchain, crate, and minimal implementation**

Use workspace metadata matching the sibling projects: package version `0.1.0`, edition `2024`, Rust version `1.97`, author `Tomer Gal`, MIT license, repository `https://github.com/tomerwave/godsdk`, keywords `sdk`, `code-generation`, `openapi`, `rust`, and categories `command-line-utilities`, `development-tools`.

Implement the error with `thiserror`, forbid unsafe code, and return `Err(GenerationError::NotImplemented)` without touching request paths.

- [ ] **Step 4: Run the focused test and formatting**

Run: `cargo fmt --all -- --check && cargo test -p godsdk-core --test generation`

Expected: PASS.

- [ ] **Step 5: Commit the core scaffold**

```bash
git add Cargo.toml rust-toolchain.toml crates/godsdk-core
git commit -m "Create the GodSDK core workspace boundary"
```

### Task 2: Add the CLI and placeholder generate command

**Files:**
- Create: `crates/godsdk-cli/Cargo.toml`
- Create: `crates/godsdk-cli/src/main.rs`
- Create: `crates/godsdk-cli/src/commands.rs`
- Create: `crates/godsdk-cli/tests/cli.rs`

**Interfaces:**
- Binary name: `godsdk`.
- Command: `godsdk generate --source <PATH> --output <PATH>`.
- `--source` and `--output` are required `PathBuf` values.
- `godsdk --version` prints `godsdk 0.1.0`.
- Missing required arguments are rejected by Clap with a non-zero exit.
- The implemented command exits non-zero and prints `SDK generation is not implemented yet` to stderr.

- [ ] **Step 1: Write failing CLI integration tests**

Cover version output, help output containing `generate`, missing `--source`, and the placeholder command’s non-zero exit plus exact stderr message. Assert the command does not create the requested output directory.

- [ ] **Step 2: Run the focused CLI tests and confirm they fail**

Run: `cargo test -p godsdk-cli --test cli`

Expected: FAIL because the CLI crate and binary do not exist.

- [ ] **Step 3: Implement the Clap command and core delegation**

Parse the command, construct `GenerationRequest`, call `godsdk_core::generate`, print the core error to stderr, and return a non-zero `ExitCode`. Do not add filesystem checks or side effects.

- [ ] **Step 4: Run the focused CLI tests**

Run: `cargo test -p godsdk-cli --test cli`

Expected: PASS.

- [ ] **Step 5: Commit the CLI scaffold**

```bash
git add crates/godsdk-cli
git commit -m "Expose the initial GodSDK CLI contract"
```

### Task 3: Add open-source project documentation and configurations

**Files:**
- Create: `AGENTS.md`
- Create: `CLAUDE.md`
- Create: `CHANGELOG.md`
- Create: `CONTRIBUTING.md`
- Create: `CODE_OF_CONDUCT.md`
- Create: `SECURITY.md`
- Create: `docs/README.md`
- Create: `docs/architecture.md`
- Create: `docs/local-development.md`
- Create: `godlint.yaml`
- Create: `godharness.yaml`
- Modify: `README.md`

**Interfaces:**
- README documents the implemented CLI scaffold, explicit non-goals, planned four-stage pipeline, sibling-product relationship, and local development commands.
- `godlint.yaml` selects `recommended@1` and excludes only generated/build directories where appropriate.
- `godharness.yaml` selects `recommended@1` and enables Codex and Claude Code adapters, matching sibling configuration names.
- AGENTS.md points contributors and agents to the relevant project documents and states that generation logic is not implemented.

- [ ] **Step 1: Write the documentation/configuration files**

Reuse the sibling repositories’ public-document structure and language, but describe GodSDK’s distinct technical SDK-generation scope. Keep future behavior labeled as planned and do not claim release automation or bindings are implemented.

- [ ] **Step 2: Validate documentation and YAML shape**

Run: `git diff --check`, `ruby -e 'require "yaml"; ARGV.each { |f| YAML.load_file(f); puts f }' godlint.yaml godharness.yaml`

Expected: PASS with both configuration files parsed successfully.

- [ ] **Step 3: Commit repository documentation and configuration**

```bash
git add README.md AGENTS.md CLAUDE.md CHANGELOG.md CONTRIBUTING.md CODE_OF_CONDUCT.md SECURITY.md docs godlint.yaml godharness.yaml
git commit -m "Prepare GodSDK for open-source development"
```

### Task 4: Add CI and repository issue/PR hygiene

**Files:**
- Create: `.github/workflows/test.yml`
- Create: `.github/ISSUE_TEMPLATE/bug-report.yml`
- Create: `.github/ISSUE_TEMPLATE/feature-request.yml`
- Create: `.github/PULL_REQUEST_TEMPLATE.md`

**Interfaces:**
- CI runs on pull requests, pushes to `main`, and manual dispatch.
- CI installs Rust `1.97.1`, then runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --locked`, and `cargo build --workspace --locked`.
- Templates direct security reports to `SECURITY.md` and ask feature proposals to distinguish scaffold needs from future generator behavior.

- [ ] **Step 1: Add the workflow and templates**

Use pinned action major versions consistent with the sibling repositories and keep permissions read-only.

- [ ] **Step 2: Validate workflow/configuration syntax**

Run: `git diff --check && ruby -e 'require "yaml"; YAML.load_file(".github/workflows/test.yml"); puts "workflow yaml parsed"'`

Expected: PASS.

- [ ] **Step 3: Commit CI and templates**

```bash
git add .github
git commit -m "Add GodSDK contribution and CI guardrails"
```

### Task 5: Run full verification and prepare the PR

**Files:**
- Modify only files required by verification failures.

- [ ] **Step 1: Run the complete local verification suite**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace --locked && cargo build --workspace --locked && git diff --check`

Expected: all commands exit zero.

- [ ] **Step 2: Validate the placeholder behavior manually**

Run: `cargo run -p godsdk-cli -- generate --source examples/spec.yaml --output /tmp/godsdk-output`

Expected: non-zero exit with `SDK generation is not implemented yet`; `/tmp/godsdk-output` is not created by the command.

- [ ] **Step 3: Review the complete diff and repository status**

Run: `git status --short && git diff main...HEAD --stat && git diff main...HEAD --check`

Expected: only the approved scaffold, documentation, configuration, and CI files are present.

- [ ] **Step 4: Push and open a draft pull request**

Push `agent/godsdk-scaffold` with tracking and create a draft PR against `main`. The body must summarize the scaffold, list explicit non-goals, and include the complete verification commands and results.
