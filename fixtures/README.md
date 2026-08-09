# GodSDK fixtures

These fixtures are versioned inputs for generated-repository tests. They are intentionally grouped
by behavior so every generator target can run the same contract suite:

- `openapi/minimal-3.1.yaml` is the baseline.
- `openapi/minimal-3.1-changed-operation.yaml` adds one operation without changing the existing
  operation or schema.
- `openapi/parameters-and-errors-3.1.yaml` covers parameter locations, request bodies, media
  types, and standard errors.
- `openapi/schemas-composition-3.1.yaml` covers nested models, recursion, and schema composition.
- `openapi/security-3.1.yaml` covers bearer, API key, basic, and OAuth2 security metadata.
- `openapi/refs-3.1.yaml` and `openapi/refs/models.yaml` cover local external references.
- `openapi/invalid/missing-path-parameter-3.1.yaml` is expected to fail validation.

The E2E harness will use these files to generate isolated temporary repositories, assert the
required contract tree, and verify that a repeat run does not rewrite unrelated files. The fixture
corpus checker in `scripts/tests/test_fixture_corpus.py` validates these invariants without adding
a YAML dependency to the generator workspace.
