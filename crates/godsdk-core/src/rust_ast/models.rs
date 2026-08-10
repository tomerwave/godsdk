use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{ApiSpec, Schema};

use super::{rust_identifier, rust_type_name, snake_case};

pub(super) fn render(spec: &ApiSpec) -> Vec<(String, String)> {
    let modules = spec.schemas.keys().map(|name| {
        let module = format_ident!("{}", snake_case(name));
        quote! { mod #module; pub use #module::*; }
    });
    let mut files = vec![rust_file(
        "sdk/rust/src/models/mod.rs",
        quote! { #(#modules)* },
    )];
    for (name, schema) in &spec.schemas {
        files.push(rust_file(
            &format!("sdk/rust/src/models/{}.rs", snake_case(name)),
            model_tokens(name, schema, spec),
        ));
    }
    files
}

fn rust_file(path: &str, tokens: TokenStream) -> (String, String) {
    let file = syn::parse2::<syn::File>(tokens).unwrap_or_else(|error| {
        panic!("Rust generator emitted invalid syntax for {path}: {error}")
    });
    (path.to_string(), prettyplease::unparse(&file))
}

fn model_tokens(name: &str, schema: &Schema, spec: &ApiSpec) -> TokenStream {
    let ident = format_ident!("{}", rust_type_name(name));
    match schema {
        Schema::Enum(values) => render_enum(&ident, values),
        Schema::OneOf(variants) | Schema::AnyOf(variants) => render_union(&ident, variants),
        Schema::Object { .. } | Schema::AllOf(_) => render_object(&ident, schema, spec),
        Schema::Reference(reference) => {
            let target = format_ident!("{}", rust_type_name(reference));
            quote! { pub type #ident = #target; }
        }
        other => {
            let ty = schema_tokens(other);
            quote! { pub type #ident = #ty; }
        }
    }
}

fn render_enum(ident: &syn::Ident, values: &[String]) -> TokenStream {
    let variants = values.iter().map(|value| {
        let variant = format_ident!("{}", rust_type_name(value));
        quote! { #[serde(rename = #value)] #variant, }
    });
    quote! {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
        pub enum #ident { #(#variants)* }
    }
}

fn render_union(ident: &syn::Ident, variants: &[Schema]) -> TokenStream {
    let variants = variants.iter().filter_map(|variant| match variant {
        Schema::Reference(reference) => {
            let variant_ident = format_ident!("{}", rust_type_name(reference));
            let type_ident = format_ident!("{}", rust_type_name(reference));
            Some(quote! { #variant_ident(#type_ident), })
        }
        _ => None,
    });
    quote! {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
        #[serde(untagged)]
        pub enum #ident { #(#variants)* }
    }
}

fn render_object(ident: &syn::Ident, schema: &Schema, spec: &ApiSpec) -> TokenStream {
    let (properties, required, additional) = object_shape(schema, spec);
    let fields = properties.iter().map(|(property, property_schema)| {
        let field = format_ident!("{}", rust_identifier(property));
        let property_type = schema_tokens(property_schema);
        let property_type = if required.contains(property) {
            quote! { #property_type }
        } else {
            quote! { Option<#property_type> }
        };
        quote! { #[serde(rename = #property)] pub #field: #property_type, }
    });
    let extra = additional.map(|schema| {
        let ty = schema_tokens(&schema);
        quote! { #[serde(flatten)] pub additional_properties: BTreeMap<String, #ty>, }
    });
    quote! {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
        pub struct #ident { #(#fields)* #extra }
    }
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
            quote! { BTreeMap<String, #value> }
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

fn object_shape(
    schema: &Schema,
    spec: &ApiSpec,
) -> (BTreeMap<String, Schema>, BTreeSet<String>, Option<Schema>) {
    match schema {
        Schema::Object {
            properties,
            required,
            additional_properties,
        } => (
            properties.clone(),
            required.clone(),
            additional_properties.as_deref().cloned(),
        ),
        Schema::AllOf(parts) => {
            let mut properties = BTreeMap::new();
            let mut required = BTreeSet::new();
            let mut additional = None;
            for part in parts {
                let (part_properties, part_required, part_additional) = object_shape(part, spec);
                properties.extend(part_properties);
                required.extend(part_required);
                additional = additional.or(part_additional);
            }
            (properties, required, additional)
        }
        Schema::Reference(name) => spec
            .schemas
            .get(name)
            .map(|schema| object_shape(schema, spec))
            .unwrap_or_else(|| (BTreeMap::new(), BTreeSet::new(), None)),
        _ => (BTreeMap::new(), BTreeSet::new(), None),
    }
}
