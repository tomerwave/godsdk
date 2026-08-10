#!/usr/bin/env bash
set -euo pipefail

tag="$1"
install -m 644 Godsdk.rb tap/Formula/godsdk.rb
git -C tap diff --check
git -C tap config user.name "github-actions[bot]"
git -C tap config user.email "41898282+github-actions[bot]@users.noreply.github.com"

if [ -n "$(git -C tap status --porcelain -- Formula/godsdk.rb)" ]; then
  git -C tap add Formula/godsdk.rb
  git -C tap commit -m "brew: update godsdk to ${tag#v}"
  git -C tap push origin HEAD:main
fi
