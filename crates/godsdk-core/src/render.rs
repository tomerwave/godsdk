use super::{ApiSpec, ParameterLocation};

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
    format!(
        "{}\nmod retry_tests {{\n{}\n}}",
        render_async_mock_test_legacy(package, method, arguments, method_path),
        render_async_retry_test(package, method, arguments)
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
