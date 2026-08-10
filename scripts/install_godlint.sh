#!/usr/bin/env bash
set -euo pipefail

version="$(python3 scripts/update_godsuite.py --print godlint)"
directory="/tmp/godlint-install"
mkdir -p "$directory"
gh release download "v$version" --repo tomerwave/godlint \
  --pattern 'godlint-x86_64-unknown-linux-gnu.tar.gz' --dir "$directory" --clobber
tar -xzf "$directory/godlint-x86_64-unknown-linux-gnu.tar.gz" -C "$directory"
sudo install -m 755 "$directory/godlint" /usr/local/bin/godlint
godlint --version
