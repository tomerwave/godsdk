use super::super::rust_ast::{mock_request_body, mock_success_body};
use super::super::{ApiIr, Operation, Schema};
use super::identifiers::ts_identifier;
use super::ordered_parameters;

pub(super) fn render_validation_test(spec: &ApiIr) -> String {
    let Some(name) = spec.schemas.keys().next() else {
        return "import { describe, it } from \"vitest\";\n\ndescribe(\"generated schemas\", () => { it(\"has no models\", () => {}); });\n".to_string();
    };
    format!(
        "import {{ describe, expect, it }} from \"vitest\";\nimport {{ {name}Schema }} from \"../src/schemas.js\";\n\ndescribe(\"generated schemas\", () => {{\n  it(\"rejects invalid {name}\", () => {{\n    expect(() => {name}Schema.parse({{}})).toThrow();\n  }});\n}});\n"
    )
}

pub(super) fn render_client_test(spec: &ApiIr) -> String {
    let Some(operation) = spec.operations.first() else {
        return "import { describe, it } from \"vitest\";\n\ndescribe(\"generated client\", () => { it(\"has no operations\", () => {}); });\n".to_string();
    };
    let method = ts_identifier(&operation.operation_id);
    let success_json =
        String::from_utf8(mock_success_body(spec, operation)).unwrap_or_else(|_| "{}".to_string());
    let mut arguments = operation
        .request_body_details
        .is_some()
        .then(|| mock_typescript_request_body(spec, operation))
        .into_iter()
        .collect::<Vec<_>>();
    arguments.extend(
        ordered_parameters(operation)
            .into_iter()
            .map(test_parameter_argument),
    );
    let arguments = arguments.join(", ");
    format!(
        "import {{ createServer }} from \"node:http\";\nimport {{ afterAll, beforeAll, describe, expect, it }} from \"vitest\";\nimport {{ Client }} from \"../src/index.js\";\n\nconst server = createServer((_request, response) => {{\n  response.writeHead(200, {{ \"content-type\": \"application/json\" }});\n  response.end(JSON.stringify({success_json}));\n}});\nlet baseUrl = \"\";\n\nbeforeAll(async () => {{\n  await new Promise<void>((resolve) => server.listen(0, \"127.0.0.1\", resolve));\n  const address = server.address();\n  if (address === null || typeof address === \"string\") throw new Error(\"mock server did not bind\");\n  baseUrl = `http://127.0.0.1:${{address.port}}`;\n}});\n\nafterAll(() => server.close());\n\ndescribe(\"generated native client\", () => {{\n  it(\"calls the Rust-backed local mock API\", async () => {{\n    const response = await new Client(baseUrl).{method}({arguments});\n    expect(response).toEqual({success_json});\n  }});\n}});\n"
    )
}

fn mock_typescript_request_body(spec: &ApiIr, operation: &Operation) -> String {
    let Some(body) = operation.request_body_details.as_ref() else {
        return "undefined".to_string();
    };
    let value = serde_json::from_slice::<serde_json::Value>(&mock_request_body(spec, operation))
        .unwrap_or(serde_json::Value::Null);
    body.schema
        .as_ref()
        .map(|schema| mock_typescript_value(&value, schema, spec))
        .unwrap_or_else(|| "null".to_string())
}

fn mock_typescript_value(value: &serde_json::Value, schema: &Schema, spec: &ApiIr) -> String {
    match schema {
        Schema::Reference(name) => spec
            .schemas
            .get(name)
            .map(|schema| mock_typescript_value(value, schema, spec))
            .unwrap_or_else(|| value.to_string()),
        Schema::String {
            format: Some(format),
        } if format == "binary" => format!(
            "new Uint8Array([{}])",
            value
                .as_str()
                .unwrap_or_default()
                .as_bytes()
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Schema::Object { properties, .. } => format!(
            "{{{}}}",
            properties
                .iter()
                .map(|(name, schema)| {
                    let value = value
                        .as_object()
                        .and_then(|object| object.get(name))
                        .unwrap_or(&serde_json::Value::Null);
                    format!("{name:?}: {}", mock_typescript_value(value, schema, spec))
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Schema::Array(item) => format!(
            "[{}]",
            value
                .as_array()
                .map(|values| values
                    .iter()
                    .map(|value| mock_typescript_value(value, item, spec))
                    .collect::<Vec<_>>()
                    .join(", "))
                .unwrap_or_default()
        ),
        _ => value.to_string(),
    }
}

fn test_parameter_argument(parameter: &crate::Parameter) -> String {
    if !parameter.required {
        return "undefined".to_string();
    }
    match parameter.schema {
        Schema::Boolean => "true".to_string(),
        Schema::Integer { .. } | Schema::Number { .. } => "1".to_string(),
        _ => "\"example\"".to_string(),
    }
}
