use std::collections::{BTreeMap, BTreeSet};

use super::{ApiSpec, Operation, ParameterLocation, Schema};

pub(crate) fn http_method_name(method: super::HttpMethod) -> &'static str {
    match method {
        super::HttpMethod::Delete => "DELETE",
        super::HttpMethod::Get => "GET",
        super::HttpMethod::Head => "HEAD",
        super::HttpMethod::Options => "OPTIONS",
        super::HttpMethod::Patch => "PATCH",
        super::HttpMethod::Post => "POST",
        super::HttpMethod::Put => "PUT",
        super::HttpMethod::Trace => "TRACE",
    }
}

#[allow(dead_code)]
fn render_rust_mock_test_legacy(spec: &ApiSpec) -> String {
    let operation = &spec.operations[0];
    let package = format!("{}_sdk", slug(&spec.title).replace('-', "_"));
    let method = rust_identifier(&operation.operation_id);
    let arguments = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|_| "\"pet-1\"")
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "use std::io::{{Read, Write}};\nuse std::net::TcpListener;\nuse std::thread;\nuse {package}::Client;\n\n#[test]\nfn calls_a_real_local_mock_server() {{\n    let listener = TcpListener::bind(\"127.0.0.1:0\").unwrap_or_else(|error| panic!(\"bind mock server: {{error}}\"));\n    let address = listener.local_addr().unwrap_or_else(|error| panic!(\"mock address: {{error}}\"));\n    let server = thread::spawn(move || {{\n        let (mut stream, _) = listener.accept().unwrap_or_else(|error| panic!(\"accept request: {{error}}\"));\n        let mut request = [0_u8; 1024];\n        let size = stream.read(&mut request).unwrap_or_else(|error| panic!(\"read request: {{error}}\"));\n        let request = String::from_utf8_lossy(&request[..size]);\n        assert!(request.contains(\"{method_path}\"));\n        stream.write_all(b\"HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: 17\\r\\nConnection: close\\r\\n\\r\\n{{\\\"name\\\":\\\"Fluffy\\\"}}\").unwrap_or_else(|error| panic!(\"write response: {{error}}\"));\n    }});\n    let client = Client::new(format!(\"http://{{address}}\"));\n    let response = client.{method}({arguments}).unwrap_or_else(|error| panic!(\"client request: {{error:?}}\"));\n    assert!(response.contains(\"Fluffy\"));\n    server.join().unwrap_or_else(|_| panic!(\"mock server joins\"));\n}}\n",
        method_path = operation
            .path
            .split('{')
            .enumerate()
            .map(|(index, segment)| {
                if index == 0 {
                    segment.to_string()
                } else {
                    segment
                        .split_once('}')
                        .map_or_else(|| segment.to_string(), |parts| format!("pet-1{}", parts.1))
                }
            })
            .collect::<String>(),
    )
}

pub(crate) fn render_rust_mock_test(spec: &ApiSpec) -> String {
    let operation = &spec.operations[0];
    let package = format!("{}_sdk", slug(&spec.title).replace('-', "_"));
    let method = rust_identifier(&operation.operation_id);
    let arguments = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|_| "\"pet-1\"")
        .collect::<Vec<_>>()
        .join(", ");
    let method_path = operation
        .path
        .split('{')
        .enumerate()
        .map(|(index, segment)| {
            if index == 0 {
                segment.to_string()
            } else {
                segment
                    .split_once('}')
                    .map_or_else(|| segment.to_string(), |parts| format!("pet-1{}", parts.1))
            }
        })
        .collect::<String>();
    render_async_mock_test(&package, &method, &arguments, &method_path)
}

pub(crate) fn render_rust_models(spec: &ApiSpec) -> String {
    let mut output = String::from("use serde::{Deserialize, Serialize};\n\n");
    for (name, schema) in &spec.schemas {
        output.push_str(&render_rust_model(name, schema, spec));
        output.push('\n');
    }
    output
}

pub(crate) fn rust_response_type(operation: &Operation) -> String {
    operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2') && response.schema.is_some())
        .and_then(|response| response.schema.as_ref())
        .map(rust_schema_type)
        .unwrap_or_else(|| "String".to_string())
}

