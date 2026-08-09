# Typed Rust and TypeScript Target Design

**Status:** In progress — typed Rust models, Zod facade, and local napi-rs execution are implemented; cross-platform artifact publication remains.
**Date:** 2026-08-09

## Goal

Continue GodSDK from the raw-string Rust runtime to a typed Rust core and a TypeScript target
that remains backed by Rust while exposing native TypeScript types and Zod runtime validation.

The generated TypeScript package must never silently degrade to `any`, generic records, or
unvalidated JSON. Unsupported schema constructs fail generation with an actionable diagnostic.

## Target architecture

```text
OpenAPI 3.1 schema graph
          |
          v
Shared typed GodSDK IR
          |
          +--> Rust serde models and typed async operations
          |        |
          |        +--> canonical HTTP/auth/retry behavior
          |
          +--> TypeScript Zod schemas and type declarations
                   |
                   +--> N-API wrapper over the Rust client
                   +--> runtime validation at the JS boundary
```

Rust remains the canonical endpoint and transport implementation. TypeScript does not generate a
second fetch/HTTP client. The TypeScript package calls a generated napi-rs native addon backed by
the Rust client core, then validates native boundary values with generated Zod schemas.

napi-rs is selected because its normal npm distribution uses a root package plus exact-version
platform packages and optional dependencies, avoiding consumer-side compilers and install-time
binary downloads. Zod is selected because one generated schema supplies runtime parsing and
static types through `z.infer`.

References:

- <https://napi.rs/docs/deep-dive/release>
- <https://napi.rs/docs/concepts/napi-attributes>
- <https://zod.dev/basics>

## Shared schema IR

Extend the current operation-only IR with deterministic named schemas:

- primitive types: string, integer, number, boolean, null;
- formats: uuid, date, date-time, byte, binary, and known numeric formats;
- arrays and maps with typed values;
- objects with ordered properties, required fields, defaults, descriptions, and
  additional-properties policy;
- enums with stable generated names;
- nullable and optional semantics kept distinct;
- `$ref` links resolved to named IR nodes;
- `allOf`, `oneOf`, and `anyOf` with explicit supported semantics;
- discriminators;
- request and response media types;
- operation status-to-schema mappings;
- standard error response schemas;
- schema source locations for actionable generation errors.

The IR must preserve the distinction between:

- missing property versus present `null`;
- unknown object keys allowed versus rejected;
- an absent response body versus an empty response body;
- a union that is statically representable versus one that needs a discriminator.

No target may use an untyped fallback for a schema that the IR accepted.

## Typed Rust output

Generate a Rust module containing:

- `serde::Serialize` and `serde::Deserialize` structs;
- enums for string/integer enums and tagged unions;
- explicit `Option<T>` only for optional properties or nullable values according to the
  configured representation;
- typed request body parameters;
- typed success response bodies;
- typed status-specific API errors;
- a documented raw-body escape hatch only for media types without a supported schema.

Generated methods should move from:

```rust
async fn get_pet(...) -> Result<String, SdkError>
```

to:

```rust
async fn get_pet(...) -> Result<GetPetResponse, SdkError>
```

The exact response wrapper may be a generated enum when an operation has multiple success
statuses. JSON decoding errors must identify the operation and model name without including
credentials or unrestricted response payloads.

## TypeScript package

Generate:

```text
sdk/typescript/
├── package.json
├── tsconfig.json
├── src/
│   ├── index.ts
│   ├── schemas.ts
│   ├── types.ts
│   └── errors.ts
├── native/                 # napi-rs loader/artifact boundary
├── tests/
│   ├── validation.test.ts
│   └── client.test.ts
└── README.md
```

The package will:

- import `zod` as a runtime dependency;
- expose every generated model schema from `schemas.ts`;
- expose `z.infer<typeof ModelSchema>` aliases from `types.ts`;
- expose a typed client facade from `index.ts`;
- call the Rust-backed napi-rs addon for endpoint execution;
- parse successful responses with the corresponding Zod schema;
- parse typed API errors with status-specific schemas;
- convert Zod failures into a stable `SdkValidationError` that includes operation/model
  identifiers but not secrets or full unbounded payloads;
- keep native loading behind napi-rs's generated platform package loader;
- publish platform packages and the root package using the napi-rs distribution model.

The generated TypeScript API must not export `any`, `Record<string, unknown>`, or
`unknown` as a normal model/operation result. An open object is represented only when the
OpenAPI schema explicitly permits additional properties, and its value type comes from the IR.

For a model such as `Document`, output should resemble:

```ts
import * as z from "zod";

export const DocumentSchema = z.object({
  id: z.uuid(),
  title: z.string(),
  content: z.string().optional(),
}).strict();

export type Document = z.infer<typeof DocumentSchema>;
```

The renderer must use the project-pinned Zod major version and should generate explicit
`.passthrough()`, `.strict()`, or typed `.catchall(...)` according to the OpenAPI
additional-properties policy. It must never use `z.custom()` without a validator.

## N-API boundary

The Rust addon exposes only generated, typed operations and model-compatible values. The
TypeScript facade owns:

- conversion of user-facing TS input into the native call shape;
- Zod input parsing before crossing into Rust;
- Zod output parsing after the native promise resolves;
- stable JavaScript error classes and operation metadata.

The native addon owns:

- the canonical Rust HTTP client;
- auth, retries, timeouts, TLS, and transport errors;
- serialization at the Rust boundary;
- N-API-compatible primitive/object conversions.

The initial binding may cross the boundary using JSON-compatible objects to reduce coupling
between napi-rs-generated declarations and the long-term public TypeScript API. This is a
transport representation, not a public generic model: the TypeScript facade validates and
narrows it immediately.

## TypeScript verification

Generated TypeScript tests must prove:

- `tsc --noEmit` succeeds with strict mode;
- every generated schema parses valid fixture data;
- invalid required fields, enums, unions, and formats fail at runtime;
- unknown keys follow the OpenAPI policy;
- the client calls the Rust-backed native operation;
- response validation rejects malformed native output;
- typed API errors preserve status and safe fields;
- no generated public declaration contains `any`;
- the package loads through the napi-rs platform loader;
- the generated package passes Godlint and Godharness.

The generated repository CI must add a Node matrix covering at least the supported CI runtime and
the native build/test path. npm publication remains disabled until the platform artifact matrix
is complete.

## Python constraint

Python is not part of this first target, but the shared IR contract reserves the same guarantee:
Python generation must produce strict typed models (planned Pydantic output) and must fail rather
than emit generic `Any` fallbacks.

## Scope boundary

This target does not yet implement:

- Python output;
- browser-only TypeScript without a native runtime;
- a second TypeScript HTTP implementation;
- OAuth token acquisition;
- arbitrary custom schema validators;
- silent handling of unsupported JSON Schema keywords;
- npm publication from an incomplete platform matrix.

## Implementation sequence

1. Extend parser/IR with components schemas, references, compositions, and typed operation
   response/request metadata.
2. Add fixture-driven IR tests for objects, arrays, enums, nullable values, refs, and unions.
3. Generate typed Rust models and typed operation response wrappers.
4. Add generated napi-rs crate/package scaffolding backed by the Rust client.
5. Generate Zod schemas, inferred TypeScript types, typed facade, and validation errors.
6. Add Node/TypeScript build and native E2E CI jobs.
7. Run full Rust, TypeScript, Godlint, and Godharness verification, then update PR #25 or
   create a dependent PR if the branch boundary is too large.
