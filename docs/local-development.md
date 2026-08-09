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
```

The current command exits non-zero with `SDK generation is not implemented yet` and does not
touch either path. This is intentional until the generation contract is designed.

## Repository tools

`godlint.yaml` and `godharness.yaml` keep this repository aligned with the Godsuite’s source
policy and agent-context workflows. Their presence does not make GodSDK’s future generator
behavior complete.
