# GodSDK

<p align="center">
  <img src="assets/godsdk-icon.svg" alt="GodSDK" width="180">
</p>

GodSDK is the technical SDK-generation tool in the Godsuite. It is planned as a Rust-based
pipeline that turns OpenAPI or custom API specifications into one strongly typed Rust client
core and ecosystem-native bindings.

> **Status: pre-alpha runtime.** Rust SDK generation, typed schema models, and a local
> Rust-backed Node.js/TypeScript target are implemented. Cross-platform native packaging and
> additional language bindings remain in progress.

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
cargo run -p godsdk-cli -- validate --spec spec.yaml
```

For released CLI binaries, use Homebrew (`brew install tomerwave/tap/godsdk`), npm (`npm install
--global godsdk`), crates.io (`cargo install godsdk-cli`), or the GitHub Releases page.

The `validate` command parses an OpenAPI 3.0 or 3.1 document without touching an output directory.
The `generate` command reads the same supported versions and creates a standalone SDK repository with
a Tokio/reqwest Rust client, a Zod-validated TypeScript facade backed by a napi-rs native crate,
local mock-server E2E tests, Cargo.lock, Godlint, and Godharness configuration.

To try the generated client:

```sh
temporary="$(mktemp -d)"
cargo run -p godsdk-cli -- generate \
  --source fixtures/openapi/minimal-3.1.yaml \
  --output "$temporary/generated"
(cd "$temporary/generated" && cargo test --manifest-path sdk/rust/Cargo.toml --locked)
(cd "$temporary/generated/sdk/typescript" && npm install && npm run test:native)
```

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

1. Ingest OpenAPI 3.0/3.1 JSON and YAML, including local references. Remote `$ref` documents are
   opt-in and require both an allowlisted host and a SHA-256 pin.
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

Repository automation also keeps Godlint and Godharness current through a reviewable scheduled
workflow. The allowed release level is configured in `.github/godsuite-versions.yml` and can be
overridden manually for patch, minor, or major updates.

See [release setup](docs/release-setup.md) for the external credentials and trusted-publisher
configuration that cannot be generated safely.

For automated generation in another repository, see the [GitHub Action and reusable workflow
guide](docs/github-action.md). The checked-in [starter fixture](fixtures/action-starter) shows the
smallest repository shape and starts in dry-run mode.
