use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{LitByteStr, LitStr};

use crate::{ApiIr, ParameterLocation, SecuritySchemeKind};

use super::rust_identifier;

pub(crate) fn render(spec: &ApiIr) -> String {
    let operation = &spec.operations[0];
    let package = format_ident!("{}", format!("{}_sdk", slug(&spec.title).replace('-', "_")));
    let method = format_ident!("{}", rust_identifier(&operation.operation_id));
    let arguments = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|_| quote! { "pet-1" })
        .collect::<Vec<_>>();
    let literals = MockLiterals {
        method_path: LitStr::new(&render_method_path(&operation.path), Span::call_site()),
        success: response_bytes(b"{\"id\":\"pet-1\",\"name\":\"Fluffy\"}"),
    };
    let auth = mock_auth(spec, operation);
    let context = MockContext {
        package: &package,
        method: &method,
        arguments: &arguments,
        auth: &auth.builder,
    };
    let imports = main_imports();
    let main = main_test(&context, &literals);
    let retry = retry_module(&context, &literals.success, &auth.assertion);
    let tokens = quote! {
        #imports
        #main
        #retry
    };
    let file = syn::parse2::<syn::File>(tokens)
        .unwrap_or_else(|error| panic!("Rust generator emitted invalid mock test: {error}"));
    prettyplease::unparse(&file)
}

struct MockLiterals {
    method_path: LitStr,
    success: LitByteStr,
}

struct MockAuth {
    builder: proc_macro2::TokenStream,
    assertion: LitStr,
}

struct MockContext<'a> {
    package: &'a syn::Ident,
    method: &'a syn::Ident,
    arguments: &'a [proc_macro2::TokenStream],
    auth: &'a proc_macro2::TokenStream,
}

fn mock_auth(spec: &ApiIr, operation: &crate::Operation) -> MockAuth {
    let required = operation
        .security
        .as_ref()
        .or(spec.security.as_ref())
        .and_then(|requirements| requirements.first())
        .and_then(|requirement| requirement.schemes.first());
    let Some(required) = required else {
        return MockAuth {
            builder: quote! { .bearer_token("secret") },
            assertion: LitStr::new("authorization: bearer secret", Span::call_site()),
        };
    };
    let scheme_name = LitStr::new(&required.name, Span::call_site());
    let scheme = spec
        .security_schemes
        .get(&required.name)
        .unwrap_or_else(|| panic!("validated security scheme is present"));
    mock_scheme_auth(&scheme_name, &scheme.kind)
}

