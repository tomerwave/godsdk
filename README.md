# GodSDK

GodSDK is the technical SDK-generation tool in the Godsuite. It is planned as a Rust-based
pipeline that turns OpenAPI or custom API specifications into one strongly typed Rust client
core and ecosystem-native bindings.

> **Status: pre-alpha scaffold.** The workspace and CLI contract exist. Generation behavior is
> intentionally not implemented yet.

## The Godsuite

- **Godlint** enforces deterministic source-code and repository policy.
- **Godharness** provides project and engineering context to coding agents.
- **GodSDK** will generate SDKs from technical API descriptions.

GodSDK is not another linter and does not treat generated code as proof of compliance. Godlint
and Godharness are development tools used to keep this repository understandable and healthy
while the generator is built.

## Current CLI

Install Rust 1.97.1, then run:

```sh
cargo run -p godsdk-cli -- --help
cargo run -p godsdk-cli -- generate --source spec.yaml --output ./generated
```

The `generate` command currently validates its command-line shape and reports that generation is
not implemented. It does not read the source, create the output directory, resolve references,
access the network, or modify the filesystem.

## Planned architecture

```text
OpenAPI or custom specification
                │
                ▼
       Ingestion and validation
                │
                ▼
       Normalized godsdk IR
                │
                ▼
        Rust client-core generator
                │
                ▼
   Python · Node · Web · Mobile bindings
```

The intended stages are:

1. Ingest OpenAPI 3.0/3.1 JSON and YAML, including local and remote reference handling.
2. Normalize endpoints, models, authentication, casing, and response shapes into a typed IR.
3. Generate a publishable Rust client core with shared HTTP, serialization, auth, retry, and
   rate-limit behavior.
4. Generate native bindings for Python, Node.js/TypeScript, WebAssembly, Swift, Kotlin, and
   other supported targets.

The boundaries, dependency choices, generated artifact layout, and binding order are not yet
final. See [the architecture notes](docs/architecture.md) for the decisions intentionally
deferred.

## Development

```sh
rustup toolchain install 1.97.1 --profile minimal
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
```

Read [local development](docs/local-development.md) before changing the workspace and
[contributing](CONTRIBUTING.md) before opening a pull request.

## Open source

GodSDK is released under the [MIT License](LICENSE). See the [security policy](SECURITY.md) for
private vulnerability reporting and the [Code of Conduct](CODE_OF_CONDUCT.md) for participation
expectations.
