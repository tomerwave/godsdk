#!/usr/bin/env bash
set -euo pipefail

if git diff --quiet && git diff --cached --quiet; then
  echo "Godsuite is already current."
  exit 0
fi
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git add -A
git commit -m "chore: update Godsuite tools"
git push