fn render_rust_model(name: &str, schema: &Schema, spec: &ApiSpec) -> String {
    match schema {
        Schema::Enum(values) => render_rust_enum(name, values),
        Schema::OneOf(variants) | Schema::AnyOf(variants) => render_rust_union(name, variants),
        Schema::Object { .. } | Schema::AllOf(_) => render_rust_object(name, schema, spec),
        Schema::Reference(reference) => {
            format!("pub type {name} = {};\n", rust_type_name(reference))
        }
        other => format!("pub type {name} = {};\n", rust_schema_type(other)),
    }
}

fn render_rust_enum(name: &str, values: &[String]) -> String {
    let variants = values
        .iter()
        .map(|value| {
            format!(
                "    #[serde(rename = {:?})]\n    {},\n",
                value,
                rust_type_name(value)
            )
        })
        .collect::<String>();
    format!(
        "#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]\npub enum {name} {{\n{variants}}}\n"
    )
}

fn render_rust_union(name: &str, variants: &[Schema]) -> String {
    let variants = variants
        .iter()
        .filter_map(|variant| match variant {
            Schema::Reference(reference) => {
                Some(format!("    {}({reference}),\n", rust_type_name(reference)))
            }
            _ => None,
        })
        .collect::<String>();
    format!(
        "#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]\n#[serde(untagged)]\npub enum {name} {{\n{variants}}}\n"
    )
}

fn render_rust_object(name: &str, schema: &Schema, _spec: &ApiSpec) -> String {
    let (properties, required, additional) = object_shape(schema, _spec);
    let fields = properties
        .iter()
        .map(|(property, property_schema)| {
            render_rust_field(property, property_schema, &required, _spec)
        })
        .collect::<String>();
    let extra = additional.map_or_else(String::new, |schema| {
        format!(
            "    #[serde(flatten)]\n    pub additional_properties: std::collections::BTreeMap<String, {}>,\n",
            rust_schema_type(&schema)
        )
    });
    format!(
        "#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]\npub struct {name} {{\n{fields}{extra}}}\n"
    )
}

fn render_rust_field(
    property: &str,
    schema: &Schema,
    required: &BTreeSet<String>,
    _spec: &ApiSpec,
) -> String {
    let type_name = rust_schema_type(schema);
    let type_name = if required.contains(property) {
        type_name
    } else {
        format!("Option<{type_name}>")
    };
    format!(
        "    #[serde(rename = {:?})]\n    pub {}: {},\n",
        property,
        rust_identifier(property),
        type_name
    )
}

fn object_shape(
    schema: &Schema,
    spec: &ApiSpec,
) -> (BTreeMap<String, Schema>, BTreeSet<String>, Option<Schema>) {
    match schema {
        Schema::Object {
            properties,
            required,
            additional_properties,
        } => (
            properties.clone(),
            required.clone(),
            additional_properties.as_deref().cloned(),
        ),
        Schema::AllOf(parts) => {
            let mut properties = BTreeMap::new();
            let mut required = BTreeSet::new();
            let mut additional = None;
            for part in parts {
                let (part_properties, part_required, part_additional) = object_shape(part, spec);
                properties.extend(part_properties);
                required.extend(part_required);
                additional = additional.or(part_additional);
            }
            (properties, required, additional)
        }
        Schema::Reference(name) => referenced_object_shape(name, spec),
        _ => (BTreeMap::new(), BTreeSet::new(), None),
    }
}

fn referenced_object_shape(
    name: &str,
    spec: &ApiSpec,
) -> (BTreeMap<String, Schema>, BTreeSet<String>, Option<Schema>) {
    spec.schemas
        .get(name)
        .map_or_else(empty_object_shape, |schema| object_shape(schema, spec))
}

fn empty_object_shape() -> (BTreeMap<String, Schema>, BTreeSet<String>, Option<Schema>) {
    (BTreeMap::new(), BTreeSet::new(), None)
}

fn rust_schema_type(schema: &Schema) -> String {
    match schema {
        Schema::String { .. } => "String".to_string(),
        Schema::Integer { .. } => "i64".to_string(),
        Schema::Number { .. } => "f64".to_string(),
        Schema::Boolean => "bool".to_string(),
        Schema::Null => "()".to_string(),
        Schema::Array(item) => format!("Vec<{}>", rust_schema_type(item)),
        Schema::Object {
            additional_properties: Some(value),
            properties,
            ..
        } if properties.is_empty() => {
            format!("BTreeMap<String, {}>", rust_schema_type(value))
        }
        Schema::Object { .. } => "serde_json::Map<String, serde_json::Value>".to_string(),
        Schema::Enum(_) | Schema::OneOf(_) | Schema::AnyOf(_) | Schema::AllOf(_) => {
            "serde_json::Value".to_string()
        }
        Schema::Reference(name) => rust_type_name(name),
        Schema::Nullable(inner) => format!("Option<{}>", rust_schema_type(inner)),
    }
}

