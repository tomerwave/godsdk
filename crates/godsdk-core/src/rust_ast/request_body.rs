use proc_macro2::TokenStream;
use quote::quote;

use super::literal;

pub(super) fn request_body_expression(
    content_type: &str,
    binary_fields: &[syn::LitStr],
) -> TokenStream {
    let content_type_literal = literal(content_type);
    if content_type == "multipart/form-data" {
        quote! { RequestBody::Multipart { bytes, binary_fields: &[#(#binary_fields),*] } }
    } else {
        quote! { RequestBody::Bytes { content_type: #content_type_literal, bytes } }
    }
}

pub(super) fn required_body_bytes(content_type: &str, name: &syn::Ident) -> TokenStream {
    if content_type == "application/octet-stream" {
        quote! { #name }
    } else if content_type == "application/x-www-form-urlencoded" {
        quote! { serde_urlencoded::to_string(#name).map_err(|error| SdkError::Serialization(error.to_string()))?.into_bytes() }
    } else {
        quote! { serde_json::to_vec(&#name).map_err(|error| SdkError::Serialization(error.to_string()))? }
    }
}

pub(super) fn optional_body_bytes(content_type: &str) -> TokenStream {
    if content_type == "application/octet-stream" {
        quote! { value }
    } else if content_type == "application/x-www-form-urlencoded" {
        quote! { serde_urlencoded::to_string(value).map_err(|error| SdkError::Serialization(error.to_string()))?.into_bytes() }
    } else {
        quote! { serde_json::to_vec(&value).map_err(|error| SdkError::Serialization(error.to_string()))? }
    }
}
