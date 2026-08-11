use proc_macro2::TokenStream;
use quote::quote;

use crate::{ApiIr, Operation, Schema};

use super::models::typed_enum_tokens;
use super::operations::{
    inline_parameter_type_name, inline_request_body_type_name, inline_response_type_name,
};

pub(super) fn render(spec: &ApiIr) -> TokenStream {
    let definitions = spec.operations.iter().flat_map(operation_definitions);
    quote! { #(#definitions)* }
}

fn operation_definitions(operation: &Operation) -> Vec<TokenStream> {
    let mut definitions = operation
        .parameters
        .iter()
        .filter_map(|parameter| {
            typed_definition(
                &inline_parameter_type_name(operation, parameter),
                &parameter.schema,
            )
        })
        .collect::<Vec<_>>();
    if let Some(schema) = operation
        .request_body_details
        .as_ref()
        .and_then(|body| body.schema.as_ref())
        && let Some(definition) =
            typed_definition(&inline_request_body_type_name(operation), schema)
    {
        definitions.push(definition);
    }
    if let Some(schema) = operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .and_then(|response| response.schema.as_ref())
        && let Some(definition) = typed_definition(&inline_response_type_name(operation), schema)
    {
        definitions.push(definition);
    }
    definitions
}

fn typed_definition(name: &syn::Ident, schema: &Schema) -> Option<TokenStream> {
    match schema {
        Schema::TypedEnum { base, values } => Some(typed_enum_tokens(name, base, values)),
        Schema::Const { base, value } => {
            Some(typed_enum_tokens(name, base, std::slice::from_ref(value)))
        }
        _ => None,
    }
}
