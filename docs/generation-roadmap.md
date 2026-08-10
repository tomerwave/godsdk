# GodSDK Generation Roadmap

Status: planning baseline after the initial repository scaffold.

## Product contract

GodSDK does not merely emit a client library. It produces a complete, buildable repository from
an OpenAPI description and a target-language selection. Every generated repository must contain
the client artifacts, tests, documentation, CI, and Godsuite governance files needed to work on
the result immediately.

The non-negotiable governance invariant is:

> Every generated repository includes and validates Godlint and Godharness before the generated
> SDK is considered usable.

That means generation must produce, at minimum:

```text
generated-repo/
├── api/openapi.yaml                         # copied or normalized source input
├── sdk/rust/                                # generated Rust SDK and single behavior source
│   ├── core/                                # endpoint/auth/serialization/retry logic
│   └── client/                              # public Rust SDK
├── sdk/python/                              # generated Python package when selected
├── sdk/typescript/                          # generated JS runtime + TS declarations
├── tests/                                    # generated contract and smoke tests
├── .godsdk/config.yaml                       # user intent and release destinations
├── .godsdk/manifest.json                     # machine state: hashes and ownership
├── godlint.yaml                              # checked-in source-policy configuration
├── godharness.yaml                           # checked-in context configuration
├── .agents/                                  # Codex skills/context installed by Godharness
├── .claude/                                  # Claude Code settings/context
├── .codex/                                   # Codex hook configuration
├── docs/godharness/                          # generated context example/index
├── .github/workflows/godlint.yml             # deterministic policy gate
├── .github/workflows/godharness.yml          # context/configuration gate
├── .github/workflows/test-generated.yml      # generated target test matrix
├── .github/workflows/release.yml             # crates.io, PyPI, npm, GitHub Releases
├── NEEDS-YOUR-ATTENTION.md                   # only unresolved external setup
└── README.md                                 # exact commands for the selected targets
```

The directory names are a proposal. The ownership and behavior rules are the important part:
generated files must be distinguishable from user-authored files, target selection must be
recorded, and governance installation must be part of the generation transaction.

## Evidence and implications

### OpenAPI is a graph, not one flat file

The OpenAPI 3.1.1 specification defines an OpenAPI Description as an entry document plus any
referenced documents, and `$ref` values are URI references that may identify relative documents.
Schema Objects follow JSON Schema 2020-12 semantics in 3.1.1. The ingestion stage therefore
needs a resolver and a normalized graph, not a collection of ad-hoc endpoint structs.

