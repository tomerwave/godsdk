use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{ApiIr, Operation, ParameterLocation, Schema};

use super::operations::{error_decoder_name, method_tokens, response_decode};
use super::{literal, rust_identifier, rust_type_name};

pub(super) struct RequestParts {
    pub(super) query: TokenStream,
    pub(super) headers: TokenStream,
    pub(super) body: TokenStream,
}

pub(super) struct OperationBodyInput {
    pub(super) security: TokenStream,
    pub(super) request_parts: RequestParts,
}

struct OperationCall {
    method: proc_macro2::Ident,
    setup: TokenStream,
    options: TokenStream,
    success: TokenStream,
}

pub(super) fn operation_body(
    operation: &Operation,
    response_type: &TokenStream,
    error_type: Option<&proc_macro2::Ident>,
    input: OperationBodyInput,
) -> TokenStream {
    let decode = response_decode(operation, response_type, error_type);
    let http_method = method_tokens(operation.method);
    let query_setup = input.request_parts.query;
    let headers_setup = input.request_parts.headers;
    let body = input.request_parts.body;
    let security = input.security;
    let request_setup = quote! {
        #query_setup
        #headers_setup
    };
    let request_options = quote! {
        RequestOptions {
            query: &query,
            headers: &headers,
            body: #body,
            requirements: #security,
        }
    };
    let success = quote! {
        let body = response.body;
        #decode
    };
    let call = OperationCall {
        method: http_method,
        setup: request_setup,
        options: request_options,
        success,
    };
    match error_type {
        Some(error_type) => render_error_operation(operation, error_type, call),
        None => render_success_operation(call),
    }
}

