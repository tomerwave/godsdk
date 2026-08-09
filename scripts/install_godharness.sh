#!/usr/bin/env bash
set -euo pipefail

version="$(python3 scripts/update_godsuite.py --print godharness)"
directory=/tmp/godharness-install
mkdir -p "$directory"
gh release download "v$version" --repo tomerwave/godharness \
  --pattern 'godharness-x86_64-unknown-linux-gnu.tar.gz' --dir "$directory" --clobber
tar -xzf "$directory/godharness-x86_64-unknown-linux-gnu.tar.gz" -C "$directory"
sudo install -m 755 "$directory/godharness" /usr/local/bin/godharness
godharness --version
