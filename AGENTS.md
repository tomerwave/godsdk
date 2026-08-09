# GodSDK agent guide

GodSDK is the SDK-generation project in the Godsuite. Read the [README](README.md),
[architecture notes](docs/architecture.md), and [local development guide](docs/local-development.md)
before changing implementation or public contracts.

## Current status

The repository is a pre-alpha Rust workspace scaffold. `godsdk generate` is a side-effect-free
placeholder that reports `SDK generation is not implemented yet`. Do not imply that it parses
specifications, creates files, resolves references, or produces bindings until those behaviors
are implemented and tested.

## Boundaries

- Godlint owns deterministic repository and source-policy enforcement.
- Godharness owns agent-context and documentation governance.
- GodSDK owns future specification ingestion, normalization, generation, and binding artifacts.
- Keep tests under `crates/<crate>/tests/`, not under `src/`.
- Keep repository content local by default; do not add network access or telemetry without a
  design that explicitly addresses the trust boundary.
- Prefer small, reviewable changes and update public documentation when a contract changes.

## Verification

Before claiming a change is complete, run the formatting, Clippy, test, build, and diff checks
listed in [local development](docs/local-development.md).