fn render_error_operation(
    operation: &Operation,
    error_type: &proc_macro2::Ident,
    call: OperationCall,
) -> TokenStream {
    let decoder = error_decoder_name(operation);
    let OperationCall {
        method,
        setup,
        options,
        success,
    } = call;
    quote! {
        #setup
        let response = self.request(Method::#method, &path, #options)
            .await
            .map_err(#error_type::Transport)?;
        if (200..300).contains(&response.status) {
            #success
        } else {
            Err(#decoder(response))
        }
    }
}

fn render_success_operation(call: OperationCall) -> TokenStream {
    let OperationCall {
        method,
        setup,
        options,
        success,
    } = call;
    quote! {
        #setup
        let response = self.request(Method::#method, &path, #options).await?;
        if (200..300).contains(&response.status) {
            #success
        } else {
            Err(SdkError::Http { status: response.status, body: response.body })
        }
    }
}

pub(super) fn operation_arguments(
    operation: &Operation,
    spec: &ApiIr,
) -> (Vec<TokenStream>, Vec<TokenStream>, RequestParts) {
    let (mut arguments, path_arguments, query_setup, header_setup) =
        parameter_arguments(operation, spec);
    let query = if query_setup.is_empty() {
        quote! { let query: Vec<(&str, String)> = Vec::new(); }
    } else {
        quote! { let mut query: Vec<(&str, String)> = Vec::new(); }
    };
    let query = quote! {
        #query
        #(#query_setup)*
    };
    let headers = if header_setup.is_empty() {
        quote! { let headers: Vec<(&str, String)> = Vec::new(); }
    } else {
        quote! { let mut headers: Vec<(&str, String)> = Vec::new(); }
    };
    let headers = quote! {
        #headers
        #(#header_setup)*
    };
    let body = request_body_argument(operation, spec, &mut arguments);
    (
        arguments,
        path_arguments,
        RequestParts {
            query,
            headers,
            body,
        },
    )
}

fn parameter_arguments(
    operation: &Operation,
    spec: &ApiIr,
) -> (
    Vec<TokenStream>,
    Vec<TokenStream>,
    Vec<TokenStream>,
    Vec<TokenStream>,
) {
    let entries = operation
        .parameters
        .iter()
        .map(|parameter| parameter_argument(parameter, spec))
        .collect::<Vec<_>>();
    let arguments = entries.iter().map(|entry| entry.0.clone()).collect();
    let path_arguments = entries
        .iter()
        .filter(|entry| !entry.1.is_empty())
        .map(|entry| entry.1.clone())
        .collect();
    let query_setup = entries.iter().flat_map(|entry| entry.2.clone()).collect();
    let header_setup = entries.iter().flat_map(|entry| entry.3.clone()).collect();
    (arguments, path_arguments, query_setup, header_setup)
}

fn parameter_argument(
    parameter: &crate::Parameter,
    spec: &ApiIr,
) -> (TokenStream, TokenStream, Vec<TokenStream>, Vec<TokenStream>) {
    let name = format_ident!("{}", rust_identifier(&parameter.name));
    if parameter.location == ParameterLocation::Path {
        return (
            quote! { #name: &str },
            quote! { crate::client::encode_path_segment(#name) },
            Vec::new(),
            Vec::new(),
        );
    }
    let ty = parameter_type(&parameter.schema, spec);
    let ty = if parameter.required {
        ty
    } else {
        quote! { Option<#ty> }
    };
    let setup = if parameter.required {
        quote! { #name.to_string() }
    } else {
        quote! { #name.as_ref().map(ToString::to_string) }
    };
    let mut query = Vec::new();
    let mut headers = Vec::new();
    add_parameter_setup(
        ParameterSetup {
            parameter,
            name: &name,
            key: literal(&parameter.name),
            setup,
        },
        &mut query,
        &mut headers,
    );
    (quote! { #name: #ty }, quote! {}, query, headers)
}

struct ParameterSetup<'a> {
    parameter: &'a crate::Parameter,
    name: &'a syn::Ident,
    key: syn::LitStr,
    setup: TokenStream,
}

fn add_parameter_setup(
    input: ParameterSetup<'_>,
    query: &mut Vec<TokenStream>,
    headers: &mut Vec<TokenStream>,
) {
    match input.parameter.location {
        ParameterLocation::Query => add_query_setup(input, query),
        ParameterLocation::Header => add_header_setup(input, headers),
        ParameterLocation::Cookie => add_cookie_setup(input, headers),
        ParameterLocation::Path => unreachable!(),
    }
}

fn add_query_setup(input: ParameterSetup<'_>, query: &mut Vec<TokenStream>) {
    let key = input.key;
    let setup = input.setup;
    if input.parameter.required {
        query.push(quote! { query.push((#key, #setup)); });
    } else {
        query.push(quote! { if let Some(value) = #setup { query.push((#key, value)); } });
    }
}

fn add_header_setup(input: ParameterSetup<'_>, headers: &mut Vec<TokenStream>) {
    let key = input.key;
    let setup = input.setup;
    if input.parameter.required {
        headers.push(quote! { headers.push((#key, #setup)); });
    } else {
        headers.push(quote! { if let Some(value) = #setup { headers.push((#key, value)); } });
    }
}

fn add_cookie_setup(input: ParameterSetup<'_>, headers: &mut Vec<TokenStream>) {
    let key = input.key;
    let name = input.name;
    if input.parameter.required {
        headers
            .push(quote! { headers.push(("Cookie", format!("{}={}", #key, #name.to_string()))); });
    } else {
        headers.push(quote! { if let Some(value) = #name.as_ref() { headers.push(("Cookie", format!("{}={}", #key, value))); } });
    }
}

fn request_body_argument(
    operation: &Operation,
    spec: &ApiIr,
    arguments: &mut Vec<TokenStream>,
) -> TokenStream {
    let Some(request_body) = operation.request_body_details.as_ref() else {
        return quote! { None };
    };
    if request_body.content_type != "application/json" {
        return quote! { None };
    }
    let body_type = request_body
        .schema
        .as_ref()
        .map(|schema| parameter_type(schema, spec))
        .unwrap_or_else(|| quote! { serde_json::Value });
    let name = format_ident!("request_body");
    if request_body.required {
        arguments.push(quote! { #name: &#body_type });
        quote! {
            Some(serde_json::to_string(#name)
                .map_err(|error| SdkError::Serialization(error.to_string()))?)
        }
    } else {
        arguments.push(quote! { #name: Option<&#body_type> });
        quote! {
            #name.map(serde_json::to_string)
                .transpose()
                .map_err(|error| SdkError::Serialization(error.to_string()))?
        }
    }
}

fn parameter_type(schema: &Schema, spec: &ApiIr) -> TokenStream {
    match schema {
        Schema::String { .. } => quote! { String },
        Schema::Integer { .. } => quote! { i64 },
        Schema::Number { .. } => quote! { f64 },
        Schema::Boolean => quote! { bool },
        Schema::Reference(name) => {
            let name = format_ident!("{}", rust_type_name(name));
            quote! { #name }
        }
        Schema::Array(item) => {
            let item = parameter_type(item, spec);
            quote! { Vec<#item> }
        }
        _ => {
            let _ = spec;
            quote! { serde_json::Value }
        }
    }
}
