#!/usr/bin/env bash
set -euo pipefail

version="$(sed -n 's/^godlint: *//p' .github/godsuite-versions.yml)"
asset="godlint-x86_64-unknown-linux-gnu.tar.gz"
directory="${RUNNER_TEMP:-/tmp}/godlint"
mkdir -p "$directory"
gh release download "v$version" --repo tomerwave/godlint --pattern "$asset*" --dir "$directory"
(cd "$directory" && sha256sum --check "$asset.sha256")
tar -xzf "$directory/$asset" -C "$directory"
echo "$directory" >> "$GITHUB_PATH"
godlint --version
