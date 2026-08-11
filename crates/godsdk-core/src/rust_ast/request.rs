use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{ApiIr, Operation, ParameterLocation, ParameterStyle, Schema};

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

pub(super) struct OperationHelpersInput {
    pub(super) request_type: syn::Ident,
    pub(super) path_helper: syn::Ident,
    pub(super) options_helper: syn::Ident,
    pub(super) path: TokenStream,
    pub(super) path_fields: Vec<syn::Ident>,
    pub(super) body: OperationBodyInput,
}

pub(super) struct OperationBodyArgs<'a> {
    pub(super) operation: &'a Operation,
    pub(super) response_type: &'a TokenStream,
    pub(super) error_type: Option<&'a proc_macro2::Ident>,
    pub(super) path_helper: &'a syn::Ident,
    pub(super) options_helper: &'a syn::Ident,
}

pub(super) fn operation_body(args: OperationBodyArgs<'_>) -> TokenStream {
    let OperationBodyArgs {
        operation,
        response_type,
        error_type,
        path_helper,
        options_helper,
    } = args;
    let decode = response_decode(operation, response_type, error_type);
    let http_method = method_tokens(operation.method);
    match error_type {
        Some(error_type) => {
            let decoder = error_decoder_name(operation);
            quote! {
                let path = Self::#path_helper(&request)?;
                let options = Self::#options_helper(&request)?;
                let response = self.request(Method::#http_method, &path, options)
                    .await
                    .map_err(#error_type::Transport)?;
                if (200..300).contains(&response.status) {
                    let body = response.body;
                    #decode
                } else {
                    Err(#decoder(response))
                }
            }
        }
        None => quote! {
            let path = Self::#path_helper(&request)?;
            let options = Self::#options_helper(&request)?;
            let response = self.request(Method::#http_method, &path, options).await?;
            if (200..300).contains(&response.status) {
                let body = response.body;
                #decode
            } else {
            Err(SdkError::Http {
                status: response.status,
                body: String::from_utf8_lossy(&response.body).into_owned(),
            })
            }
        },
    }
}

pub(super) fn operation_helpers(
    operation: &Operation,
    input: OperationHelpersInput,
) -> TokenStream {
    let OperationHelpersInput {
        request_type,
        path_helper,
        options_helper,
        path,
        path_fields,
        body,
    } = input;
    let (security, query, headers, body) = operation_helper_parts(body);
    let (path_destructure, query_destructure, header_destructure, body_destructure) =
        request_destructuring(operation, &request_type, &path_fields);
    let query_helper = format_ident!("{}_query", options_helper);
    let headers_helper = format_ident!("{}_headers", options_helper);
    let body_helper = format_ident!("{}_body", options_helper);
    quote! {
        fn #path_helper(request: &#request_type) -> Result<String, SdkError> {
            #path_destructure
            #path
            Ok(path)
        }

        fn #query_helper(request: &#request_type) -> Result<Vec<(String, String)>, SdkError> {
            #query_destructure
            #query
            Ok(query)
        }

        fn #headers_helper(request: &#request_type) -> Result<Vec<(String, String)>, SdkError> {
            #header_destructure
            #headers
            Ok(headers)
        }

        fn #body_helper(request: &#request_type) -> Result<Option<RequestBody>, SdkError> {
            #body_destructure
            Ok(#body)
        }

        fn #options_helper(request: &#request_type) -> Result<RequestOptions, SdkError> {
            Ok(RequestOptions {
                query: Self::#query_helper(request)?,
                headers: Self::#headers_helper(request)?,
                body: Self::#body_helper(request)?,
                requirements: #security,
            })
        }
    }
}

fn operation_helper_parts(
    input: OperationBodyInput,
) -> (TokenStream, TokenStream, TokenStream, TokenStream) {
    let OperationBodyInput {
        security,
        request_parts:
            RequestParts {
                query,
                headers,
                body,
            },
    } = input;
    (security, query, headers, body)
}

