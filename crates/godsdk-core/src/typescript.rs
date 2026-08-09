use super::{ApiSpec, Operation, Schema};

pub(crate) fn render_typescript_files(spec: &ApiSpec) -> Vec<(&'static str, String)> {
    vec![
        ("sdk/typescript/package.json", render_package(spec)),
        ("sdk/typescript/tsconfig.json", render_tsconfig()),
        ("sdk/typescript/src/schemas.ts", render_schemas(spec)),
        ("sdk/typescript/src/types.ts", render_types(spec)),
        ("sdk/typescript/src/errors.ts", render_errors()),
        ("sdk/typescript/src/native.ts", render_native(spec)),
        ("sdk/typescript/src/index.ts", render_index(spec)),
        (
            "sdk/typescript/tests/validation.test.ts",
            render_validation_test(spec),
        ),
        ("sdk/typescript/README.md", render_readme(spec)),
    ]
}

fn render_package(spec: &ApiSpec) -> String {
    format!(
        "{{\n  \"name\": \"{}-sdk\",\n  \"version\": \"0.1.0\",\n  \"type\": \"module\",\n  \"exports\": {{\".\": \"./dist/index.js\"}},\n  \"scripts\": {{\"build\": \"tsc --noEmit\", \"test\": \"vitest run\"}},\n  \"dependencies\": {{\"zod\": \"^4.0.0\"}},\n  \"devDependencies\": {{\"@types/node\": \"^22.0.0\", \"typescript\": \"^5.0.0\", \"vitest\": \"^3.0.0\"}}\n}}\n",
        slug(&spec.title)
    )
}

fn render_tsconfig() -> String {
    "{\n  \"compilerOptions\": {\n    \"target\": \"ES2022\",\n    \"module\": \"NodeNext\",\n    \"moduleResolution\": \"NodeNext\",\n    \"strict\": true,\n    \"declaration\": true,\n    \"noUncheckedIndexedAccess\": true,\n    \"exactOptionalPropertyTypes\": true,\n    \"noImplicitOverride\": true,\n    \"outDir\": \"dist\"\n  },\n  \"include\": [\"src/**/*.ts\", \"tests/**/*.ts\"]\n}\n".to_string()
}

fn render_schemas(spec: &ApiSpec) -> String {
    let mut output = String::from("import * as z from \"zod\";\n\n");
    for (name, schema) in &spec.schemas {
        output.push_str(&format!(
            "export const {name}Schema = {};\n\n",
            zod_schema(schema, spec)
        ));
    }
    for operation in &spec.operations {
        if let Some(schema) = inline_success_schema(operation) {
            output.push_str(&format!(
                "export const {}Schema = {};\n\n",
                operation_response_name(operation),
                zod_schema(schema, spec)
            ));
        }
    }
    output
}

