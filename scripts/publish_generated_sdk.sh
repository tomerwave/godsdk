#!/usr/bin/env bash
set -euo pipefail

git config user.name github-actions[bot]
git config user.email 41898282+github-actions[bot]@users.noreply.github.com
git add -A

if git diff --cached --quiet; then
  echo "No generated changes to publish."
  exit 0
fi

git switch -c "$BRANCH"
git commit -m "chore: update generated SDK"
git push --set-upstream origin "$BRANCH"

{
  echo "### GodSDK commit mode"
  echo
  echo "Generated changes were pushed to \`$BRANCH\`. Open a pull request after reviewing the artifact."
} >> "$GITHUB_STEP_SUMMARY"
