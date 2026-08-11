use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse2;

use super::operations_render::render_native_operation;
use super::{has_error_responses, type_identifier};
use crate::ApiIr;

use super::identifiers::slug;

pub(super) fn render_native_cargo(spec: &ApiIr) -> String {
    let crate_name = rust_crate_name(spec);
    let package = slug(&spec.title);
    [
        "[package]".to_string(),
        ["name = \"", &package, "-typescript-native\""].concat(),
        "version = \"0.1.0\"".to_string(),
        "edition = \"2024\"".to_string(),
        "rust-version = \"1.97\"".to_string(),
        String::new(),
        "[lib]".to_string(),
        "crate-type = [\"cdylib\"]".to_string(),
        String::new(),
        "[dependencies]".to_string(),
        "napi = { version = \"3.12\", features = [\"napi9\", \"tokio_rt\", \"serde-json\"] }"
            .to_string(),
        "napi-derive = \"3.6\"".to_string(),
        "serde_json = \"1\"".to_string(),
        [
            crate_name,
            " = { package = \"".to_string(),
            package,
            "-sdk\", path = \"../../rust\" }".to_string(),
        ]
        .concat(),
    ]
    .join("\n")
}

pub(super) fn render_native_package() -> String {
    "{\n  \"type\": \"commonjs\"\n}\n".to_string()
}

pub(super) fn render_native_rust(spec: &ApiIr) -> String {
    let crate_name = rust_crate_name(spec);
    let methods: Vec<TokenStream> = spec
        .operations
        .iter()
        .map(|operation| render_native_operation(operation, &crate_name))
        .collect();
    render_rust(native_rust_file(spec, &crate_name, &methods))
}

fn native_rust_file(spec: &ApiIr, crate_name: &str, methods: &[TokenStream]) -> TokenStream {
    let crate_ident = format_ident!("{crate_name}");
    let mut imports = vec![quote! { Client as RustClient }];
    if spec
        .operations
        .iter()
        .any(|operation| !has_error_responses(operation))
    {
        imports.push(quote! { SdkError });
    }
    for operation in spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
    {
        let error = format_ident!("{}Error", type_identifier(&operation.operation_id));
        imports.push(quote! { #error });
    }
    quote! {
        use napi::bindgen_prelude::*;
        use napi_derive::napi;
        use #crate_ident::{#(#imports),*};

        #[napi]
        pub struct NativeClient {
            inner: RustClient,
        }

        #[napi]
        impl NativeClient {
            #[napi(constructor)]
            pub fn new(base_url: String) -> Result<Self> {
                let inner = RustClient::builder(base_url).build().map_err(to_napi_error)?;
                Ok(Self { inner })
            }
            #(#methods)*
        }

        fn to_napi_error(error: impl std::fmt::Display) -> Error {
            Error::from_reason(error.to_string())
        }
    }
}

fn render_rust(tokens: TokenStream) -> String {
    let file = parse2::<syn::File>(tokens).unwrap_or_else(|error| {
        panic!("TypeScript native generator emitted invalid Rust: {error}")
    });
    prettyplease::unparse(&file)
}

fn rust_crate_name(spec: &ApiIr) -> String {
    slug(&spec.title).replace('-', "_")
}
