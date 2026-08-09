# Contributing to GodSDK

GodSDK is pre-alpha. The repository currently ships a workspace and CLI scaffold, not an SDK
generator. Architecture feedback, specification-format analysis, and tests for the future
contracts are useful contributions.

## Before opening an issue or pull request

- Read the [README](README.md) and [architecture notes](docs/architecture.md).
- Search existing issues and pull requests first.
- Keep a proposal focused on one boundary: ingestion, IR, generation, bindings, or tooling.
- Explain whether the proposal changes the public CLI, generated artifact contract, or only
  internal implementation.
- For security issues, follow [SECURITY.md](SECURITY.md) instead of opening a public issue.

## Development rules

- Use Rust 1.97.1 and keep the workspace buildable.
- Keep generation behavior deterministic and local-first by default.
- Do not add a dependency or target binding without documenting why it belongs in the current
  stage.
- Keep tests out of `src/`; use crate integration tests under `crates/<crate>/tests/`.
- Update the README, architecture notes, and changelog when public behavior changes.
- Keep commits small and use a clear intent-oriented message. Lore trailers are welcome for
  decisions with lasting trade-offs.

## Pull requests

Describe what changed, why it belongs in the current phase, how it was tested, and which future
work remains intentionally out of scope. A pull request that changes generation semantics should
include contract tests and explain compatibility implications.
