use proc_macro2::TokenStream;
use quote::quote;
use syn::LitStr;

use crate::ApiIr;

mod builder;
mod client;
mod mock;
mod models;
mod operations;
mod transport;

pub(crate) fn render_files(spec: &ApiIr) -> Vec<(String, String)> {
    let mut files = vec![
        rust_file("sdk/rust/src/lib.rs", render_lib()),
        rust_file("sdk/rust/src/client/mod.rs", client::render_mod()),
        rust_file("sdk/rust/src/client/auth.rs", client::render_auth()),
        rust_file("sdk/rust/src/client/builder.rs", builder::render()),
        rust_file("sdk/rust/src/client/error.rs", client::render_error()),
        rust_file("sdk/rust/src/client/retry.rs", client::render_retry()),
        rust_file("sdk/rust/src/client/transport.rs", transport::render()),
        rust_file("sdk/rust/src/operations/mod.rs", operations::render(spec)),
    ];
    files.extend(models::render(spec));
    files
}

pub(crate) use mock::render as render_mock_test;

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
    }
}

fn literal(value: &str) -> LitStr {
    LitStr::new(value, proc_macro2::Span::call_site())
}

pub(super) fn snake_case(value: &str) -> String {
    rust_identifier(value)
}

pub(super) fn rust_type_name(value: &str) -> String {
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

pub(crate) fn rust_identifier(value: &str) -> String {
    let mut result = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            result.push('_');
        }
        result.push(if character.is_ascii_alphanumeric() {
            character.to_ascii_lowercase()
        } else {
            '_'
        });
    }
    if result
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        result.insert(0, '_');
    }
    result
}
