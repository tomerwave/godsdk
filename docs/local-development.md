# Local development

## Toolchain

The workspace pins Rust 1.97.1 in `rust-toolchain.toml`.

```sh
rustup toolchain install 1.97.1 --profile minimal
```

## Checks

Run the same checks locally that CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
git diff --check
```

## Running the CLI

```sh
cargo run -p godsdk-cli -- --help
cargo run -p godsdk-cli -- generate --source spec.yaml --output ./generated
cargo run -p godsdk-cli -- validate --spec spec.yaml
```

`validate` is non-mutating: it parses the specification and reports its normalized summary without
creating generated files. GodSDK accepts OpenAPI 3.0.x and 3.1.x. Local references resolve by
default; remote references require both `--remote-ref-host HOST` and
`--remote-ref-pin URL=SHA256`. The command creates a fresh generated repository. The generated
Rust client is async-first and
its local mock-server integration test runs without internet access:

```sh
temporary="$(mktemp -d)"
cargo run -p godsdk-cli -- generate \
  --source fixtures/openapi/minimal-3.1.yaml \
  --output "$temporary/generated"
(cd "$temporary/generated" && cargo test --manifest-path sdk/rust/Cargo.toml --locked)
```

## Repository tools

`godlint.yaml` and `godharness.yaml` keep this repository aligned with the Godsuite’s source
policy and agent-context workflows. Their presence does not make GodSDK’s future generator
behavior complete.

CI runs Godlint through `tomerwave/godlint@v1` and runs the Rust workspace checks separately.
The scheduled `Update Godsuite tools` workflow updates both the pinned Godlint action version
and the installed Godharness release within the policy in `.github/godsuite-versions.yml`.
Run it manually when you want to widen the policy from `patch` to `minor` or `major`; every
change is committed by GitHub Actions for review.
