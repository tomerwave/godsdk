# Release setup

The release workflow publishes the GodSDK CLI to GitHub Releases, crates.io, and npm, then updates
the Godsuite Homebrew tap. npm publishes both `godsdk` and `@godsdk/cli`; each installs the global
`godsdk` command and selects a tested platform binary without an install-time download.

Before the first version tag, configure these external services:

- Confirm the `godsdk-cli` and `godsdk-core` package names on crates.io and configure the
  `crates-io` GitHub environment with a crates.io trusted publisher or `CARGO_REGISTRY_TOKEN`.
- Create or control the npm `@godsdk` scope, reserve `godsdk` and `@godsdk/cli`, and configure the
  `npm` GitHub environment as a trusted publisher with npm provenance enabled. `NPM_TOKEN` was
  only needed for the bootstrap release and can be removed after the first trusted-publishing
  release succeeds.
- Configure the `homebrew-tap` GitHub environment and add `HOMEBREW_TAP_SSH_KEY` with write access
  to `tomerwave/homebrew-tap`.
- Confirm the supported target matrix and that the required GitHub-hosted ARM runners are
  available to the repository.

For generated TypeScript SDKs, reserve the npm root and platform package names and configure npm
trusted publishing separately. Generated Python SDKs use the pinned PyPI trusted-publishing
workflow, so reserve the package name and configure its PyPI publisher separately. These external
registrations are intentionally not performed by the CLI.
