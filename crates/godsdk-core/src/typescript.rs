#[path = "typescript/identifiers.rs"]
mod identifiers;
#[path = "typescript/schemas.rs"]
mod schemas;

use super::code_writer::CodeWriter;
use super::rust_ast::mock_success_body;
use super::{ApiIr, Operation};
use identifiers::{slug, ts_identifier};
use schemas::{
    inline_success_schema, operation_response_name, render_schemas, render_types, schema_model_name,
};

pub(crate) fn render_typescript_files(spec: &ApiIr) -> Vec<(&'static str, String)> {
    vec![
        ("sdk/typescript/package.json", render_package(spec)),
        ("sdk/typescript/tsconfig.json", render_tsconfig()),
        ("sdk/typescript/src/schemas.ts", render_schemas(spec)),
        ("sdk/typescript/src/types.ts", render_types(spec)),
        ("sdk/typescript/src/errors.ts", render_errors(spec)),
        ("sdk/typescript/src/native.ts", render_native_loader(spec)),
        (
            "sdk/typescript/native/index.d.ts",
            render_native_declaration(spec),
        ),
        ("sdk/typescript/src/index.ts", render_index(spec)),
        (
            "sdk/typescript/tests/validation.test.ts",
            render_validation_test(spec),
        ),
        (
            "sdk/typescript/tests/client.test.ts",
            render_client_test(spec),
        ),
        ("sdk/typescript/README.md", render_readme(spec)),
        (
            "sdk/typescript/native/Cargo.toml",
            render_native_cargo(spec),
        ),
        (
            "sdk/typescript/native/package.json",
            render_native_package(),
        ),
        ("sdk/typescript/native/src/lib.rs", render_native_rust(spec)),
    ]
}

fn render_package(spec: &ApiIr) -> String {
    let package = slug(&spec.title);
    CodeWriter::from_lines(package_lines(&package))
}

fn package_lines(package: &str) -> Vec<String> {
    vec![
        "{".to_string(),
        format!("  \"name\": \"{package}-sdk\","),
        "  \"version\": \"0.1.0\",".to_string(),
        "  \"type\": \"module\",".to_string(),
        "  \"main\": \"./dist/index.js\",".to_string(),
        "  \"exports\": {\".\": \"./dist/index.js\"},".to_string(),
        "  \"scripts\": {\"build\": \"tsc --noEmit\", \"build:native\": \"napi build --manifest-path native/Cargo.toml --platform --release\", \"test\": \"vitest run\", \"test:native\": \"npm run build:native && npm test\", \"prepublishOnly\": \"napi prepublish -t npm --no-gh-release --root-publisher npm\"},".to_string(),
        format!("  \"napi\": {{\"binaryName\": \"{package}-sdk\", \"packageName\": \"{package}-sdk\", \"targets\": [\"x86_64-unknown-linux-gnu\", \"x86_64-unknown-linux-musl\", \"aarch64-unknown-linux-gnu\", \"aarch64-unknown-linux-musl\", \"x86_64-apple-darwin\", \"aarch64-apple-darwin\", \"x86_64-pc-windows-msvc\"]}},"),
        "  \"dependencies\": {\"zod\": \"^4.4.3\"},".to_string(),
        "  \"devDependencies\": {\"@napi-rs/cli\": \"^3.8.3\", \"@types/node\": \"^22.0.0\", \"tsx\": \"^4.20.3\", \"typescript\": \"^5.0.0\", \"vitest\": \"^3.0.0\"}".to_string(),
        "}".to_string(),
    ]
}

fn render_tsconfig() -> String {
    CodeWriter::from_lines([
        "{",
        "  \"compilerOptions\": {",
        "    \"target\": \"ES2022\",",
        "    \"module\": \"NodeNext\",",
        "    \"moduleResolution\": \"NodeNext\",",
        "    \"strict\": true,",
        "    \"declaration\": true,",
        "    \"noUncheckedIndexedAccess\": true,",
        "    \"exactOptionalPropertyTypes\": true,",
        "    \"noImplicitOverride\": true,",
        "    \"outDir\": \"dist\"",
        "  },",
        "  \"include\": [\"src/**/*.ts\", \"tests/**/*.ts\"]",
        "}",
    ])
}

