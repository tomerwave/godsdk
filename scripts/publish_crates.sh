#!/usr/bin/env bash
set -euo pipefail

version="$1"

published() {
  local package="$1"
  cargo +1.97.1 info "$package@$version" --registry crates-io >/dev/null 2>&1
}

publish_if_missing() {
  local package="$1"
  if published "$package"; then
    return
  fi
  cargo +1.97.1 publish -p "$package" --locked
}

publish_if_missing godsdk-core

for attempt in {1..30}; do
  if published godsdk-core; then
    break
  fi
  if [ "$attempt" -eq 30 ]; then
    echo "godsdk-core@$version did not become visible in the crates.io index" >&2
    exit 1
  fi
  sleep 5
done

publish_if_missing godsdk-cli