Source: [OpenAPI Specification 3.1.1](https://spec.openapis.org/oas/v3.1.1.html), especially the
OpenAPI Description, Reference Object, and Schema Object sections.

OpenAPI path templates must have matching path parameters, and operations may have an
`operationId` used to resolve links. A generator must define deterministic naming when an
`operationId` is absent, and must reject collisions rather than silently produce ambiguous APIs.

Source: [OpenAPI path templating](https://spec.openapis.org/oas/v3.1.1.html#path-templating) and
[OpenAPI operation/link semantics](https://spec.openapis.org/oas/v3.1.1.html#link-object).

### Users expect validate, generate, dry-run, and repeatable updates

OpenAPI Generator exposes separate `validate` and `generate` commands, accepts a specification
as a file or URL, writes to an output directory, and includes `--dry-run`, `--minimal-update`,
and `--skip-overwrite` controls. These are strong evidence for the minimum ergonomic surface of
GodSDK, even though GodSDK's generated-repository contract is intentionally broader.

Source: [OpenAPI Generator usage](https://openapi-generator.tech/docs/usage/).

### GitHub Actions should receive a reference to the spec, not the whole spec

Custom Actions declare string inputs through `action.yml`. Manual workflow inputs have a bounded
payload, and GitHub documents a 65,535-character maximum for `workflow_dispatch`. The Action
should therefore accept `spec-path` as the default, with an explicit `spec-url` mode for remote
inputs. It should not require users to paste a large YAML document into a workflow input.

Sources: [Action metadata inputs](https://docs.github.com/en/actions/reference/workflows-and-actions/metadata-syntax)
and [workflow dispatch inputs](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow).

GitHub supports both composite Actions and reusable workflows. A composite Action is the right
unit for installing the pinned GodSDK binary and invoking the CLI; a reusable workflow is the
right unit for checkout, permissions, target matrices, governance checks, and optional commits.

Source: [Reusable workflow and composite Action guidance](https://docs.github.com/en/actions/concepts/workflows-and-actions/reusing-workflow-configurations).

### Target packaging imposes real repository contracts

For Python, Maturin's official guide shows a Rust `cdylib`, PyO3 configuration, a PEP 517
`pyproject.toml`, local `maturin develop`, and distributable wheels from `maturin build`. The
generated Python directory therefore needs packaging metadata and an install/import test, not
only generated Rust bindings.

Source: [Maturin tutorial](https://www.maturin.rs/tutorial.html) and [PyO3 user guide](https://pyo3.rs/main/).

For JavaScript/TypeScript, napi-rs's maintained template generates a native `.node` addon,
loader JavaScript, and TypeScript declarations. Its distribution model uses a root package plus
platform-specific optional packages, and its CI must explicitly build the target matrix. JS and
TypeScript should therefore be one generator target with a JS runtime surface and generated
`.d.ts`, not two separate implementations.

Source: [NAPI-RS getting started and distribution model](https://napi.rs/docs/introduction/getting-started).

For Rust, a virtual Cargo workspace is the natural generated-repository root: Cargo documents
that it can organize multiple packages without a root package, requires an explicit resolver,
and runs workspace-wide commands over its members. The generated Rust core and target bindings
can therefore share a workspace without pretending they are one crate.

Source: [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html).

## Generated repository ownership and incremental regeneration

Running the CLI again must update only relevant generated files. This is a first-class product
requirement, not a later optimization.

### Manifest

Each generated repository carries `.godsdk/config.yaml` for user intent and
`.godsdk/manifest.json` for machine state. The config selects targets, names packages, and enables
the concrete release destinations. The manifest carries:

- generator version and template-set version;
- canonical input spec digest and resolved-reference digests;
- selected targets and target configuration digests;
- generated file path, target, template identifier, and last-generated content digest;
- governance bundle version and Godlint/Godharness versions;
- generation options that affect file ownership or layout.

The generated Rust SDK is the single implementation source. Python and TypeScript bindings expose
the Rust SDK surface and must not duplicate endpoint behavior.

### Regeneration algorithm

1. Load and validate the existing manifest before reading or writing generated output.
2. Resolve the input spec into a canonical, deterministic representation.
3. Compute the target-specific generation plan and content hashes in memory.
4. Compare planned files with the manifest and working tree.
5. Write only changed generated files, in stable ordering, through a temporary directory and
   atomic rename.
6. Preserve user-owned files and generated files that are unchanged.
7. Delete a previously generated file only when its current content still matches the manifest;
   if a user modified it, report a conflict and require `--prune` plus explicit confirmation.
8. Update the manifest last, only after every selected target and governance artifact succeeds.

Required CLI modes:

```text
godsdk validate --spec api/openapi.yaml
godsdk generate --spec api/openapi.yaml --targets rust,python,typescript --output .
godsdk generate --spec api/openapi.yaml --targets python --output . --dry-run
godsdk generate --spec api/openapi.yaml --targets python --output . --check
godsdk generate --spec api/openapi.yaml --targets python --output . --prune
```

`--dry-run` prints an add/change/delete/conflict plan without writing. `--check` exits non-zero
when the repository is out of date. A target-specific run must not rewrite unrelated targets;
shared files are regenerated only when their computed inputs change.

## GitHub Action and workflow design

### Release contract

The generated repository includes release automation for crates.io, PyPI, npm, and GitHub Releases
from the first release implementation. The CLI generates package metadata, workflows, version
wiring, OIDC permissions, checksums, and pre-release checks. `NEEDS-YOUR-ATTENTION.md` contains only
external actions the CLI cannot perform, such as registering a package or configuring a trusted
publisher/environment. Bun is supported as a consumer of the npm package, not as a separate
registry.

### Action contract

Publish a release-backed `tomerwave/godsdk-action` composite Action. It downloads an exact
GodSDK binary for the runner, verifies its checksum, and invokes the CLI. Its initial inputs:

```text
spec-path          required unless spec-url is used
spec-url           optional, explicit remote input
targets            required CSV: rust,python,typescript
output             default: .
project-name       required for a new repository
package-scope      optional for npm output
generator-version  default: pinned/repository policy
mode               generate | dry-run | check
prune              default false
```

The Action must never silently fetch remote references. Remote `$ref` resolution is opt-in and
requires both an allowlisted host and a SHA-256 pin for each retrieved document. The active policy
is recorded in generated configuration without storing secrets.

### Reusable workflow contract

Publish a reusable workflow, for example `.github/workflows/generate-sdk.yml`, that supports both
`workflow_call` and `workflow_dispatch`:

- `workflow_call` lets a service repository invoke generation from another workflow;
- `workflow_dispatch` provides the easy manual path from the GitHub Actions UI;
- inputs select the spec path, targets, output mode, and whether to commit;
- default permissions are read-only;
- `contents: write` is granted only to an explicit commit/push job;
- generated changes are summarized as an artifact or pull request before commit;
- the workflow runs Godlint and Godharness against the generated repository before declaring
  success.

For a genuinely new repository, use a small starter repository/template containing the workflow.
The Action should not create arbitrary repositories by default. Repository creation and push
require an explicit token and are a separate, auditable step.

### Governance bootstrap

The generator must treat governance as a required output stage:

1. Write `godlint.yaml`, `godharness.yaml`, and the pinned Godsuite manifest.
2. Install the versioned Godharness adapter files for Codex and Claude Code.
3. Write Godlint and Godharness CI workflows.
4. Run `godharness check` and Godlint against the staged generated repository.
5. Refuse to finish if either tool or required adapter files are missing.

The generated README must show users exactly how to run the generated target tests and where the
governance files came from.

## End-to-end test architecture

The E2E suite must test repository generation, not only individual code templates.

### Fixture corpus

Start with a versioned OpenAPI fixture corpus covering:

1. minimal valid API and one operation;
2. path, query, header, cookie, and request-body parameters;
3. required, optional, nullable, arrays, maps, enums, nested objects, and recursive models;
4. `oneOf`, `anyOf`, `allOf`, discriminators, and unknown additional properties;
5. multiple media types, binary/file payloads, pagination, and standard error responses;
6. security schemes: bearer, API key, basic, and OAuth2 metadata;
7. local and external relative `$ref` documents;
8. missing/ambiguous operation identifiers, invalid path parameters, and unsupported features;
9. examples and descriptions that must survive into generated documentation;
10. deterministic API changes used to test incremental regeneration.

The first vertical slice should use an OAS 3.1.1 fixture. Add a deliberately scoped OAS 3.0
compatibility fixture only after the 3.1 IR contract is stable; do not silently rewrite 3.0 into
3.1 or discard 3.1 JSON Schema semantics.

### Generated-repository test loop

For each fixture and target selection:

1. generate into a fresh temporary repository;
2. assert the expected file tree and manifest;
3. assert Godlint/Godharness configuration, adapter files, and workflows exist;
4. run the generated repository’s validation and target build commands;
5. start a deterministic local mock HTTP server;
6. run generated client calls against it and assert request serialization, auth, response
   decoding, error decoding, and retry behavior;
7. regenerate without changes and assert a clean Git diff;
8. change exactly one operation in the spec and assert only relevant target files change;
9. modify a generated file and verify conflict detection prevents silent deletion;
10. run `--check` and verify it detects drift after an intentional edit.

### Target-specific gates

**Rust:** generated workspace compiles with locked dependencies; integration tests exercise every
fixture operation against the local server; generated code is formatted and Clippy-clean.

**Python:** build a wheel with Maturin, install it into a clean virtual environment, import the
module, run typed client calls, and run the generated Python test suite. The initial matrix should
cover the supported minimum Python version and the newest supported CPython version.

**JavaScript/TypeScript:** install the generated package, build the napi native addon, run
TypeScript type-checking, import the JavaScript loader, verify generated declarations, and call
the local server from Node. Release tests must exercise the platform-package layout, not just the
host-local addon.

### CI layers

- PR fast lane: one Linux fixture per target, generation idempotence, governance checks, Rust,
  one Python, and one Node version.
- Nightly compatibility lane: full fixture corpus, OAS 3.0/3.1 compatibility, Python/Node
  version matrix, and target-specific failure diagnostics.
- Release lane: full cross-platform artifact matrix, clean-environment installs, checksums, and
  generated-repository smoke tests from the published binary.

## Roadmap and delivery order

### Phase 0 — contracts and fixtures

- Freeze generated-repository layout, manifest schema, target names, ownership rules, CLI error
  contract, and governance invariant.
- Add the fixture corpus and a runner that can assert file trees and clean regeneration.
- Keep the current CLI placeholder until these contracts have tests.

### Phase 1 — OpenAPI ingestion and Rust vertical slice

- Implement JSON/YAML loading, OAS version detection, validation, and local `$ref` resolution.
- Build a typed normalized IR for operations, parameters, schemas, media types, responses, and
  security.
- Generate a Rust client repository from one OAS 3.1.1 fixture.
- Complete the generated-repository E2E loop, including local-server request/response tests and
  mandatory Godlint/Godharness bootstrap.

### Phase 2 — repository generator and incremental updates

- Add manifest ownership and target-specific content hashing.
- Implement `--dry-run`, `--check`, conflict detection, safe prune, atomic writes, and relevant-
  only regeneration.
- Generate README, CI, examples, package metadata, and governance artifacts as one repository
  bundle.

### Phase 3 — Python target

- Expose the shared Rust core through PyO3.
- Generate Maturin/PEP 517 metadata, Python package API, typing surface, examples, and tests.
- Prove clean wheel build/install/import and local-server behavior in CI.

### Phase 4 — JS/TS target

- Expose the shared Rust core through napi-rs.
- Generate the JS loader, TypeScript declarations, package metadata, target package matrix, and
  Node test suite.
- Prove host build first, then platform package assembly and clean installation.

### Phase 5 — GitHub Action and workflow productization

- Release the binary-backed composite Action and reusable workflow.
- Support repository-local spec paths first, then opt-in remote specs with security controls.
- Add dry-run PR summaries and explicit commit mode.
- Test the Action from a fixture starter repository, not only from this source repository.

### Phase 6 — compatibility and ecosystem expansion

- Expand OAS 3.0 compatibility where the IR can preserve semantics.
- Add additional bindings only after the generated-repository and governance contracts remain
  stable.
- Add release packaging for crates.io, PyPI, npm, and platform artifacts only when each package
  contract is independently tested.

## Definition of done for the first real release

The first non-scaffold release is complete only when a clean temporary repository can run one
command with an OpenAPI spec and selected targets, receive a complete repository, pass Godlint
and Godharness, build/install each requested target, call a local mock API successfully, and
rerun generation without unrelated file churn or silent user-file loss.
