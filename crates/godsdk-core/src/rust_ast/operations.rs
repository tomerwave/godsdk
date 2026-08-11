use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{ApiIr, HttpMethod, Operation, ParameterLocation, Schema, SecuritySchemeKind};

use super::inline_types;
use super::request::{
    OperationBodyArgs, OperationBodyInput, OperationHelpersInput, operation_arguments,
    operation_body, operation_helpers,
};
use super::{literal, rust_identifier, rust_type_name};

pub(super) fn render(spec: &ApiIr) -> TokenStream {
    let inline_types = inline_types::render(spec);
    let methods = spec
        .operations
        .iter()
        .map(|operation| render_operation(operation, spec));
    let auth_import = if has_security(spec) {
        quote! { AuthRequirement, }
    } else {
        quote! {}
    };
    let response_import = if has_error_operations(spec) {
        quote! { HttpResponse, }
    } else {
        quote! {}
    };
    let errors = spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
        .map(|operation| render_error_contract(operation, spec));
    let request_types = spec
        .operations
        .iter()
        .map(|operation| render_request_type(operation, spec));
    quote! {
        use reqwest::Method;

        use crate::client::{#auth_import #response_import Client, RequestBody, RequestOptions, SdkError};
        use crate::models::*;

        #(#request_types)*
        #inline_types

        #(#errors)*

        impl Client {
            #(#methods)*
        }
    }
}

fn has_error_operations(spec: &ApiIr) -> bool {
    spec.operations.iter().any(has_error_responses)
}

fn has_error_responses(operation: &Operation) -> bool {
    operation
        .responses
        .iter()
        .any(|response| !response.status.starts_with('2'))
}

fn has_security(spec: &ApiIr) -> bool {
    spec.security.is_some()
        || spec
            .operations
            .iter()
            .any(|operation| operation.security.is_some())
}

fn render_operation(operation: &Operation, spec: &ApiIr) -> TokenStream {
    let method = format_ident!("{}", rust_identifier(&operation.operation_id));
    let request_type = request_type_name(operation);
    let (_arguments, path_arguments, request_parts) = operation_arguments(operation, spec);
    let path = operation_path(operation, &path_arguments);
    let path_helper = format_ident!("{}_path", rust_identifier(&operation.operation_id));
    let options_helper = format_ident!("{}_options", rust_identifier(&operation.operation_id));
    let path_fields = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|parameter| format_ident!("{}", rust_identifier(&parameter.name)))
        .collect::<Vec<_>>();
    let helpers = operation_helpers(
        operation,
        OperationHelpersInput {
            request_type: request_type.clone(),
            path_helper: path_helper.clone(),
            options_helper: options_helper.clone(),
            path,
            path_fields,
            body: OperationBodyInput {
                security: operation_security(operation, spec),
                request_parts,
            },
        },
    );
    let response_type = response_type(operation, spec);
    let error_type = has_error_responses(operation).then(|| error_type_name(operation));
    let return_type = error_type
        .as_ref()
        .map_or_else(|| quote! { SdkError }, |error_type| quote! { #error_type });
    let body = operation_body(OperationBodyArgs {
        operation,
        response_type: &response_type,
        error_type: error_type.as_ref(),
        path_helper: &path_helper,
        options_helper: &options_helper,
    });
    quote! {
        #helpers
        pub async fn #method(&self, request: #request_type) -> Result<#response_type, #return_type> {
            #body
        }
    }
}

fn render_request_type(operation: &Operation, spec: &ApiIr) -> TokenStream {
    let request_type = request_type_name(operation);
    let (fields, _, _) = operation_arguments(operation, spec);
    quote! {
        #[derive(Debug, Clone)]
        pub struct #request_type {
            #(pub #fields,)*
        }
    }
}

pub(super) fn inline_parameter_type_name(
    operation: &Operation,
    parameter: &crate::Parameter,
) -> syn::Ident {
    format_ident!(
        "{}{}",
        rust_type_name(&operation.operation_id),
        rust_type_name(&parameter.name)
    )
}

pub(super) fn inline_request_body_type_name(operation: &Operation) -> syn::Ident {
    format_ident!("{}RequestBody", rust_type_name(&operation.operation_id))
}

pub(super) fn inline_response_type_name(operation: &Operation) -> syn::Ident {
    format_ident!("{}Response", rust_type_name(&operation.operation_id))
}

fn request_type_name(operation: &Operation) -> proc_macro2::Ident {
    format_ident!("{}Request", rust_type_name(&operation.operation_id))
}

fn error_type_name(operation: &Operation) -> proc_macro2::Ident {
    format_ident!("{}Error", rust_type_name(&operation.operation_id))
}

pub(super) fn error_decoder_name(operation: &Operation) -> proc_macro2::Ident {
    format_ident!("decode_{}_error", rust_identifier(&operation.operation_id))
}

