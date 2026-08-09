# GodSDK fixtures

These fixtures are versioned inputs for generated-repository tests. They intentionally begin
with a minimal OpenAPI 3.1.1 pair so the first vertical slice can prove deterministic generation
and relevant-only updates:

- `openapi/minimal-3.1.yaml` is the baseline.
- `openapi/minimal-3.1-changed-operation.yaml` adds one operation without changing the existing
  operation or schema.

The E2E harness will use these files to generate isolated temporary repositories, assert the
required contract tree, and verify that a repeat run does not rewrite unrelated files.
