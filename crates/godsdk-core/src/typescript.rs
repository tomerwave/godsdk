#[path = "typescript/identifiers.rs"]
mod identifiers;
#[path = "typescript/native_render.rs"]
mod native_render;
#[path = "typescript/operations_render.rs"]
mod operations_render;
#[path = "typescript/schemas.rs"]
mod schemas;
#[path = "typescript/test_render.rs"]
mod test_render;

use super::code_writer::CodeWriter;
use super::{ApiIr, Operation, Schema};
use identifiers::{slug, ts_identifier};
use native_render::{render_native_cargo, render_native_package, render_native_rust};
use operations_render::render_operation;
use schemas::{
    inline_request_schema, inline_success_schema, operation_request_name, operation_response_name,
    render_schemas, render_types, schema_model_name,
};
use test_render::{render_client_test, render_validation_test};

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
        .map(|operation| render_error_contract(operation, spec))
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

fn render_error_contract(operation: &Operation, spec: &ApiIr) -> String {
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
                .and_then(|schema| schema_model_name(schema, spec))
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
                .and_then(|schema| schema_model_name(schema, spec))
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
                native_method_parameters(operation, spec)
            )
        })
        .collect::<String>();
    format!(
        "import binding from \"../native/index.js\";\nimport * as z from \"zod\";\n\nexport type NativeValue = unknown;\nexport const NativeValueSchema = z.unknown();\nexport type NativeResult = {{ ok: true; value: NativeValue }} | {{ ok: false; status: number; body: NativeValue }};\n\nexport interface NativeClient {{\n{methods}}}\n\ninterface NativeBinding {{\n  NativeClient: new (baseUrl: string) => NativeClient;\n}}\n\nconst nativeBinding = binding as NativeBinding;\n\nexport function loadNative(baseUrl: string) {{\n  return new nativeBinding.NativeClient(baseUrl);\n}}\n"
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
                native_method_parameters(operation, spec)
            )
        })
        .collect::<String>();
    format!(
        "import type * as z from \"zod\";\nexport type NativeValue = unknown;\nexport declare const NativeValueSchema: z.ZodType<NativeValue>;\nexport type NativeResult = {{ ok: true; value: NativeValue }} | {{ ok: false; status: number; body: NativeValue }};\n\nexport declare class NativeClient {{\n{methods}}}\n\ndeclare const binding: {{ NativeClient: typeof NativeClient }};\nexport default binding;\n"
    )
}

fn native_method_parameters(operation: &Operation, spec: &ApiIr) -> String {
    let mut parameters = Vec::new();
    if operation
        .request_body_details
        .as_ref()
        .is_some_and(|body| body.required)
    {
        parameters.push("requestBody: string".to_string());
    }
    parameters.extend(
        ordered_parameters(operation)
            .into_iter()
            .filter(|parameter| parameter.required)
            .map(|parameter| {
                let name = ts_identifier(&parameter.name);
                let ty = if parameter.location == super::ParameterLocation::Path {
                    "string".to_string()
                } else {
                    native_schema_type_name(&parameter.schema, spec)
                };
                format!("{name}: {ty}")
            }),
    );
    if operation
        .request_body_details
        .as_ref()
        .is_some_and(|body| !body.required)
    {
        parameters.push("requestBody?: string".to_string());
    }
    parameters.extend(
        ordered_parameters(operation)
            .into_iter()
            .filter(|parameter| !parameter.required)
            .map(|parameter| {
                let name = ts_identifier(&parameter.name);
                let ty = if parameter.location == super::ParameterLocation::Path {
                    "string".to_string()
                } else {
                    native_schema_type_name(&parameter.schema, spec)
                };
                format!("{name}?: {ty}")
            }),
    );
    parameters.join(", ")
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
        .filter(|operation| inline_success_schema(operation, spec).is_some())
        .map(operation_response_name)
        .collect::<Vec<_>>();
    let request_names = spec
        .operations
        .iter()
        .filter(|operation| inline_request_schema(operation, spec).is_some())
        .map(operation_request_name)
        .collect::<Vec<_>>();
    append_schema_imports(&mut imports, &response_names);
    append_schema_imports(&mut imports, &request_names);
    append_error_imports(&mut imports, spec);
    imports.push_str("import { SdkHttpError } from \"./errors.js\";\n");
    let mut type_names = spec.schemas.keys().cloned().collect::<Vec<_>>();
    type_names.extend(response_names);
    type_names.extend(request_names);
    let types = type_names
        .iter()
        .map(|name| type_alias_name(name))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "import * as z from \"zod\";\nimport {{ loadNative, type NativeClient, type NativeValue, NativeValueSchema }} from \"./native.js\";\nimport type {{ {types} }} from \"./types.js\";\n{imports}\nexport * from \"./schemas.js\";\nexport * from \"./types.js\";\nexport * from \"./errors.js\";\n\nexport class Client {{\n  private readonly native: NativeClient;\n\n  constructor(baseUrl: string) {{\n    this.native = loadNative(baseUrl);\n  }}\n\n"
    )
}

fn append_schema_imports(imports: &mut String, names: &[String]) {
    let mut names = names.to_vec();
    names.sort();
    names.dedup();
    for name in names {
        imports.push_str(&format!(
            "import {{ {name}Schema }} from \"./schemas.js\";\n"
        ));
    }
}

fn append_error_imports(imports: &mut String, spec: &ApiIr) {
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
}

fn render_index_methods(spec: &ApiIr) -> String {
    spec.operations
        .iter()
        .map(|operation| render_operation(operation, spec))
        .collect()
}

fn schema_type_name(schema: &Schema, spec: &ApiIr) -> String {
    match schema {
        Schema::Reference(name) if spec.schemas.contains_key(name) => type_alias_name(name),
        Schema::String {
            format: Some(format),
        } if format == "binary" => "Uint8Array".to_string(),
        Schema::String { .. } => "string".to_string(),
        Schema::Integer { .. } | Schema::Number { .. } => "number".to_string(),
        Schema::Boolean => "boolean".to_string(),
        Schema::Null => "null".to_string(),
        Schema::Array(item) => format!("{}[]", schema_type_name(item, spec)),
        _ => "NativeValue".to_string(),
    }
}

pub(super) fn type_alias_name(name: &str) -> String {
    if name.ends_with("Schema") {
        format!("{name}Type")
    } else {
        name.to_string()
    }
}

fn native_schema_type_name(schema: &Schema, spec: &ApiIr) -> String {
    match schema {
        Schema::String {
            format: Some(format),
        } if format == "binary" => "Uint8Array".to_string(),
        Schema::String { .. } => "string".to_string(),
        Schema::Integer { .. } | Schema::Number { .. } => "number".to_string(),
        Schema::Boolean => "boolean".to_string(),
        _ => {
            let _ = spec;
            "NativeValue".to_string()
        }
    }
}

fn ordered_parameters(operation: &Operation) -> Vec<&crate::Parameter> {
    operation
        .parameters
        .iter()
        .filter(|parameter| parameter.required)
        .chain(
            operation
                .parameters
                .iter()
                .filter(|parameter| !parameter.required),
        )
        .collect()
}

fn render_readme(spec: &ApiIr) -> String {
    CodeWriter::from_lines([
        ["# ", &spec.title, " TypeScript SDK"].concat(),
        String::new(),
        "Install dependencies, then run `npm run test:native`. The command builds the Rust-backed napi-rs addon, starts a local mock API, and verifies runtime response validation with Zod.".to_string(),
    ])
}
