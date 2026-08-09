# Release setup

The release workflow publishes the GodSDK CLI to GitHub Releases and crates.io, then updates the
Godsuite Homebrew tap. It does not publish the CLI to npm or PyPI; those registries are for the
generated language-specific SDK packages.

Before the first version tag, configure these external services:

- Confirm the `godsdk-cli` and `godsdk-core` package names on crates.io and configure the
  `crates-io` GitHub environment with a crates.io trusted publisher or `CARGO_REGISTRY_TOKEN`.
- Configure the `homebrew-tap` GitHub environment and add `HOMEBREW_TAP_SSH_KEY` with write access
  to `tomerwave/homebrew-tap`.
- Confirm the supported target matrix and that the required GitHub-hosted ARM runners are
  available to the repository.

For generated TypeScript SDKs, reserve the npm root and platform package names and configure npm
trusted publishing separately. A future generated Python target should use PyPI trusted
publishing. These are intentionally not placed in the CLI release workflow.
