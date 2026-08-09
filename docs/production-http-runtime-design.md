# Production HTTP Runtime Design

**Status:** Proposed
**Date:** 2026-08-09

## Goal

Generated Rust SDKs must be usable as production clients rather than only as transport
demonstrations. The generated crate should provide one stable, strongly typed client core that
can later be reused by Python, Node, WebAssembly, mobile, and other bindings.

The generator remains responsible for producing the repository and its source. The generated
Rust crate owns HTTP behavior. The caller owns the Tokio runtime; the SDK must not create a
global runtime or hide blocking work inside async methods.

## Decision

Generate an async-first Rust client using `reqwest` on Tokio, with a small generated public API
around a reusable `reqwest::Client`.

The generated crate will use a production HTTP library rather than maintaining a custom socket
transport. `reqwest` provides HTTP/1.1 and HTTP/2, connection pooling, TLS integrations,
redirect policy, proxy support, and request/response streaming. Tokio supplies the executor,
network I/O, and timers, but callers remain free to choose a current-thread or multi-thread
runtime.

References:

- <https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html>
- <https://docs.rs/reqwest/latest/reqwest/>
- <https://docs.rs/tokio/latest/tokio/runtime/index.html>
- <https://tokio.rs/tokio/topics/bridging>

## Generated public API

The first production runtime slice will generate these stable concepts:

```rust
pub struct Client { /* shared reqwest client and generated configuration */ }
pub struct ClientBuilder { /* validated, immutable client configuration */ }
pub struct RequestContext { /* safe request metadata and optional correlation id */ }
pub struct ApiError { /* transport, serialization, timeout, and HTTP status details */ }
pub struct ErrorResponse { /* status, headers, and bounded response body */ }
```

The intended usage is:

```rust
let client = Client::builder("https://api.example.com")
    .bearer_token(token)
    .timeout(Duration::from_secs(30))
    .build()?;

let result = client.get_pet("pet-1").await?;
```

Generated operation methods will be async, `Send`, and borrow the client. A single `Client` is
safe to clone and reuse across concurrent tasks. The SDK will not expose the underlying
`reqwest::Client` as part of the initial generated API, but will provide configuration hooks
for the behaviors bindings need.

## Request construction

The generator will normalize and render:

- path parameters with URL-safe encoding;
- query parameters with OpenAPI serialization rules for the supported styles;
- header parameters and generated default headers;
- JSON request bodies using `serde` and `serde_json`;
- content negotiation through `Accept` and `Content-Type`;
- operation-specific request types and response types once the schema IR supports them.

Until typed schema generation is complete, opaque response bodies may remain available as a
compatibility escape hatch. That escape hatch must not prevent the runtime from enforcing
status handling, size limits, timeout behavior, or content decoding.

## Configuration and safety defaults

`ClientBuilder` will validate the base URL before constructing the client and will make the
following behavior explicit and configurable:

- total request timeout, with a conservative nonzero default;
- connection timeout;
- optional read timeout for stalled response bodies;
- maximum response body size for error payloads;
- user-agent and default headers;
- redirect policy, defaulting to a bounded safe policy;
- proxy behavior delegated to reqwest's configured environment/system behavior;
- TLS backend selected through generated Cargo features, defaulting to rustls;
- optional custom root certificates and mTLS configuration behind explicit features;
- optional request correlation metadata without logging secrets.

Secrets must never be included in `Debug` output, generated error strings, or tracing fields.
Authorization headers and sensitive configured headers will be redacted in diagnostics.

## Authentication

The initial runtime contract will support generated configuration for the common OpenAPI
security schemes:

- HTTP bearer authentication;
- API keys in headers or query parameters;
- HTTP basic authentication.

Authentication will be applied centrally during request construction so generated operations do
not duplicate security logic. OAuth token acquisition/refresh, signed requests, and arbitrary
custom authentication will be represented as future extension points rather than implemented as
partial bespoke mechanisms in this slice.

## Errors and status handling

Every generated operation will distinguish at least:

- invalid client configuration;
- transport and connection failures;
- timeout and cancellation;
- serialization/deserialization failures;
- non-success HTTP status responses;
- response body limit violations.

Non-success responses will preserve the status code, selected safe headers, and a bounded body
for diagnostics or typed error decoding. The runtime will not silently convert an HTTP error into
a successful empty response. Error formatting will avoid echoing credentials or unrestricted
server-controlled payloads.

