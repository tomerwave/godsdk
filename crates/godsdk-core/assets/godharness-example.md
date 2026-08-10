---
id: generated-sdk-context
title: Generated SDK repository context
keywords: [sdk, api, openapi, generated]
paths: [sdk/**, api/**]
must-read: true
supersedes: []
relates-to: []
---

## Rule

Keep endpoint behavior in the generated Rust SDK core. Language targets are bindings and
packaging surfaces over that shared behavior.

## Why

Regenerating from the API specification must not create divergent endpoint implementations across
language targets.

## How to apply

Run `godsdk generate` from the source specification when the API contract changes, then run the
selected target tests and the Godsuite governance checks.
