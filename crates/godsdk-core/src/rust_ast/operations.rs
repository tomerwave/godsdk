use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{ApiIr, HttpMethod, Operation, ParameterLocation, Schema};

use super::{literal, rust_identifier, rust_type_name};

pub(super) fn render(spec: &ApiIr) -> TokenStream {
    let methods = spec.operations.iter().map(render_operation);
    quote! {
        use reqwest::Method;

        use crate::client::{Client, SdkError};
        use crate::models::*;

        impl Client {
            #(#methods)*
        }
    }
}

fn render_operation(operation: &Operation) -> TokenStream {
    let method = format_ident!("{}", rust_identifier(&operation.operation_id));
    let (arguments, path_arguments) = operation_arguments(operation);
    let path = operation_path(operation, &path_arguments);
    let response_type = response_type(operation);
    let http_method = method_tokens(operation.method);
    let decode = response_decode(operation, &response_type);
    quote! {
        pub async fn #method(&self, #(#arguments),*) -> Result<#response_type, SdkError> {
            #path
            let body = self.request(Method::#http_method, &path).await?;
            #decode
        }
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

fn response_decode(operation: &Operation, response_type: &TokenStream) -> TokenStream {
    if is_string_response(operation) {
        quote! { Ok(body) }
    } else {
        quote! {
            serde_json::from_str::<#response_type>(&body)
                .map_err(|error| SdkError::Serialization(error.to_string()))
        }
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
