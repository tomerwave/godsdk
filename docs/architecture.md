# Architecture

## Product boundary

GodSDK will generate SDK artifacts from technical API specifications. Godlint and Godharness are
development dependencies of the repository, not components of the runtime generation pipeline.

## Planned pipeline

### Ingestion

Read OpenAPI 3.0/3.1 JSON or YAML, validate the document, and eventually resolve local or remote
`$ref` dependencies under an explicit security policy. The scaffold does not perform any of
these operations.

### Intermediate representation

Normalize paths, methods, parameters, headers, request bodies, responses, models, enums,
recursive types, optionality, authentication, and canonical naming into a typed Rust IR. The IR
should be the stable boundary between parsing and target generators.

### Rust client core

Generate a publishable Rust crate containing shared HTTP, serialization, authentication, retry,
rate-limit, and error behavior. The exact HTTP runtime and template engine remain open decisions.

### Bindings

Generate native ecosystem artifacts through target-specific engines such as PyO3, napi-rs,
wasm-bindgen, UniFFI, or Diplomat. Target order and compatibility guarantees are intentionally
not selected yet.

## Deferred decisions

- published versus internal IR crate;
- supported OpenAPI dialects and vendor extensions;
- async runtime and HTTP client;
- template customization and plugin boundaries;
- output overwrite and diff behavior;
- package names, versioning, and release targets.
