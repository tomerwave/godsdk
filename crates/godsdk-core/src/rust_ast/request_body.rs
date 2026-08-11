use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::literal;
use crate::RequestBody;

pub(super) fn form_request_body_argument(
    request_body: &RequestBody,
    body_type: TokenStream,
    arguments: &mut Vec<TokenStream>,
) -> TokenStream {
    let name = format_ident!("request_body");
    arguments.push(if request_body.required {
        quote! { #name: #body_type }
    } else {
        quote! { #name: Option<#body_type> }
    });
    if request_body.required {
        quote! { crate::client::form_request_body(#name) }
    } else {
        quote! { crate::client::optional_form_request_body(#name) }
    }
}

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
        quote! { crate::client::serialize_form_body(#name)? }
    } else {
        quote! { serde_json::to_vec(&#name).map_err(|error| SdkError::Serialization(error.to_string()))? }
    }
}

pub(super) fn optional_body_bytes(content_type: &str) -> TokenStream {
    if content_type == "application/octet-stream" {
        quote! { value }
    } else if content_type == "application/x-www-form-urlencoded" {
        quote! { crate::client::serialize_form_body(value)? }
    } else {
        quote! { serde_json::to_vec(&value).map_err(|error| SdkError::Serialization(error.to_string()))? }
    }
}