fn render_error_contract(operation: &Operation, spec: &ApiIr) -> TokenStream {
    let error_type = error_type_name(operation);
    let decoder = error_decoder_name(operation);
    let variants = operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
        .map(|response| error_variant(response, spec));
    let arms = operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
        .filter_map(|response| error_decoder_arm(response, &error_type, spec));
    quote! {
        #[derive(Debug, thiserror::Error)]
        pub enum #error_type {
            #[error("transport error: {0}")]
            Transport(#[from] SdkError),
            #(#variants)*
            #[error("unexpected API response: HTTP {status}")]
            Unexpected { status: u16, body: String },
        }

        fn #decoder(response: HttpResponse) -> #error_type {
            match response.status {
                #(#arms)*
                status => #error_type::Unexpected { status, body: String::from_utf8_lossy(&response.body).into_owned() },
            }
        }
    }
}

fn error_variant(response: &crate::Response, spec: &ApiIr) -> TokenStream {
    let variant = status_variant(&response.status);
    let mut message = String::from("API returned HTTP ");
    message.push_str(&response.status);
    match response.schema.as_ref() {
        Some(schema) => {
            let schema = schema_tokens(schema, spec);
            quote! {
                #[error(#message)]
                #variant(#schema),
            }
        }
        None => quote! {
            #[error(#message)]
            #variant,
        },
    }
}

fn error_decoder_arm(
    response: &crate::Response,
    error_type: &proc_macro2::Ident,
    spec: &ApiIr,
) -> Option<TokenStream> {
    let status = response.status.parse::<u16>().ok()?;
    let variant = status_variant(&response.status);
    let arm = match response.schema.as_ref() {
        Some(schema) => {
            let schema = schema_tokens(schema, spec);
            quote! {
                #status => match serde_json::from_slice::<#schema>(&response.body) {
                    Ok(value) => #error_type::#variant(value),
                    Err(_) => #error_type::Unexpected {
                        status: response.status,
                        body: String::from_utf8_lossy(&response.body).into_owned(),
                    },
                },
            }
        }
        None => quote! { #status => #error_type::#variant, },
    };
    Some(arm)
}

fn status_variant(status: &str) -> proc_macro2::Ident {
    let suffix = rust_type_name(status);
    let suffix = suffix.strip_prefix('_').unwrap_or(&suffix);
    format_ident!("Status{suffix}")
}

