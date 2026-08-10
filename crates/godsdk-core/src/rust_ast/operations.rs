use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{ApiIr, HttpMethod, Operation, ParameterLocation, Schema, SecuritySchemeKind};

use super::{literal, rust_identifier, rust_type_name};

pub(super) fn render(spec: &ApiIr) -> TokenStream {
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
        .map(render_error_contract);
    quote! {
        use reqwest::Method;

        use crate::client::{#auth_import #response_import Client, SdkError};
        use crate::models::*;

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
    let (arguments, path_arguments) = operation_arguments(operation);
    let path = operation_path(operation, &path_arguments);
    let response_type = response_type(operation);
    let security = operation_security(operation, spec);
    let error_type = has_error_responses(operation).then(|| error_type_name(operation));
    let return_type = error_type
        .as_ref()
        .map_or_else(|| quote! { SdkError }, |error_type| quote! { #error_type });
    let body = operation_body(operation, &response_type, error_type.as_ref(), security);
    quote! {
        pub async fn #method(&self, #(#arguments),*) -> Result<#response_type, #return_type> {
            #path
            #body
        }
    }
}

fn operation_body(
    operation: &Operation,
    response_type: &TokenStream,
    error_type: Option<&proc_macro2::Ident>,
    security: TokenStream,
) -> TokenStream {
    let decode = response_decode(operation, response_type, error_type);
    let http_method = method_tokens(operation.method);
    let call = quote! { self.request(Method::#http_method, &path, #security).await? };
    let success = quote! {
        let body = response.body;
        #decode
    };
    match error_type {
        Some(error_type) => {
            let decoder = error_decoder_name(operation);
            quote! {
                let response = self.request(Method::#http_method, &path, #security)
                    .await
                    .map_err(#error_type::Transport)?;
                if (200..300).contains(&response.status) {
                    #success
                } else {
                    Err(#decoder(response))
                }
            }
        }
        None => quote! {
            let response = #call;
            if (200..300).contains(&response.status) {
                #success
            } else {
                Err(SdkError::Http { status: response.status, body: response.body })
            }
        },
    }
}

fn error_type_name(operation: &Operation) -> proc_macro2::Ident {
    format_ident!("{}Error", rust_type_name(&operation.operation_id))
}

fn error_decoder_name(operation: &Operation) -> proc_macro2::Ident {
    format_ident!("decode_{}_error", rust_identifier(&operation.operation_id))
}

fn render_error_contract(operation: &Operation) -> TokenStream {
    let error_type = error_type_name(operation);
    let decoder = error_decoder_name(operation);
    let variants = operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
        .map(error_variant);
    let arms = operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
        .filter_map(|response| error_decoder_arm(response, &error_type));
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
                status => #error_type::Unexpected { status, body: response.body },
            }
        }
    }
}

fn error_variant(response: &crate::Response) -> TokenStream {
    let variant = status_variant(&response.status);
    let message = format!("API returned HTTP {}", response.status);
    match response.schema.as_ref() {
        Some(schema) => {
            let schema = schema_tokens(schema);
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
) -> Option<TokenStream> {
    let status = response.status.parse::<u16>().ok()?;
    let variant = status_variant(&response.status);
    let arm = match response.schema.as_ref() {
        Some(schema) => {
            let schema = schema_tokens(schema);
            quote! {
                #status => match serde_json::from_str::<#schema>(&response.body) {
                    Ok(value) => #error_type::#variant(value),
                    Err(_) => #error_type::Unexpected {
                        status: response.status,
                        body: response.body,
                    },
                },
            }
        }
        None => quote! { #status => #error_type::#variant, },
    };
    Some(arm)
}

fn status_variant(status: &str) -> proc_macro2::Ident {
    format_ident!("Status{}", rust_type_name(status))
}

fn operation_security(operation: &Operation, spec: &ApiIr) -> TokenStream {
    let Some(requirements) = operation.security.as_ref().or(spec.security.as_ref()) else {
        return quote! { None };
    };
    let alternatives = requirements
        .iter()
        .map(|requirement| render_security_alternative(requirement, spec));
    quote! { Some(&[#(#alternatives),*]) }
}

fn render_security_alternative(
    requirement: &crate::SecurityRequirement,
    spec: &ApiIr,
) -> TokenStream {
    let schemes = requirement
        .schemes
        .iter()
        .map(|required| render_security_requirement(required, spec));
    quote! { &[#(#schemes),*][..] }
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

fn operation_arguments(operation: &Operation) -> (Vec<TokenStream>, Vec<TokenStream>) {
    let parameters = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path);
    let mut arguments = Vec::new();
    let mut path_arguments = Vec::new();
    for parameter in parameters {
        let name = format_ident!("{}", rust_identifier(&parameter.name));
        arguments.push(quote! { #name: &str });
        path_arguments.push(quote! { crate::client::encode_path_segment(#name) });
    }
    (arguments, path_arguments)
}

fn operation_path(operation: &Operation, path_arguments: &[TokenStream]) -> TokenStream {
    let (path_format, has_arguments) = path_template(operation);
    let path_literal = literal(&path_format);
    if has_arguments {
        quote! { let path = format!(#path_literal, #(#path_arguments),*); }
    } else {
        quote! { let path = #path_literal.to_string(); }
    }
}

fn response_decode(
    operation: &Operation,
    response_type: &TokenStream,
    error_type: Option<&proc_macro2::Ident>,
) -> TokenStream {
    if is_string_response(operation) {
        quote! { Ok(body) }
    } else {
        let serialization = error_type.map_or_else(
            || quote! { SdkError::Serialization(error.to_string()) },
            |error_type| quote! { #error_type::Transport(SdkError::Serialization(error.to_string())) },
        );
        quote! { serde_json::from_str::<#response_type>(&body).map_err(|error| #serialization) }
    }
}

fn path_template(operation: &Operation) -> (String, bool) {
    let mut path = operation.path.clone();
    let mut has_arguments = false;
    for parameter in operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
    {
        path = path.replace(&format!("{{{}}}", parameter.name), "{}");
        has_arguments = true;
    }
    (path, has_arguments)
}

fn method_tokens(method: HttpMethod) -> proc_macro2::Ident {
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

fn response_type(operation: &Operation) -> TokenStream {
    operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2') && response.schema.is_some())
        .and_then(|response| response.schema.as_ref())
        .map(schema_tokens)
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

fn schema_tokens(schema: &Schema) -> TokenStream {
    match schema {
        Schema::String { .. } => quote! { String },
        Schema::Integer { .. } => quote! { i64 },
        Schema::Number { .. } => quote! { f64 },
        Schema::Boolean => quote! { bool },
        Schema::Null => quote! { () },
        Schema::Array(item) => {
            let item = schema_tokens(item);
            quote! { Vec<#item> }
        }
        Schema::Object {
            additional_properties: Some(value),
            properties,
            ..
        } if properties.is_empty() => {
            let value = schema_tokens(value);
            quote! { std::collections::BTreeMap<String, #value> }
        }
        Schema::Object { .. } => quote! { serde_json::Map<String, serde_json::Value> },
        Schema::Reference(name) => {
            let ident = format_ident!("{}", rust_type_name(name));
            quote! { #ident }
        }
        Schema::Nullable(inner) => {
            let inner = schema_tokens(inner);
            quote! { Option<#inner> }
        }
        Schema::Enum(_) | Schema::OneOf(_) | Schema::AnyOf(_) | Schema::AllOf(_) => {
            quote! { serde_json::Value }
        }
    }
}
