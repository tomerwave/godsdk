# GodSDK Repository Scaffold Design

## Goal

Establish a buildable, open-source-ready Rust workspace for `godsdk`, with a stable
placeholder CLI surface and clear boundaries for the future SDK-generation pipeline. This
phase prepares the repository for implementation without implementing OpenAPI ingestion,
intermediate-representation normalization, code generation, or language bindings.

## Context

`godsdk` is the SDK-generation member of the Godsuite. Unlike `godlint`, it is not a source
policy enforcement product, and unlike `godharness`, it is not an agent-context resolver. Its
future job is to turn an API description into one Rust client core and ecosystem-specific
bindings. The sibling repositories provide the Rust workspace conventions, release hygiene,
CI shape, `godlint` configuration, and `godharness` configuration to reuse here.

## Architecture

The initial workspace has two crates:

- `crates/godsdk-core`: the future domain and generation engine boundary. It starts with a
  minimal public error type and a placeholder generation request/result contract so later
  ingestion, IR, and generators can grow behind a library API.
- `crates/godsdk-cli`: the user-facing binary. It exposes `godsdk generate` with explicit
  source and output arguments, delegates to `godsdk-core`, and reports that generation is not
  implemented yet. The command shape is intentionally useful for documentation and future
  compatibility without claiming behavior that does not exist.

No binding crates or generator-specific crates are created yet. They will be added when an
IR contract and target-support strategy are designed.

## CLI Contract for the Scaffold

The binary supports:

```text
godsdk --version
godsdk generate --source <PATH> --output <PATH>
```

`generate` validates only command-line shape. It does not read the source, create output,
resolve references, access the network, or modify the filesystem. It exits non-zero with an
explicit not-implemented diagnostic. This keeps the placeholder safe and makes the eventual
behavioral boundary testable.

## Repository and Open-Source Baseline

The repository will mirror the mature sibling conventions where they apply:

- MIT license, changelog, contribution guide, code of conduct, and security policy;
- Rust toolchain pin and workspace metadata suitable for crates.io publication later;
- `godlint.yaml` using the recommended suite, with no duplicate source-policy engine;
- `godharness.yaml` using the recommended suite and adapter declarations as preparation for
  agent-aware development;
- CI that runs formatting, linting, tests, and a locked build;
- documentation index, local-development guide, architecture notes, and release notes;
- issue and pull-request templates where they provide useful public project hygiene;
- release workflow scaffolding only for checks that are meaningful before publishing is
  configured. Secrets, package publishing, and self-update automation remain explicitly
  disabled until release ownership and package names are finalized.

## Testing Strategy

The scaffold is proved by contract tests rather than implementation tests:

- core unit tests cover the placeholder request/result behavior and its explicit error;
- CLI integration tests cover version output, command help, required arguments, and the
  non-zero not-implemented result without touching the filesystem;
- CI runs the same checks contributors can run locally;
- repository configuration is validated by the installed `godlint` and `godharness` tools
  where available, while Rust checks remain self-contained.

## Non-Goals

- OpenAPI 3.0/3.1 parsing or validation;
- remote or recursive `$ref` resolution;
- schema-to-IR conversion;
- Rust template generation;
- HTTP runtime implementation;
- PyO3, napi-rs, wasm-bindgen, UniFFI, or Diplomat support;
- network access, telemetry, graphical interfaces, or automatic standard invention;
- publishing artifacts or claiming the release pipeline is production-ready.

## Future Boundary Decisions Deferred

The following remain intentionally open until the first implementation design:

- whether the IR is a separate published crate or an internal core module;
- supported OpenAPI dialect and extension policy;
- template ownership and customization/plugin model;
- async runtime and HTTP client policy;
- binding target order and compatibility guarantees;
- generated artifact layout and overwrite/diff behavior;
- release package names and multi-ecosystem publishing strategy.