fn operation_security(operation: &Operation, spec: &ApiIr) -> TokenStream {
    let Some(requirements) = operation.security.as_ref().or(spec.security.as_ref()) else {
        return quote! { None };
    };
    let alternatives = requirements
        .iter()
        .map(|requirement| render_security_alternative(requirement, spec));
    quote! { Some(vec![#(#alternatives),*]) }
}

fn render_security_alternative(
    requirement: &crate::SecurityRequirement,
    spec: &ApiIr,
) -> TokenStream {
    let schemes = requirement
        .schemes
        .iter()
        .map(|required| render_security_requirement(required, spec));
    quote! { vec![#(#schemes),*] }
}

fn render_security_requirement(
    required: &crate::RequiredSecurityScheme,
    spec: &ApiIr,
) -> TokenStream {
    let scheme = spec
        .security_schemes
        .get(&required.name)
        .unwrap_or_else(|| panic!("validated security scheme is present"));
    render_security_kind(&required.name, &scheme.kind)
}

fn render_security_kind(name: &str, kind: &SecuritySchemeKind) -> TokenStream {
    let name = literal(name);
    match kind {
        SecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("bearer") => {
            quote! { AuthRequirement::Bearer { scheme: #name } }
        }
        SecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("basic") => {
            quote! { AuthRequirement::Basic { scheme: #name } }
        }
        SecuritySchemeKind::Http { .. } => quote! { AuthRequirement::Http { scheme: #name } },
        SecuritySchemeKind::ApiKey {
            name: key_name,
            location: ParameterLocation::Header,
        } => {
            let key_name = literal(key_name);
            quote! { AuthRequirement::ApiKeyHeader { scheme: #name, name: #key_name } }
        }
        SecuritySchemeKind::ApiKey {
            name: key_name,
            location: ParameterLocation::Query,
        } => {
            let key_name = literal(key_name);
            quote! { AuthRequirement::ApiKeyQuery { scheme: #name, name: #key_name } }
        }
        SecuritySchemeKind::ApiKey {
            name: key_name,
            location: ParameterLocation::Cookie,
        } => {
            let key_name = literal(key_name);
            quote! { AuthRequirement::ApiKeyCookie { scheme: #name, name: #key_name } }
        }
        SecuritySchemeKind::ApiKey {
            location: ParameterLocation::Path,
            ..
        } => panic!("validated API key cannot use a path location"),
        SecuritySchemeKind::OAuth2 { .. } => quote! { AuthRequirement::Bearer { scheme: #name } },
    }
}

fn operation_path(operation: &Operation, path_arguments: &[TokenStream]) -> TokenStream {
    let pushes = path_pushes(operation.path.as_str(), path_arguments);
    quote! {
        let mut path = String::new();
        #(#pushes)*
    }
}

fn path_pushes(path: &str, path_arguments: &[TokenStream]) -> Vec<TokenStream> {
    let mut pushes = Vec::new();
    let mut remaining = path;
    for argument in path_arguments {
        let Some((segment, rest)) = path_push_segment(remaining, argument) else {
            break;
        };
        pushes.extend(segment);
        remaining = rest;
    }
    if !remaining.is_empty() {
        pushes.push(quote! { path.push_str(#remaining); });
    }
    pushes
}

fn path_push_segment<'path>(
    remaining: &'path str,
    argument: &TokenStream,
) -> Option<(Vec<TokenStream>, &'path str)> {
    let start = remaining.find('{')?;
    let end = start + remaining[start..].find('}')? + 1;
    let mut segment = Vec::new();
    let prefix = &remaining[..start];
    if !prefix.is_empty() {
        segment.push(quote! { path.push_str(#prefix); });
    }
    segment.push(quote! { path.push_str(&#argument); });
    Some((segment, &remaining[end..]))
}

pub(super) fn response_decode(
    operation: &Operation,
    response_type: &TokenStream,
    error_type: Option<&proc_macro2::Ident>,
) -> TokenStream {
    let serialization = error_type.map_or_else(
        || quote! { SdkError::Serialization(error.to_string()) },
        |error_type| quote! { #error_type::Transport(SdkError::Serialization(error.to_string())) },
    );
    if is_string_response(operation) {
        if is_binary_response(operation) {
            return quote! { Ok(body) };
        }
        quote! { Ok(String::from_utf8(body).map_err(|error| #serialization)? ) }
    } else {
        quote! { serde_json::from_slice::<#response_type>(&body).map_err(|error| #serialization) }
    }
}

fn is_binary_response(operation: &Operation) -> bool {
    operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2') && response.schema.is_some())
        .and_then(|response| response.schema.as_ref())
        .is_some_and(|schema| matches!(schema, Schema::String { format: Some(format) } if format == "binary"))
}

pub(super) fn method_tokens(method: HttpMethod) -> proc_macro2::Ident {
    format_ident!(
        "{}",
        match method {
            HttpMethod::Delete => "DELETE",
            HttpMethod::Get => "GET",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Trace => "TRACE",
        }
    )
}

fn response_type(operation: &Operation, spec: &ApiIr) -> TokenStream {
    operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2') && response.schema.is_some())
        .and_then(|response| response.schema.as_ref())
        .map(|schema| {
            let inline = matches!(schema, Schema::TypedEnum { .. } | Schema::Const { .. })
                .then(|| inline_response_type_name(operation));
            schema_tokens_with_inline(schema, spec, inline.as_ref())
        })
        .unwrap_or_else(|| quote! { String })
}

fn is_string_response(operation: &Operation) -> bool {
    operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2') && response.schema.is_some())
        .and_then(|response| response.schema.as_ref())
        .is_none_or(|schema| matches!(schema, Schema::String { .. }))
}

fn schema_tokens(schema: &Schema, spec: &ApiIr) -> TokenStream {
    schema_tokens_with_inline(schema, spec, None)
}

fn schema_tokens_with_inline(
    schema: &Schema,
    spec: &ApiIr,
    inline: Option<&syn::Ident>,
) -> TokenStream {
    match schema {
        Schema::Any => quote! { serde_json::Value },
        Schema::String {
            format: Some(format),
        } if format == "binary" => quote! { Vec<u8> },
        Schema::String { .. } => quote! { String },
        Schema::Integer { .. } => quote! { i64 },
        Schema::Number { .. } => quote! { f64 },
        Schema::Boolean => quote! { bool },
        Schema::TypedEnum { base, .. } => {
            inline.map_or_else(|| schema_tokens(base, spec), |ident| quote! { #ident })
        }
        Schema::Const { base, .. } => schema_tokens(base, spec),
        Schema::Null => quote! { () },
        Schema::Array(item) => {
            let item = schema_tokens(item, spec);
            quote! { Vec<#item> }
        }
        Schema::Object {
            additional_properties: Some(value),
            properties,
            ..
        } if properties.is_empty() => {
            let value = schema_tokens(value, spec);
            quote! { std::collections::BTreeMap<String, #value> }
        }
        Schema::Object { .. } => quote! { serde_json::Map<String, serde_json::Value> },
        Schema::Reference(name) => {
            if spec.schemas.contains_key(name) {
                let ident = format_ident!("{}", rust_type_name(name));
                quote! { #ident }
            } else {
                quote! { serde_json::Value }
            }
        }
        Schema::Nullable(inner) => {
            let inner = schema_tokens(inner, spec);
            quote! { Option<#inner> }
        }
        Schema::Enum(_) | Schema::OneOf(_) | Schema::AnyOf(_) | Schema::AllOf(_) => {
            quote! { serde_json::Value }
        }
    }
}