fn mock_scheme_auth(scheme_name: &LitStr, kind: &SecuritySchemeKind) -> MockAuth {
    match kind {
        SecuritySchemeKind::Http { scheme, .. } => mock_http_auth(scheme_name, scheme),
        SecuritySchemeKind::ApiKey { name, location } => {
            mock_api_key_auth(scheme_name, name, *location)
        }
        SecuritySchemeKind::OAuth2 { .. } => MockAuth {
            builder: quote! { .bearer_token_for(#scheme_name, "secret") },
            assertion: LitStr::new("authorization: bearer secret", Span::call_site()),
        },
    }
}

fn mock_http_auth(scheme_name: &LitStr, scheme: &str) -> MockAuth {
    if scheme.eq_ignore_ascii_case("bearer") {
        return MockAuth {
            builder: quote! { .bearer_token_for(#scheme_name, "secret") },
            assertion: LitStr::new("authorization: bearer secret", Span::call_site()),
        };
    }
    if scheme.eq_ignore_ascii_case("basic") {
        return MockAuth {
            builder: quote! { .basic_auth_for(#scheme_name, "user", Some("secret".to_string())) },
            assertion: LitStr::new("authorization: basic", Span::call_site()),
        };
    }
    MockAuth {
        builder: quote! { .http_auth_for(#scheme_name, "secret") },
        assertion: LitStr::new(
            &format!("authorization: {scheme} secret").to_ascii_lowercase(),
            Span::call_site(),
        ),
    }
}

fn mock_api_key_auth(scheme_name: &LitStr, name: &str, location: ParameterLocation) -> MockAuth {
    let key_name = LitStr::new(name, Span::call_site());
    match location {
        ParameterLocation::Header => MockAuth {
            builder: quote! { .api_key_header_for(#scheme_name, #key_name, "secret") },
            assertion: LitStr::new(
                &format!("{name}: secret").to_ascii_lowercase(),
                Span::call_site(),
            ),
        },
        ParameterLocation::Query => MockAuth {
            builder: quote! { .api_key_query_for(#scheme_name, #key_name, "secret") },
            assertion: LitStr::new(
                &format!("{name}=secret").to_ascii_lowercase(),
                Span::call_site(),
            ),
        },
        ParameterLocation::Cookie => MockAuth {
            builder: quote! { .api_key_cookie_for(#scheme_name, #key_name, "secret") },
            assertion: LitStr::new(
                &format!("cookie: {name}=secret").to_ascii_lowercase(),
                Span::call_site(),
            ),
        },
        ParameterLocation::Path => panic!("validated API key cannot use a path location"),
    }
}

fn main_imports() -> proc_macro2::TokenStream {
    quote! {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;
    }
}

fn main_test(context: &MockContext<'_>, literals: &MockLiterals) -> proc_macro2::TokenStream {
    let server = main_server(&literals.method_path, &literals.success, context.auth);
    let call = main_call(context.method, context.arguments);
    let package = context.package;
    quote! {
        use #package::Client;

        #[tokio::test]
        async fn calls_a_real_local_mock_server() {
            #server
            #call
            server.join().unwrap_or_else(|_| panic!("mock server joins"));
        }
    }
}

fn main_server(
    method_path: &LitStr,
    success: &LitByteStr,
    auth: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    quote! {
        let listener = TcpListener::bind("127.0.0.1:0")
            .unwrap_or_else(|error| panic!("bind mock server: {error}"));
        let address = listener
            .local_addr()
            .unwrap_or_else(|error| panic!("mock address: {error}"));
        let server = thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .unwrap_or_else(|error| panic!("accept request: {error}"));
            let mut request = [0_u8; 4096];
            let size = stream
                .read(&mut request)
                .unwrap_or_else(|error| panic!("read request: {error}"));
            let request = String::from_utf8_lossy(&request[..size]);
            assert!(request.contains(#method_path));
            stream
                .write_all(#success)
                .unwrap_or_else(|error| panic!("write response: {error}"));
        });
        let client = Client::builder(format!("http://{address}"))
            #auth
            .build()
            .unwrap_or_else(|error| panic!("client: {error}"));
    }
}

fn main_call(
    method: &syn::Ident,
    arguments: &[proc_macro2::TokenStream],
) -> proc_macro2::TokenStream {
    quote! {
        let response = client.#method(#(#arguments),*)
            .await
            .unwrap_or_else(|error| panic!("client request: {error}"));
        assert!(format!("{response:?}").contains("Fluffy"));
    }
}

fn retry_module(
    context: &MockContext<'_>,
    success: &LitByteStr,
    assertion: &LitStr,
) -> proc_macro2::TokenStream {
    let server = retry_server(success, assertion);
    let call = retry_call(context);
    let package = context.package;
    quote! {
        mod retry_tests {
            use std::io::{Read, Write};
            use std::net::TcpListener;
            use std::thread;
            use std::time::Duration;
            use #package::{Client, RetryPolicy};

            #server

            #[tokio::test]
            async fn retries_a_transient_response() {
                #call
                server.join().unwrap_or_else(|_| panic!("retry server joins"));
            }
        }
    }
}

fn retry_server(success: &LitByteStr, assertion: &LitStr) -> proc_macro2::TokenStream {
    quote! {
        fn spawn_retry_server() -> (String, std::thread::JoinHandle<()>) {
            let listener = TcpListener::bind("127.0.0.1:0")
                .unwrap_or_else(|error| panic!("bind retry server: {error}"));
            let address = listener
                .local_addr()
                .unwrap_or_else(|error| panic!("retry server address: {error}"));
            let server = thread::spawn(move || {
                for attempt in 0..2 {
                    let (mut stream, _) = listener
                        .accept()
                        .unwrap_or_else(|error| panic!("accept retry request: {error}"));
                    let mut request = [0_u8; 4096];
                    let size = stream
                        .read(&mut request)
                        .unwrap_or_else(|error| panic!("read retry request: {error}"));
                    let request = String::from_utf8_lossy(&request[..size]);
                    assert!(request.to_ascii_lowercase().contains(#assertion));
                    if attempt == 0 {
                        stream
                            .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 7\r\nConnection: close\r\n\r\nretry me")
                            .unwrap_or_else(|error| panic!("write retry response: {error}"));
                    } else {
                        stream
                            .write_all(#success)
                            .unwrap_or_else(|error| panic!("write success response: {error}"));
                    }
                }
            });
            (format!("http://{address}"), server)
        }
    }
}

fn retry_call(context: &MockContext<'_>) -> proc_macro2::TokenStream {
    let method = context.method;
    let arguments = context.arguments;
    let auth = context.auth;
    quote! {
        let policy = RetryPolicy {
            max_retries: 1,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            retry_statuses: vec![503],
            retry_non_idempotent: false,
        };
        let (base_url, server) = spawn_retry_server();
        let client = Client::builder(base_url)
            #auth
            .retry_policy(policy)
            .build()
            .unwrap_or_else(|error| panic!("client: {error}"));
        let response = client.#method(#(#arguments),*)
            .await
            .unwrap_or_else(|error| panic!("retry request: {error}"));
        assert!(format!("{response:?}").contains("Fluffy"));
    }
}

fn response_bytes(body: &[u8]) -> LitByteStr {
    let mut response =
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ".to_vec();
    response.extend_from_slice(body.len().to_string().as_bytes());
    response.extend_from_slice(b"\r\nConnection: close\r\n\r\n");
    response.extend_from_slice(body);
    LitByteStr::new(&response, Span::call_site())
}

fn render_method_path(path: &str) -> String {
    path.split('{')
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
        .collect()
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