## Retries and resilience

Retries will be opt-in through a generated `RetryPolicy`, with conservative defaults:

- no retry for non-idempotent operations unless explicitly enabled;
- retry transient transport failures and configured status codes;
- exponential backoff with a maximum delay and jitter;
- respect a valid `Retry-After` value when it is within the configured limit;
- stop at the attempt count or overall request deadline;
- do not retry after a response body has been handed to the caller.

The first implementation may keep the policy in the generated runtime rather than introduce a
second middleware abstraction. If the policy later needs load balancing, rate limiting, or
external middleware composition, the design can adopt the Tokio ecosystem's Tower layer model
without changing generated operation signatures.

## Observability

The runtime will expose an optional `tracing` feature. When enabled, it emits structured events
for request start, retry, response status, and failure. It will record method, URL host/path
metadata, attempt count, and latency, while excluding authorization, cookies, request bodies,
and response bodies by default. The feature must not be required for normal consumers.

## Feature and dependency policy

Generated Cargo manifests will use narrow, documented features:

- `default = ["rustls-tls"]`;
- `rustls-tls` for the default TLS backend;
- `native-tls` as an opt-in alternative where platform trust stores are required;
- `tracing` for optional diagnostics;
- `blocking` only after a deliberate sync adapter design, not as a second implementation of
  every generated operation.

The generator will pin compatible dependency ranges through the repository's existing
dependency-update policy and will generate a valid lockfile by invoking Cargo generation in the
output repository. It will not hand-maintain a fake lockfile as runtime dependencies grow.

The generated crate should depend on established ecosystem libraries (`reqwest`, `tokio`,
`serde`, `serde_json`, `url`, and narrowly selected supporting crates) instead of implementing
HTTP, TLS, URL encoding, retry timing, or tracing primitives itself. New dependencies require a
clear capability gap and a license/build/maintenance check.

## Testing contract

The generated repository will include real local HTTP integration tests, not only unit tests.
The test matrix will cover:

- successful JSON request/response;
- path and query encoding;
- request headers and authentication;
- timeout and connection failure;
- non-success status with bounded error body;
- malformed JSON response;
- retry success after transient failures;
- no retry for unsafe operations by default;
- redirect policy;
- concurrent reuse of one client;
- TLS behavior in a separately gated test where certificates can be controlled;
- Godlint and Godharness checks on the generated repository.

The local mock server will be replaced or extended with an established test server library if
that reduces custom protocol parsing. Tests must remain deterministic and must not require an
internet connection.

## Scope boundaries

This design intentionally does not implement all schema and binding work at once. The runtime
slice depends on the IR being able to represent request/response schemas and security schemes,
but it may initially use generated opaque JSON values where typed models are not yet available.

Out of scope for the first runtime slice:

- OAuth discovery and token refresh;
- websocket, SSE, and multipart streaming APIs;
- automatic pagination helpers;
- generated language bindings;
- user-supplied arbitrary middleware graphs;
- remote specification fetching during generation;
- a generator-owned Tokio runtime.

## Alternatives considered

### Keep the custom synchronous TCP transport

Rejected because it would require GodSDK to own HTTP parsing, TLS, pooling, HTTP/2, proxying,
redirects, cancellation, and resilience behavior. That is a large security and maintenance
surface and would not be a credible production foundation for future bindings.

### Use reqwest blocking as the primary API

Rejected as the primary model because generated SDKs will be called from concurrent services and
language bindings. Blocking calls would either occupy threads or require every binding to build
its own async bridge. A blocking adapter remains possible after the async contract is stable.

### Build directly on hyper/http and Tower

Rejected for the first slice because it exposes more lower-level policy choices and increases the
generated runtime surface. Tower remains a possible internal extension once retry, rate limiting,
or middleware composition outgrows the local policy implementation.

## Implementation sequencing

1. Add the generated runtime dependency/features and replace the minimal transport with an async
   `ClientBuilder`/`Client` API.
2. Add centralized URL, header, auth, timeout, and error handling.
3. Add bounded retries and optional tracing.
4. Extend the IR and renderer for typed JSON request/response models and security schemes.
5. Expand generated integration tests and fixtures, then verify Godlint/Godharness and lockfile
   reproducibility.

