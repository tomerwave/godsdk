godsdk Architecture & Design Specification
1. Executive Summary
Traditional SDK generators emit standalone, unoptimized client libraries for every target language. This approach leads to fragmented logic, duplicate bug fixes across multiple codebases, and inconsistent runtime performance.
⁠godsdk⁠ solves this by generating a single, robust, strongly typed Rust HTTP client core from OpenAPI (or custom) specs, and then exposing that single core to other target ecosystems through zero-cost native FFI bindings or runtime addons (e.g., ⁠PyO3⁠, ⁠napi-rs⁠, ⁠wasm-bindgen⁠, ⁠UniFFI⁠).
2. Key Advantages
1. Write Once, Fix Everywhere: Protocol logic, security headers, auth token refresh loops, rate limiting, and HTTP retry policies live entirely inside the Rust core crate.
2. Type Safety & Performance: Leverage Rust's memory safety, fearless concurrency, and zero-cost abstractions across all higher-level language bindings.
3. Consistent Behavior: Every language wrapper uses the same underlying execution engine, guaranteeing consistent response parsing and error handling.
4. Single-Binary CLI Generator: Built in Rust, ⁠godsdk⁠ compiles to a dependency-free binary that runs lightning-fast in CI/CD pipelines.
3. System Architecture & Pipeline
The ⁠godsdk⁠ engine processes specs through four modular stages:
Stage 1: Ingestion
￼ Parses OpenAPI 3.0/3.1 JSON/YAML schemas.
￼ Resolves local and remote recursive ⁠$ref⁠ dependencies.
￼ Validates schema completeness and flags ambiguous endpoints.
Stage 2: Intermediate Representation (IR) Normalization
The raw OpenAPI object is transformed into an internal, strongly typed Rust IR struct tree (⁠godsdk_ir⁠).
￼ Endpoints: HTTP methods, path strings, query parameters, header keys, request payloads, and status codes.
￼ Models: Struct definitions, enums (tagged/untagged), recursive types, optional vs. mandatory fields.
￼ Authentication: Bearer tokens, API keys, OAuth2 grant structures.
￼ Casing Normalization: Converts spec strings into canonical cases using ⁠heck⁠ (⁠snake_case⁠ for Rust/Python, ⁠camelCase⁠ for JS/TS).
Stage 3: Core Rust Crate Generator
Generates a publishable Rust crate (⁠<sdk-name>-core⁠):
￼ HTTP Client Engine: ⁠reqwest⁠ + ⁠tokio⁠ (or lightweight ⁠ureq⁠ for sync targets).
￼ Serialization Layer: ⁠serde⁠ + ⁠serde_json⁠.
￼ Templating: ⁠askama⁠ (Jinja-style templates pre-compiled into the ⁠godsdk⁠ binary for speed).
Stage 4: Multi-Language Bindings Engine
Generates ecosystem-native wrappers around the Core Rust crate:
Target Language / Platform
Binding Engine
Output Artifact
Python
PyO3 + maturin
Native Wheel (.whl), asyncio compatible
Node.js / TypeScript
napi-rs
.node native binary + .d.ts type declarations
Browser / Web
wasm-bindgen
WebAssembly package + JS glue code
Swift / Kotlin / Go
UniFFI / Diplomat
Native CFFI bridges and JNI/Swift interfaces

4. Recommended Tech Stack
￼ Generator CLI: ⁠clap⁠ (v4 with derive macros)
￼ Spec Parsing: ⁠openapiv3⁠, ⁠serde_yaml⁠, ⁠serde_json⁠, ⁠url⁠
￼ Templating Engine: ⁠askama⁠ or ⁠rinja⁠
￼ Rust Formatting: ⁠prettyplease⁠ or standard ⁠rustfmt⁠ execution
￼ Case Transformations: ⁠heck⁠
￼ Type Synthesis: ⁠typify⁠ (for JSON Schema conversion)
5. Development Roadmap
Phase 1: Core Engine & Ingestion (V0.1)
￼ [ ] Implement CLI interface for ⁠godsdk⁠ (⁠godsdk generate -s spec.yaml -o ./out⁠).
￼ [ ] Build OpenAPI 3.0/3.1 parser into normalized ⁠godsdk_ir⁠.
￼ [ ] Implement ⁠askama⁠ templates for generating standalone, idiomatic Rust SDK crates.
Phase 2: Python & Node.js Bindings (V0.2)
￼ [ ] Add ⁠PyO3⁠ binding generation for native Python modules with ⁠async/await⁠ support.
￼ [ ] Add ⁠napi-rs⁠ generator for Node.js/TypeScript native addons with generated ⁠.d.ts⁠ types.
Phase 3: WebAssembly & Mobile Bridge (V0.3)
￼ [ ] Add ⁠wasm-bindgen⁠ build profile for browser compatibility.
￼ [ ] Integrate ⁠UniFFI⁠ template generation for Swift and Kotlin targets.
Phase 4: Developer Tooling & Plugin Ecosystem (V1.0)
￼ [ ] Add custom extension hooks for user-defined templates.
￼ [ ] Implement interactive dry-runs and schema diff warnings.