fn render_errors(spec: &ApiIr) -> String {
    let imports = spec
        .schemas
        .keys()
        .filter(|name| spec.operations.iter().any(|operation| {
            operation
                .responses
                .iter()
                .filter(|response| !response.status.starts_with('2'))
                .any(|response| matches!(response.schema, Some(super::Schema::Reference(ref reference)) if reference == *name))
        }))
        .collect::<Vec<_>>();
    let contracts = spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
        .map(render_error_contract)
        .collect::<String>();
    CodeWriter::from_lines(error_file_lines(&imports, &contracts))
}

fn error_file_lines(imports: &[&String], contracts: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for name in imports {
        lines.push(format!("import type {{ {name} }} from \"./types.js\";"));
        lines.push(format!("import {{ {name}Schema }} from \"./schemas.js\";"));
    }
    lines.extend([
        "import type { NativeValue } from \"./native.js\";".to_string(),
        String::new(),
        "export type NativeResult = { ok: true; value: NativeValue } | { ok: false; status: number; body: NativeValue };".to_string(),
        String::new(),
        "export class SdkValidationError extends Error {".to_string(),
        "  readonly operation: string;".to_string(),
        "  readonly model: string;".to_string(),
        String::new(),
        "  constructor(operation: string, model: string) {".to_string(),
        "    super(`Response validation failed for ${operation} (${model})`);".to_string(),
        "    this.name = \"SdkValidationError\";".to_string(),
        "    this.operation = operation;".to_string(),
        "    this.model = model;".to_string(),
        "  }".to_string(),
        "}".to_string(),
        String::new(),
        "export class SdkHttpError extends Error {".to_string(),
        "  readonly status: number;".to_string(),
        "  readonly body: NativeValue;".to_string(),
        String::new(),
        "  constructor(status: number, body: NativeValue) {".to_string(),
        "    super(`API returned HTTP ${status}`);".to_string(),
        "    this.name = \"SdkHttpError\";".to_string(),
        "    this.status = status;".to_string(),
        "    this.body = body;".to_string(),
        "  }".to_string(),
        "}".to_string(),
        String::new(),
    ]);
    lines.extend(contracts.trim_end().lines().map(str::to_string));
    lines
}

fn has_error_responses(operation: &Operation) -> bool {
    operation
        .responses
        .iter()
        .any(|response| !response.status.starts_with('2'))
}

fn render_error_contract(operation: &Operation) -> String {
    let operation_name = type_identifier(&operation.operation_id);
    let name = format!("{operation_name}Error");
    let variants = operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
        .filter_map(|response| {
            let status = response.status.parse::<u16>().ok()?;
            let body_type = response
                .schema
                .as_ref()
                .and_then(schema_model_name)
                .unwrap_or_else(|| "NativeValue".to_string());
            Some(vec![
                format!("export class {operation_name}Status{status}Error extends {name} {{"),
                format!("  readonly typedBody: {body_type};"),
                String::new(),
                format!("  constructor(status: number, body: {body_type}) {{"),
                "    super(status, body);".to_string(),
                "    this.typedBody = body;".to_string(),
                "  }".to_string(),
                "}".to_string(),
                String::new(),
            ])
        })
        .flatten()
        .collect::<Vec<_>>();
    let arms = operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
        .filter_map(|response| {
            let status = response.status.parse::<u16>().ok()?;
            let constructor = response
                .schema
                .as_ref()
                .and_then(schema_model_name)
                .map_or_else(|| format!("new {operation_name}Status{status}Error({status}, result.body)"), |model| {
                    format!("new {operation_name}Status{status}Error({status}, {model}Schema.parse(result.body))")
                });
            Some(format!("      case {status}: return {constructor};"))
        })
        .collect::<Vec<_>>();
    CodeWriter::from_lines(error_contract_lines(&name, &arms, &variants))
}

fn error_contract_lines(name: &str, arms: &[String], variants: &[String]) -> Vec<String> {
    let mut lines = vec![
        format!("export class {name} extends SdkHttpError {{"),
        format!("  static from(result: {{ status: number; body: NativeValue }}): {name} {{"),
        "    switch (result.status) {".to_string(),
    ];
    lines.extend(arms.iter().cloned());
    lines.extend([
        format!("      default: return new {name}(result.status, result.body);"),
        "    }".to_string(),
        "  }".to_string(),
        "}".to_string(),
        String::new(),
    ]);
    lines.extend(variants.iter().cloned());
    lines
}

