use proc_macro2::TokenStream;
use quote::quote;
use syn::LitStr;

use crate::ApiIr;

mod builder;
mod client;
mod mock;
mod mock_sample;
mod models;
mod operations;
mod parameter_serialization;
mod request;
mod transport;

pub(crate) fn render_files(spec: &ApiIr) -> Vec<(String, String)> {
    let mut files = vec![
        rust_file("sdk/rust/src/lib.rs", render_lib()),
        rust_file("sdk/rust/src/client/mod.rs", client::render_mod()),
        rust_file("sdk/rust/src/client/auth.rs", client::render_auth()),
        rust_file("sdk/rust/src/client/builder.rs", builder::render()),
        rust_file("sdk/rust/src/client/error.rs", client::render_error()),
        rust_file("sdk/rust/src/client/retry.rs", client::render_retry()),
        rust_file(
            "sdk/rust/src/client/parameter_serialization.rs",
            parameter_serialization::render(),
        ),
        rust_file("sdk/rust/src/client/transport.rs", transport::render()),
        rust_file("sdk/rust/src/operations/mod.rs", operations::render(spec)),
    ];
    files.extend(models::render(spec));
    files
}

pub(crate) use mock::render as render_mock_test;
pub(crate) use mock_sample::request_body as mock_request_body;
pub(crate) use mock_sample::success_body as mock_success_body;

fn rust_file(path: &str, tokens: TokenStream) -> (String, String) {
    let file = syn::parse2::<syn::File>(tokens).unwrap_or_else(|error| {
        panic!("Rust generator emitted invalid syntax for {path}: {error}")
    });
    (path.to_string(), prettyplease::unparse(&file))
}

fn render_lib() -> TokenStream {
    quote! {
        mod client;
        mod models;
        mod operations;

        pub use client::{Client, ClientBuilder, RetryPolicy, SdkError};
        pub use models::*;
        pub use operations::*;
    }
}

fn literal(value: &str) -> LitStr {
    LitStr::new(value, proc_macro2::Span::call_site())
}

pub(super) fn snake_case(value: &str) -> String {
    rust_identifier(value)
}

pub(super) fn rust_type_name(value: &str) -> String {
    let mut result: String = value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect();
    if result
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        result.insert(0, '_');
    }
    if result.is_empty() {
        result.push_str("GeneratedType");
    }
    result
}

pub(crate) fn rust_identifier(value: &str) -> String {
    let mut result = normalize_identifier(value);
    if result
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        result.insert(0, '_');
    }
    if is_rust_keyword(&result) {
        result.insert_str(0, "r#");
    }
    result
}

fn normalize_identifier(value: &str) -> String {
    value
        .chars()
        .enumerate()
        .fold(String::new(), |mut result, (index, character)| {
            if character.is_ascii_uppercase() && index > 0 {
                result.push('_');
            }
            result.push(if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            });
            result
        })
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn is_rust_keyword(value: &str) -> bool {
    RUST_KEYWORDS.contains(&value)
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
];