fn request_destructuring(
    operation: &Operation,
    request_type: &syn::Ident,
    path_fields: &[syn::Ident],
) -> (TokenStream, TokenStream, TokenStream, TokenStream) {
    let query_fields = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Query)
        .map(|parameter| format_ident!("{}", rust_identifier(&parameter.name)))
        .collect::<Vec<_>>();
    let header_fields = operation
        .parameters
        .iter()
        .filter(|parameter| {
            matches!(
                parameter.location,
                ParameterLocation::Header | ParameterLocation::Cookie
            )
        })
        .map(|parameter| format_ident!("{}", rust_identifier(&parameter.name)))
        .collect::<Vec<_>>();
    let body_fields = operation
        .request_body_details
        .as_ref()
        .filter(|body| {
            matches!(
                body.content_type.as_str(),
                "application/json" | "multipart/form-data" | "application/octet-stream"
            )
        })
        .map(|_| vec![format_ident!("request_body")])
        .unwrap_or_default();
    (
        request_destructure(request_type, path_fields),
        request_destructure(request_type, &query_fields),
        request_destructure(request_type, &header_fields),
        request_destructure(request_type, &body_fields),
    )
}

fn request_destructure(request_type: &syn::Ident, fields: &[syn::Ident]) -> TokenStream {
    if fields.is_empty() {
        quote! { let _ = request; }
    } else {
        quote! { let #request_type { #(#fields),*, .. } = request; }
    }
}

pub(super) fn operation_arguments(
    operation: &Operation,
    spec: &ApiIr,
) -> (Vec<TokenStream>, Vec<TokenStream>, RequestParts) {
    let (mut arguments, path_arguments, query_setup, header_setup) =
        parameter_arguments(operation, spec);
    let query = if query_setup.is_empty() {
        quote! { let query: Vec<(String, String)> = Vec::new(); }
    } else {
        quote! { let mut query: Vec<(String, String)> = Vec::new(); }
    };
    let query = quote! {
        #query
        #(#query_setup)*
    };
    let headers = if header_setup.is_empty() {
        quote! { let headers: Vec<(String, String)> = Vec::new(); }
    } else {
        quote! { let mut headers: Vec<(String, String)> = Vec::new(); }
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
    let ty = parameter_type(&parameter.schema, spec);
    let name_literal = literal(&parameter.name);
    let style = style_literal(parameter.serialization.style);
    let explode = parameter.serialization.explode;
    if parameter.location == ParameterLocation::Path {
        return (
            quote! { #name: #ty },
            quote! {
                crate::client::serialize_path_parameter_value(
                    &#name,
                    #name_literal,
                    #style,
                    #explode,
                )?
            },
            Vec::new(),
            Vec::new(),
        );
    }
    let ty = if parameter.required {
        ty
    } else {
        quote! { Option<#ty> }
    };
    let mut query = Vec::new();
    let mut headers = Vec::new();
    add_parameter_setup(
        ParameterSetup {
            parameter,
            name: &name,
            key: literal(&parameter.name),
            style,
            explode,
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
    style: syn::LitStr,
    explode: bool,
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
    let name = input.name;
    let style = input.style;
    let explode = input.explode;
    if input.parameter.required {
        query.push(quote! {
            query.extend(crate::client::serialize_parameter_value(
                #key,
                &#name,
                #style,
                #explode,
            )?);
        });
    } else {
        query.push(quote! {
            if let Some(value) = #name.as_ref() {
                query.extend(crate::client::serialize_parameter_value(
                    #key,
                    value,
                    #style,
                    #explode,
                )?);
            }
        });
    }
}

fn add_header_setup(input: ParameterSetup<'_>, headers: &mut Vec<TokenStream>) {
    let key = input.key;
    let name = input.name;
    let style = input.style;
    let explode = input.explode;
    if input.parameter.required {
        headers.push(quote! {
        headers.extend(crate::client::serialize_parameter_value(
            #key,
            &#name,
                #style,
                #explode,
            )?);
        });
    } else {
        headers.push(quote! {
            if let Some(value) = #name.as_ref() {
                headers.extend(crate::client::serialize_parameter_value(
                    #key,
                    value,
                    #style,
                    #explode,
                )?);
            }
        });
    }
}

fn add_cookie_setup(input: ParameterSetup<'_>, headers: &mut Vec<TokenStream>) {
    let key = input.key;
    let name = input.name;
    let explode = input.explode;
    if input.parameter.required {
        headers.push(quote! {
            headers.push((
                "Cookie".to_string(),
                crate::client::serialize_cookie_value(
                    #key,
                    &#name,
                    #explode,
                )?,
            ));
        });
    } else {
        headers.push(quote! {
            if let Some(value) = #name.as_ref() {
                headers.push((
                    "Cookie".to_string(),
                    crate::client::serialize_cookie_value(
                        #key,
                        value,
                        #explode,
                    )?,
                ));
            }
        });
    }
}

fn style_literal(style: ParameterStyle) -> syn::LitStr {
    literal(match style {
        ParameterStyle::Simple => "simple",
        ParameterStyle::Form => "form",
        ParameterStyle::Label => "label",
        ParameterStyle::Matrix => "matrix",
        ParameterStyle::SpaceDelimited => "spaceDelimited",
        ParameterStyle::PipeDelimited => "pipeDelimited",
        ParameterStyle::DeepObject => "deepObject",
    })
}

fn request_body_argument(
    operation: &Operation,
    spec: &ApiIr,
    arguments: &mut Vec<TokenStream>,
) -> TokenStream {
    let Some(request_body) = operation.request_body_details.as_ref() else {
        return quote! { None };
    };
    let body_type = request_body
        .schema
        .as_ref()
        .map(|schema| parameter_type(schema, spec))
        .unwrap_or_else(|| quote! { serde_json::Value });
    let name = format_ident!("request_body");
    let content_type = literal(&request_body.content_type);
    let constructor = request_body_constructor(&request_body.content_type, &content_type);
    if request_body.required {
        arguments.push(quote! { #name: #body_type });
        let bytes = required_body_bytes(&request_body.content_type, &name);
        quote! {
            Some(#constructor(#bytes))
        }
    } else {
        arguments.push(quote! { #name: Option<#body_type> });
        let bytes = optional_body_bytes(&request_body.content_type);
        quote! {
            #name.map(|value| #constructor(#bytes))
        }
    }
}

fn request_body_constructor(content_type: &str, content_type_literal: &syn::LitStr) -> TokenStream {
    if content_type == "multipart/form-data" {
        quote! { RequestBody::MultipartJson }
    } else {
        quote! { |bytes| RequestBody::Bytes { content_type: #content_type_literal, bytes } }
    }
}

fn required_body_bytes(content_type: &str, name: &syn::Ident) -> TokenStream {
    if content_type == "application/octet-stream" {
        quote! { #name }
    } else {
        quote! { serde_json::to_vec(&#name).map_err(|error| SdkError::Serialization(error.to_string()))? }
    }
}

fn optional_body_bytes(content_type: &str) -> TokenStream {
    if content_type == "application/octet-stream" {
        quote! { value }
    } else {
        quote! { serde_json::to_vec(&value).map_err(|error| SdkError::Serialization(error.to_string()))? }
    }
}

fn parameter_type(schema: &Schema, spec: &ApiIr) -> TokenStream {
    match schema {
        Schema::String {
            format: Some(format),
        } if format == "binary" => quote! { Vec<u8> },
        Schema::String { .. } => quote! { String },
        Schema::Integer { .. } => quote! { i64 },
        Schema::Number { .. } => quote! { f64 },
        Schema::Boolean => quote! { bool },
        Schema::TypedEnum { base, .. } => parameter_type(base, spec),
        Schema::Reference(name) => {
            if spec.schemas.contains_key(name) {
                let name = format_ident!("{}", rust_type_name(name));
                quote! { #name }
            } else {
                quote! { serde_json::Value }
            }
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