fn type_identifier(value: &str) -> String {
    value
        .split(['-', '_', ' ', '.'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

fn render_native_loader(spec: &ApiIr) -> String {
    let methods = spec
        .operations
        .iter()
        .map(|operation| {
            format!(
                "  {}({}): Promise<NativeResult>;\n",
                ts_identifier(&operation.operation_id),
                operation
                    .parameters
                    .iter()
                    .filter(|parameter| parameter.location == super::ParameterLocation::Path)
                    .map(|parameter| format!("{}: string", ts_identifier(&parameter.name)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<String>();
    format!(
        "import binding from \"../native/index.js\";\n\nexport type NativeValue = null | boolean | number | string | NativeValue[] | {{ [key: string]: NativeValue }};\nexport type NativeResult = {{ ok: true; value: NativeValue }} | {{ ok: false; status: number; body: NativeValue }};\n\nexport interface NativeClient {{\n{methods}}}\n\ninterface NativeBinding {{\n  NativeClient: new (baseUrl: string) => NativeClient;\n}}\n\nconst nativeBinding = binding as NativeBinding;\n\nexport function loadNative(baseUrl: string): NativeClient {{\n  return new nativeBinding.NativeClient(baseUrl);\n}}\n"
    )
}

fn render_native_declaration(spec: &ApiIr) -> String {
    let methods = spec
        .operations
        .iter()
        .map(|operation| {
            format!(
                "  {}({}): Promise<NativeResult>;\n",
                ts_identifier(&operation.operation_id),
                operation
                    .parameters
                    .iter()
                    .filter(|parameter| parameter.location == super::ParameterLocation::Path)
                    .map(|parameter| format!("{}: string", ts_identifier(&parameter.name)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<String>();
    format!(
        "export type NativeValue = null | boolean | number | string | NativeValue[] | {{ [key: string]: NativeValue }};\nexport type NativeResult = {{ ok: true; value: NativeValue }} | {{ ok: false; status: number; body: NativeValue }};\n\nexport declare class NativeClient {{\n{methods}}}\n\ndeclare const binding: {{ NativeClient: typeof NativeClient }};\nexport default binding;\n"
    )
}

fn render_index(spec: &ApiIr) -> String {
    format!(
        "{}{}}}\n",
        render_index_header(spec),
        render_index_methods(spec)
    )
}

fn render_index_header(spec: &ApiIr) -> String {
    let mut imports = spec
        .schemas
        .keys()
        .map(|name| format!("import {{ {name}Schema }} from \"./schemas.js\";\n"))
        .collect::<String>();
    let response_names = spec
        .operations
        .iter()
        .filter(|operation| inline_success_schema(operation).is_some())
        .map(operation_response_name)
        .collect::<Vec<_>>();
    for name in &response_names {
        imports.push_str(&format!(
            "import {{ {name}Schema }} from \"./schemas.js\";\n"
        ));
    }
    for operation in spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
    {
        imports.push_str(&format!(
            "import {{ {}Error }} from \"./errors.js\";\n",
            type_identifier(&operation.operation_id)
        ));
    }
    imports.push_str("import { SdkHttpError } from \"./errors.js\";\n");
    let mut type_names = spec.schemas.keys().cloned().collect::<Vec<_>>();
    type_names.extend(response_names);
    let types = type_names.join(", ");
    format!(
        "import * as z from \"zod\";\nimport {{ loadNative, type NativeClient }} from \"./native.js\";\nimport type {{ {types} }} from \"./types.js\";\n{imports}\nexport * from \"./schemas.js\";\nexport * from \"./types.js\";\nexport * from \"./errors.js\";\n\nexport class Client {{\n  private readonly native: NativeClient;\n\n  constructor(baseUrl: string) {{\n    this.native = loadNative(baseUrl);\n  }}\n\n"
    )
}

fn render_index_methods(spec: &ApiIr) -> String {
    spec.operations
        .iter()
        .map(|operation| render_operation(operation, spec))
        .collect()
}

fn render_operation(operation: &Operation, _spec: &ApiIr) -> String {
    let parameters = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == super::ParameterLocation::Path)
        .map(|parameter| format!("{}: string", ts_identifier(&parameter.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let response = operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .and_then(|response| response.schema.as_ref());
    let method = ts_identifier(&operation.operation_id);
    let arguments = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == super::ParameterLocation::Path)
        .map(|parameter| ts_identifier(&parameter.name))
        .collect::<Vec<_>>()
        .join(", ");
    let error = has_error_responses(operation)
        .then(|| format!("{}Error", type_identifier(&operation.operation_id)));
    let error_handling = error.as_ref().map_or_else(
        || "      throw new SdkHttpError(result.status, result.body);\n".to_string(),
        |error| format!("      throw {}.from(result);\n", error),
    );
    let success = response.map_or_else(
        || "    return;\n".to_string(),
        |response| {
            let model =
                schema_model_name(response).unwrap_or_else(|| operation_response_name(operation));
            let parser = format!("{model}Schema");
            format!("    return {parser}.parse(result.value);\n")
        },
    );
    let return_type = response
        .map(|response| {
            schema_model_name(response).unwrap_or_else(|| operation_response_name(operation))
        })
        .unwrap_or_else(|| "void".to_string());
    format!(
        "  async {method}({parameters}): Promise<{return_type}> {{\n    const result = await this.native.{method}({arguments});\n    if (!result.ok) {{\n{error_handling}    }}\n{success}  }}\n\n"
    )
}

fn render_validation_test(spec: &ApiIr) -> String {
    let Some(name) = spec.schemas.keys().next() else {
        return "import { describe, it } from \"vitest\";\n\ndescribe(\"generated schemas\", () => { it(\"has no models\", () => {}); });\n".to_string();
    };
    format!(
        "import {{ describe, expect, it }} from \"vitest\";\nimport {{ {name}Schema }} from \"../src/schemas.js\";\n\ndescribe(\"generated schemas\", () => {{\n  it(\"rejects invalid {name}\", () => {{\n    expect(() => {name}Schema.parse({{}})).toThrow();\n  }});\n}});\n"
    )
}

fn render_client_test(spec: &ApiIr) -> String {
    let Some(operation) = spec.operations.first() else {
        return "import { describe, it } from \"vitest\";\n\ndescribe(\"generated client\", () => { it(\"has no operations\", () => {}); });\n".to_string();
    };
    let method = ts_identifier(&operation.operation_id);
    let success_json =
        String::from_utf8(mock_success_body(spec, operation)).unwrap_or_else(|_| "{}".to_string());
    let arguments = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == super::ParameterLocation::Path)
        .map(|_| "\"pet-1\"")
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "import {{ createServer }} from \"node:http\";\nimport {{ afterAll, beforeAll, describe, expect, it }} from \"vitest\";\nimport {{ Client }} from \"../src/index.js\";\n\nconst server = createServer((_request, response) => {{\n  response.writeHead(200, {{ \"content-type\": \"application/json\" }});\n  response.end(JSON.stringify({success_json}));\n}});\nlet baseUrl = \"\";\n\nbeforeAll(async () => {{\n  await new Promise<void>((resolve) => server.listen(0, \"127.0.0.1\", resolve));\n  const address = server.address();\n  if (address === null || typeof address === \"string\") throw new Error(\"mock server did not bind\");\n  baseUrl = `http://127.0.0.1:${{address.port}}`;\n}});\n\nafterAll(() => server.close());\n\ndescribe(\"generated native client\", () => {{\n  it(\"calls the Rust-backed local mock API\", async () => {{\n    const response = await new Client(baseUrl).{method}({arguments});\n    expect(response).toEqual({success_json});\n  }});\n}});\n"
    )
}

fn render_readme(spec: &ApiIr) -> String {
    format!(
        "# {} TypeScript SDK\n\nInstall dependencies, then run `npm run test:native`. The command builds the Rust-backed napi-rs addon, starts a local mock API, and verifies runtime response validation with Zod.\n",
        spec.title
    )
}

fn render_native_cargo(spec: &ApiIr) -> String {
    let crate_name = rust_crate_name(spec);
    format!(
        "[package]\nname = \"{}-typescript-native\"\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.97\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nnapi = {{ version = \"3.12\", features = [\"napi9\", \"tokio_rt\", \"serde-json\"] }}\nnapi-derive = \"3.6\"\nserde_json = \"1\"\n{} = {{ package = \"{}-sdk\", path = \"../../rust\" }}\n",
        slug(&spec.title),
        crate_name,
        slug(&spec.title),
    )
}

fn render_native_package() -> String {
    "{\n  \"type\": \"commonjs\"\n}\n".to_string()
}

fn render_native_rust(spec: &ApiIr) -> String {
    let methods = spec
        .operations
        .iter()
        .map(render_native_operation)
        .collect::<String>();
    let error_imports = spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
        .map(|operation| format!("{}Error", type_identifier(&operation.operation_id)))
        .collect::<Vec<_>>();
    let error_imports = if error_imports.is_empty() {
        String::new()
    } else {
        format!(", {}", error_imports.join(", "))
    };
    format!(
        "use napi::bindgen_prelude::*;\nuse napi_derive::napi;\nuse {}::{{Client as RustClient, SdkError{error_imports}}};\n\n#[napi]\npub struct NativeClient {{\n    inner: RustClient,\n}}\n\n#[napi]\nimpl NativeClient {{\n    #[napi(constructor)]\n    pub fn new(base_url: String) -> Result<Self> {{\n        let inner = RustClient::builder(base_url).build().map_err(to_napi_error)?;\n        Ok(Self {{ inner }})\n    }}\n\n{methods}}}\n\nfn to_napi_error(error: impl std::fmt::Display) -> Error {{\n    Error::from_reason(error.to_string())\n}}\n",
        rust_crate_name(spec),
    )
}

fn render_native_operation(operation: &Operation) -> String {
    let parameters = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == super::ParameterLocation::Path)
        .map(|parameter| format!(", {}: String", super::rust_identifier(&parameter.name)))
        .collect::<String>();
    let arguments = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == super::ParameterLocation::Path)
        .map(|parameter| format!("&{}", super::rust_identifier(&parameter.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let method = super::rust_identifier(&operation.operation_id);
    let body = if has_error_responses(operation) {
        let error_type = format!("{}Error", type_identifier(&operation.operation_id));
        let arms = operation
            .responses
            .iter()
            .filter(|response| !response.status.starts_with('2'))
            .filter_map(|response| {
                let status = response.status.parse::<u16>().ok()?;
                let variant = format!("Status{status}");
                let (pattern, body) = response.schema.as_ref().map_or_else(
                    || (format!("{error_type}::{variant}"), "serde_json::Value::Null".to_string()),
                    |_| (format!("{error_type}::{variant}(value)"), "value".to_string()),
                );
                Some(format!(
                    "            Err({pattern}) => Ok(serde_json::json!({{\"ok\": false, \"status\": {status}, \"body\": {body}}})),\n"
                ))
            })
            .collect::<String>();
        format!(
            "        match self.inner.{method}({arguments}).await {{\n            Ok(value) => Ok(serde_json::json!({{\"ok\": true, \"value\": value}})),\n            Err({error_type}::Unexpected {{ status, body }}) => Ok(serde_json::json!({{\"ok\": false, \"status\": status, \"body\": body}})),\n            Err({error_type}::Transport(error)) => Err(to_napi_error(error)),\n{arms}        }}"
        )
    } else {
        format!(
            "        match self.inner.{method}({arguments}).await {{\n            Ok(value) => Ok(serde_json::json!({{\"ok\": true, \"value\": value}})),\n            Err(SdkError::Http {{ status, body }}) => Ok(serde_json::json!({{\"ok\": false, \"status\": status, \"body\": body}})),\n            Err(error) => Err(to_napi_error(error)),\n        }}"
        )
    };
    format!(
        "    #[napi]\n    pub async fn {method}(&self{parameters}) -> Result<serde_json::Value> {{\n{body}\n    }}\n\n",
    )
}

fn rust_crate_name(spec: &ApiIr) -> String {
    slug(&spec.title).replace('-', "_")
}
