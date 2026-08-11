# GitHub generation workflow

GodSDK provides a release-backed composite Action and a reusable workflow for repositories that
want to regenerate SDKs from a checked-in OpenAPI document.

The recommended first run is `dry-run`. It downloads the exact CLI release for the runner,
verifies the release archive checksum, generates into the requested output, and runs Godlint and
Godharness in the reusable workflow. The action reports `changed-files`, `summary`, and the exact
`generator-version` used.

## Minimal caller

```yaml
name: Generate SDKs

on:
  workflow_dispatch:
    inputs:
      mode:
        required: true
        default: dry-run
        type: choice
        options: [dry-run, check, write]
      commit:
        required: true
        default: false
        type: boolean

jobs:
  generate:
    uses: tomerwave/godsdk/.github/workflows/generate-sdk.yml@v0.1.2
    with:
      spec-path: api/openapi.yaml
      targets: rust,typescript,python
      mode: ${{ inputs.mode }}
      commit: ${{ inputs.commit }}
      generator-version: 0.1.2
      godlint-version: 0.7.0
      godharness-version: 0.1.6
```

Pin the reusable workflow to a release tag or immutable commit. Update the pin deliberately when
upgrading GodSDK. Do not use a floating branch in production.

## Remote specifications and references

Local `spec-path` is the default and safest input. `spec-url` is explicit and must be HTTPS plus a
caller-supplied `spec-sha256`. Remote `$ref` documents require both `remote-ref-hosts` and a
matching `remote-ref-pins` entry. The workflow never derives an allowlist or checksum from the
specification itself.

## Commit mode

The default workflow permissions are read-only. Setting `commit: true` enables a separate job with
`contents: write`; it reruns the pinned generator, reruns Godlint and Godharness, and pushes an
isolated `godsdk/update-<run-id>` branch. Review that branch in a pull request before merging.

The caller must grant `contents: write` at the caller level when it intentionally enables commit
mode; GitHub does not let a reusable workflow elevate permissions that its caller withheld. Keep
the default `contents: read` setting for dry-run and check runs.

The workflow does not create repositories or silently open pull requests. Repository creation,
branch protection, and pull-request policy remain explicit administrator actions.

## Release preparation

Before consumers can update the version pin:

1. Merge the action changes.
2. Push a matching `vX.Y.Z` tag so the release workflow publishes all runner archives and checksums.
3. Update caller workflow pins from the previous release to the new tag or immutable commit.
4. Ensure the consuming repository allows Actions to receive the required read permission; enable
   write permission only where the explicit commit job is intended.

The starter fixture at `fixtures/action-starter` is a contract fixture, not an Anchor-specific
integration. It intentionally contains no generated SDK output.