fn render_types(spec: &ApiSpec) -> String {
    let mut output = String::from("import type * as z from \"zod\";\nimport { ");
    output.push_str(
        &spec
            .schemas
            .keys()
            .map(|name| format!("{name}Schema"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    output.push_str(" } from \"./schemas.js\";\n\n");
    for name in spec.schemas.keys() {
        output.push_str(&format!(
            "export type {name} = z.infer<typeof {name}Schema>;\n"
        ));
    }
    for operation in &spec.operations {
        if inline_success_schema(operation).is_some() {
            let name = operation_response_name(operation);
            output.push_str(&format!(
                "export type {name} = z.infer<typeof {name}Schema>;\n"
            ));
        }
    }
    output
}

fn render_errors() -> String {
    "export class SdkValidationError extends Error {\n  readonly operation: string;\n  readonly model: string;\n\n  constructor(operation: string, model: string) {\n    super(`Response validation failed for ${operation} (${model})`);\n    this.name = \"SdkValidationError\";\n    this.operation = operation;\n    this.model = model;\n  }\n}\n".to_string()
}

fn render_native(spec: &ApiSpec) -> String {
    let methods = spec
        .operations
        .iter()
        .map(|operation| {
            format!(
                "  {}({}): Promise<NativeValue>;\n",
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
        "export type NativeValue = null | boolean | number | string | NativeValue[] | {{ [key: string]: NativeValue }};\n\nexport interface NativeClient {{\n{methods}}}\n\nexport function loadNative(): NativeClient {{\n  throw new Error(\"The generated napi-rs native package is not installed\");\n}}\n"
    )
}

fn render_index(spec: &ApiSpec) -> String {
    format!(
        "{}{}}}\n",
        render_index_header(spec),
        render_index_methods(spec)
    )
}

fn render_index_header(spec: &ApiSpec) -> String {
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
    let mut type_names = spec.schemas.keys().cloned().collect::<Vec<_>>();
    type_names.extend(response_names);
    let types = type_names.join(", ");
    format!(
        "import * as z from \"zod\";\nimport {{ loadNative }} from \"./native.js\";\nimport type {{ {types} }} from \"./types.js\";\n{imports}\nexport * from \"./schemas.js\";\nexport * from \"./types.js\";\nexport * from \"./errors.js\";\n\nexport class Client {{\n  private readonly native = loadNative();\n\n"
    )
}

fn render_index_methods(spec: &ApiSpec) -> String {
    spec.operations
        .iter()
        .map(|operation| render_operation(operation, spec))
        .collect()
}

fn render_operation(operation: &Operation, _spec: &ApiSpec) -> String {
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
    let Some(response) = response else {
        return render_void_operation(operation, &parameters);
    };
    let model = schema_model_name(response).unwrap_or_else(|| operation_response_name(operation));
    let parser = format!("{model}Schema");
    format!(
        "  async {}({parameters}): Promise<{model}> {{\n    const value = await this.native.{}({});\n    return {parser}.parse(value);\n  }}\n\n",
        ts_identifier(&operation.operation_id),
        ts_identifier(&operation.operation_id),
        operation
            .parameters
            .iter()
            .filter(|parameter| parameter.location == super::ParameterLocation::Path)
            .map(|parameter| ts_identifier(&parameter.name))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn render_void_operation(operation: &Operation, parameters: &str) -> String {
    format!(
        "  async {}({parameters}): Promise<void> {{\n    await this.native.{}({});\n  }}\n\n",
        ts_identifier(&operation.operation_id),
        ts_identifier(&operation.operation_id),
        operation
            .parameters
            .iter()
            .filter(|parameter| parameter.location == super::ParameterLocation::Path)
            .map(|parameter| ts_identifier(&parameter.name))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn render_validation_test(spec: &ApiSpec) -> String {
    let Some(name) = spec.schemas.keys().next() else {
        return "import { describe, it } from \"vitest\";\n\ndescribe(\"generated schemas\", () => { it(\"has no models\", () => {}); });\n".to_string();
    };
    format!(
        "import {{ describe, expect, it }} from \"vitest\";\nimport {{ {name}Schema }} from \"../src/schemas.js\";\n\ndescribe(\"generated schemas\", () => {{\n  it(\"rejects invalid {name}\", () => {{\n    expect(() => {name}Schema.parse({{}})).toThrow();\n  }});\n}});\n"
    )
}

fn render_readme(spec: &ApiSpec) -> String {
    format!(
        "# {} TypeScript SDK\n\nInstall dependencies, then run `npm test`. Runtime response validation is generated with Zod. The native napi-rs package is intentionally a separate release artifact.\n",
        spec.title
    )
}

fn zod_schema(schema: &Schema, spec: &ApiSpec) -> String {
    match schema {
        Schema::String { .. } => "z.string()".to_string(),
        Schema::Integer { .. } => "z.number().int()".to_string(),
        Schema::Number { .. } => "z.number()".to_string(),
        Schema::Boolean => "z.boolean()".to_string(),
        Schema::Null => "z.null()".to_string(),
        Schema::Array(item) => format!("z.array({})", zod_schema(item, spec)),
        Schema::Object { .. } => zod_object_schema(schema, spec),
        Schema::Enum(values) => format!(
            "z.enum([{}])",
            values
                .iter()
                .map(|value| format!("{:?}", value))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Schema::Reference(name) => format!("{name}Schema"),
        Schema::Nullable(inner) => format!("{}.nullable()", zod_schema(inner, spec)),
        Schema::OneOf(values) | Schema::AnyOf(values) => format!(
            "z.union([{}])",
            values
                .iter()
                .map(|value| zod_schema(value, spec))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Schema::AllOf(values) => values
            .iter()
            .map(|value| zod_schema(value, spec))
            .reduce(|left, right| format!("z.intersection({left}, {right})"))
            .unwrap_or_else(|| "z.never()".to_string()),
    }
}

fn zod_object_schema(schema: &Schema, spec: &ApiSpec) -> String {
    let Schema::Object {
        properties,
        required,
        additional_properties,
    } = schema
    else {
        return "z.never()".to_string();
    };
    let fields = properties
        .iter()
        .map(|(name, value)| {
            let optional = if required.contains(name) {
                ""
            } else {
                ".optional()"
            };
            format!(
                "  {}: {}{},",
                ts_property(name),
                zod_schema(value, spec),
                optional
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let object = format!("z.object({{\n{fields}\n}})");
    match additional_properties.as_deref() {
        Some(value) => format!("{object}.catchall({})", zod_schema(value, spec)),
        None => format!("{object}.strict()"),
    }
}

fn schema_model_name(schema: &Schema) -> Option<String> {
    match schema {
        Schema::Reference(name) => Some(name.clone()),
        _ => None,
    }
}

fn inline_success_schema(operation: &Operation) -> Option<&Schema> {
    operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .and_then(|response| response.schema.as_ref())
        .filter(|schema| schema_model_name(schema).is_none())
}

fn operation_response_name(operation: &Operation) -> String {
    format!("{}Response", type_identifier(&operation.operation_id))
}

fn type_identifier(value: &str) -> String {
    ts_identifier(value)
        .split(' ')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

fn ts_property(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        value.to_string()
    } else {
        format!("{:?}", value)
    }
}

fn ts_identifier(value: &str) -> String {
    let mut output = String::new();
    for part in value
        .split(['-', '_', ' ', '.'])
        .filter(|part| !part.is_empty())
    {
        append_identifier_part(&mut output, part);
    }
    if output.is_empty() {
        "value".to_string()
    } else {
        output
    }
}

fn append_identifier_part(output: &mut String, part: &str) {
    if output.is_empty() {
        output.push_str(part);
        return;
    }
    let mut chars = part.chars();
    if let Some(first) = chars.next() {
        output.push(first.to_ascii_uppercase());
    }
    output.push_str(chars.as_str());
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
