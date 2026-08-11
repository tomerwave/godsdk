use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::super::rust_type_name;
use crate::{ApiIr, Schema};

pub(super) fn parameter_type(
    schema: &Schema,
    spec: &ApiIr,
    inline: Option<&syn::Ident>,
) -> TokenStream {
    match schema {
        Schema::String {
            format: Some(format),
        } if format == "binary" => quote! { Vec<u8> },
        Schema::String { .. } => quote! { String },
        Schema::Integer { .. } => quote! { i64 },
        Schema::Number { .. } => quote! { f64 },
        Schema::Boolean => quote! { bool },
        Schema::TypedEnum { base, .. } => inline.map_or_else(
            || parameter_type(base, spec, None),
            |ident| quote! { #ident },
        ),
        Schema::Reference(name) if spec.schemas.contains_key(name) => {
            let name = format_ident!("{}", rust_type_name(name));
            quote! { #name }
        }
        Schema::Array(item) => {
            let item = parameter_type(item, spec, None);
            quote! { Vec<#item> }
        }
        _ => quote! { serde_json::Value },
    }
}