fn rust_type_name(value: &str) -> String {
    value
        .split(['/', '#', '-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect()
}

#[allow(dead_code)]
fn render_async_mock_test_legacy(
    package: &str,
    method: &str,
    arguments: &str,
    method_path: &str,
) -> String {
    format!(
        "use std::io::{{Read, Write}};\nuse std::net::TcpListener;\nuse std::thread;\nuse {package}::Client;\n\n#[tokio::test]\nasync fn calls_a_real_local_mock_server() {{\n    let listener = TcpListener::bind(\"127.0.0.1:0\").unwrap_or_else(|error| panic!(\"bind mock server: {{error}}\"));\n    let address = listener.local_addr().unwrap_or_else(|error| panic!(\"mock address: {{error}}\"));\n    let server = thread::spawn(move || {{\n        let (mut stream, _) = listener.accept().unwrap_or_else(|error| panic!(\"accept request: {{error}}\"));\n        let mut request = [0_u8; 4096];\n        let size = stream.read(&mut request).unwrap_or_else(|error| panic!(\"read request: {{error}}\"));\n        let request = String::from_utf8_lossy(&request[..size]);\n        assert!(request.contains(\"{method_path}\"));\n        stream.write_all(b\"HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: 17\\r\\nConnection: close\\r\\n\\r\\n{{\\\"name\\\":\\\"Fluffy\\\"}}\").unwrap_or_else(|error| panic!(\"write response: {{error}}\"));\n    }});\n    let client = Client::new(format!(\"http://{{address}}\")).unwrap_or_else(|error| panic!(\"client: {{error}}\"));\n    let response = client.{method}({arguments}).await.unwrap_or_else(|error| panic!(\"client request: {{error}}\"));\n    assert!(response.contains(\"Fluffy\"));\n    server.join().unwrap_or_else(|_| panic!(\"mock server joins\"));\n}}\n",
        package = package,
        method = method,
        arguments = arguments,
        method_path = method_path,
    )
}

fn render_async_mock_test(
    package: &str,
    method: &str,
    arguments: &str,
    method_path: &str,
) -> String {
    let rendered = format!(
        "{}\nmod retry_tests {{\n{}\n}}",
        render_async_mock_test_legacy(package, method, arguments, method_path),
        render_async_retry_test(package, method, arguments)
    );
    replace_string_assertions(rendered)
}

fn replace_string_assertions(rendered: String) -> String {
    rendered
        .replace(
            "assert!(response.contains(\"Fluffy\"));",
            "assert!(format!(\"{response:?}\").contains(\"Fluffy\"));",
        )
        .replace("Content-Length: 17", "Content-Length: 30")
        .replace(
            "{\\\"name\\\":\\\"Fluffy\\\"}",
            "{\\\"id\\\":\\\"pet-1\\\",\\\"name\\\":\\\"Fluffy\\\"}",
        )
}

#[allow(dead_code)]
fn render_async_retry_test_legacy(package: &str, method: &str, arguments: &str) -> String {
    format!(
        "use std::io::{{Read, Write}};\nuse std::net::TcpListener;\nuse std::thread;\nuse std::time::Duration;\nuse {package}::{{Client, RetryPolicy}};\n\n#[tokio::test]\nasync fn retries_a_transient_response() {{\n    let listener = TcpListener::bind(\"127.0.0.1:0\").unwrap_or_else(|error| panic!(\"bind retry server: {{error}}\"));\n    let address = listener.local_addr().unwrap_or_else(|error| panic!(\"retry server address: {{error}}\"));\n    let server = thread::spawn(move || {{\n        for attempt in 0..2 {{\n            let (mut stream, _) = listener.accept().unwrap_or_else(|error| panic!(\"accept retry request: {{error}}\"));\n            let mut request = [0_u8; 4096];\n            let size = stream.read(&mut request).unwrap_or_else(|error| panic!(\"read retry request: {{error}}\"));\n            let request = String::from_utf8_lossy(&request[..size]);\n            assert!(request.contains(\"Authorization: Bearer secret\"));\n            if attempt == 0 {{\n                stream.write_all(b\"HTTP/1.1 503 Service Unavailable\\r\\nContent-Length: 7\\r\\nConnection: close\\r\\n\\r\\nretry me\").unwrap_or_else(|error| panic!(\"write retry response: {{error}}\"));\n            }} else {{\n                stream.write_all(b\"HTTP/1.1 200 OK\\r\\nContent-Length: 17\\r\\nConnection: close\\r\\n\\r\\n{{\\\"name\\\":\\\"Fluffy\\\"}}\").unwrap_or_else(|error| panic!(\"write success response: {{error}}\"));\n            }}\n        }}\n    }});\n    let policy = RetryPolicy {{ max_retries: 1, initial_backoff: Duration::from_millis(1), max_backoff: Duration::from_millis(5), retry_statuses: vec![503], retry_non_idempotent: false }};\n    let client = Client::builder(format!(\"http://{{address}}\")).bearer_token(\"secret\").retry_policy(policy).build().unwrap_or_else(|error| panic!(\"client: {{error}}\"));\n    let response = client.{method}({arguments}).await.unwrap_or_else(|error| panic!(\"retry request: {{error}}\"));\n    assert!(response.contains(\"Fluffy\"));\n    server.join().unwrap_or_else(|_| panic!(\"retry server joins\"));\n}}\n",
        package = package,
        method = method,
        arguments = arguments,
    )
}

fn render_async_retry_test(package: &str, method: &str, arguments: &str) -> String {
    format!(
        "use std::io::{{Read, Write}};\nuse std::net::TcpListener;\nuse std::thread;\nuse std::time::Duration;\nuse {package}::{{Client, RetryPolicy}};\n\n#[tokio::test]\nasync fn retries_a_transient_response() {{\n    let listener = TcpListener::bind(\"127.0.0.1:0\").unwrap_or_else(|error| panic!(\"bind retry server: {{error}}\"));\n    let address = listener.local_addr().unwrap_or_else(|error| panic!(\"retry server address: {{error}}\"));\n    let server = thread::spawn(move || {{\n        for attempt in 0..2 {{\n            let (mut stream, _) = listener.accept().unwrap_or_else(|error| panic!(\"accept retry request: {{error}}\"));\n            let mut request = [0_u8; 4096];\n            let size = stream.read(&mut request).unwrap_or_else(|error| panic!(\"read retry request: {{error}}\"));\n            let request = String::from_utf8_lossy(&request[..size]);\n            assert!(request.to_ascii_lowercase().contains(\"authorization: bearer secret\"));\n            if attempt == 0 {{\n                stream.write_all(b\"HTTP/1.1 503 Service Unavailable\\r\\nContent-Length: 7\\r\\nConnection: close\\r\\n\\r\\nretry me\").unwrap_or_else(|error| panic!(\"write retry response: {{error}}\"));\n            }} else {{\n                stream.write_all(b\"HTTP/1.1 200 OK\\r\\nContent-Length: 17\\r\\nConnection: close\\r\\n\\r\\n{{\\\"name\\\":\\\"Fluffy\\\"}}\").unwrap_or_else(|error| panic!(\"write success response: {{error}}\"));\n            }}\n        }}\n    }});\n    let policy = RetryPolicy {{ max_retries: 1, initial_backoff: Duration::from_millis(1), max_backoff: Duration::from_millis(5), retry_statuses: vec![503], retry_non_idempotent: false }};\n    let client = Client::builder(format!(\"http://{{address}}\")).bearer_token(\"secret\").retry_policy(policy).build().unwrap_or_else(|error| panic!(\"client: {{error}}\"));\n    let response = client.{method}({arguments}).await.unwrap_or_else(|error| panic!(\"retry request: {{error}}\"));\n    assert!(response.contains(\"Fluffy\"));\n    server.join().unwrap_or_else(|_| panic!(\"retry server joins\"));\n}}\n",
        package = package,
        method = method,
        arguments = arguments,
    )
}

pub(crate) fn rust_identifier(value: &str) -> String {
    let mut identifier = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            identifier.push('_');
        }
        identifier.push(character.to_ascii_lowercase());
    }
    identifier
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
