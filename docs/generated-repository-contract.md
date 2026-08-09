# Generated repository contract

This document defines the first implementation contract for a repository produced by GodSDK.
The generator may add files as targets evolve, but it must not weaken these invariants.

## Product boundary

GodSDK generates a complete repository from an OpenAPI description and one or more target
languages. A generated client library without its repository metadata, tests, documentation, and
Godsuite governance is an incomplete result.

Every generated repository MUST include and pass Godlint and Godharness before generation is
reported as successful.

## Required layout

```text
<output>/
├── api/openapi.yaml
├── sdk/rust/
├── sdk/python/                         # when python is selected
├── sdk/typescript/                     # when typescript is selected
├── tests/
├── .godsdk/manifest.json
├── godlint.yaml
├── godharness.yaml
├── .agents/
├── .claude/
├── .codex/
├── docs/godharness/
├── .github/workflows/godlint.yml
├── .github/workflows/godharness.yml
├── .github/workflows/test-generated.yml
└── README.md
```

Target directories that were not selected MUST NOT be created. Shared files such as the README,
manifest, governance configuration, and workflows are regenerated only when their inputs change.

## Ownership and update rules

The manifest is the source of truth for generated-file ownership. Each generated file records its
relative path, target, template identifier, and the digest of the content last written by GodSDK.
Files absent from the manifest are user-owned and MUST be preserved.

On a repeat run, GodSDK MUST:

1. Resolve and canonicalize the input before planning writes.
2. Compute the complete target-specific plan in memory.
3. Write only changed generated files in stable order.
4. Preserve user-owned files and unchanged generated files.
5. Delete an old generated file only when its current content still matches the manifest digest.
6. Report a conflict instead of silently deleting or overwriting a user-modified generated file.
7. Update the manifest only after all selected output and governance checks succeed.

`--dry-run` reports the planned adds, changes, deletes, and conflicts without writing. `--check`
fails when the repository differs from the current generation plan. `--prune` enables deletion of
obsolete generated files, subject to the ownership and conflict rules above.

## Manifest contract

`.godsdk/manifest.json` MUST conform to
[`schemas/godsdk-manifest.schema.json`](../schemas/godsdk-manifest.schema.json). The schema
requires generator/template versions, the canonical input digest, selected targets, and the
generated-file ownership records needed for safe incremental updates.

Digests use lowercase hexadecimal SHA-256. Paths use `/` separators and are relative to the
generated repository root. Manifest arrays and generated-file records MUST be emitted in stable
lexicographic order to make repeat runs reviewable.

## Initial CLI surface

```text
godsdk validate --spec api/openapi.yaml
godsdk generate --spec api/openapi.yaml --targets rust,python,typescript --output .
godsdk generate --spec api/openapi.yaml --targets python --output . --dry-run
godsdk generate --spec api/openapi.yaml --targets python --output . --check
godsdk generate --spec api/openapi.yaml --targets python --output . --prune
```

The current scaffold still exposes the earlier `--source`/`--output` placeholder. The new
interface is the target contract for the first real generator implementation; compatibility
aliases, if retained, must be explicit and documented.

## Governance bootstrap

Generation MUST stage these artifacts as part of the same repository transaction:

- `godlint.yaml` and the pinned Godsuite version manifest;
- `godharness.yaml` and the versioned Godharness adapter files;
- Godlint and Godharness workflows;
- the generated-repository test workflow;
- README commands explaining target tests and governance checks.

The generator MUST fail if required governance files are missing or if either tool reports a
problem. It must never claim a successful generated repository while omitting governance.
