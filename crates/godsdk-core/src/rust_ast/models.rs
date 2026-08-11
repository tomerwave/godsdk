use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{ApiIr, Schema};

use super::{rust_identifier, rust_type_name, snake_case};

pub(super) fn render(spec: &ApiIr) -> Vec<(String, String)> {
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

fn model_tokens(name: &str, schema: &Schema, spec: &ApiIr) -> TokenStream {
    let ident = format_ident!("{}", rust_type_name(name));
    let body = match schema {
        Schema::Enum(values) => render_enum(&ident, values),
        Schema::TypedEnum { base, values } => render_typed_enum(&ident, base, values),
        Schema::Const { base, value } => {
            render_typed_enum(&ident, base, std::slice::from_ref(value))
        }
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
    };
    quote! {
        #[allow(unused_imports)]
        use std::collections::BTreeMap;
        #[allow(unused_imports)]
        use crate::*;
        #body
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

fn render_typed_enum(
    ident: &syn::Ident,
    base: &Schema,
    values: &[serde_json::Value],
) -> TokenStream {
    let ty = schema_tokens(base);
    let constants = values.iter().enumerate().map(|(index, value)| {
        let name = format_ident!("VALUE_{index}");
        let literal = scalar_literal(value);
        quote! { pub const #name: Self = Self(#literal); }
    });
    let matches = values
        .iter()
        .map(scalar_literal)
        .map(|literal| quote! { #literal => Ok(Self(value)), });
    let serialize = scalar_serialize(base);
    quote! {
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct #ident(pub #ty);

        impl #ident {
            #(#constants)*
        }

        impl TryFrom<#ty> for #ident {
            type Error = &'static str;

            fn try_from(value: #ty) -> Result<Self, Self::Error> {
                match value { #(#matches)* _ => Err("value is not a member of the declared enum") }
            }
        }

        impl<'de> serde::Deserialize<'de> for #ident {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where D: serde::Deserializer<'de> {
                let value = <#ty as serde::Deserialize>::deserialize(deserializer)?;
                Self::try_from(value).map_err(serde::de::Error::custom)
            }
        }

        impl serde::Serialize for #ident {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where S: serde::Serializer {
                #serialize
            }
        }
    }
}

pub(super) fn typed_enum_tokens(
    ident: &syn::Ident,
    base: &Schema,
    values: &[serde_json::Value],
) -> TokenStream {
    render_typed_enum(ident, base, values)
}

fn scalar_literal(value: &serde_json::Value) -> TokenStream {
    let text = value.to_string();
    syn::parse_str(&text).unwrap_or_else(|error| panic!("typed enum value is valid Rust: {error}"))
}

fn scalar_serialize(base: &Schema) -> TokenStream {
    match base {
        Schema::Integer { .. } => quote! { serde::Serializer::serialize_i64(serializer, self.0) },
        Schema::Number { .. } => quote! { serde::Serializer::serialize_f64(serializer, self.0) },
        Schema::Boolean => quote! { serde::Serializer::serialize_bool(serializer, self.0) },
        _ => quote! { serde::Serializer::serialize_str(serializer, &self.0) },
    }
}

fn render_union(ident: &syn::Ident, variants: &[Schema]) -> TokenStream {
    let variants = variants.iter().enumerate().map(|(index, variant)| {
        let (variant_ident, type_ident) = match variant {
            Schema::Reference(reference) => (
                format_ident!("{}", rust_type_name(reference)),
                schema_tokens(variant),
            ),
            _ => (format_ident!("Variant{index}"), schema_tokens(variant)),
        };
        quote! { #variant_ident(#type_ident), }
    });
    quote! {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
        #[serde(untagged)]
        pub enum #ident { #(#variants)* }
    }
}

fn render_object(ident: &syn::Ident, schema: &Schema, spec: &ApiIr) -> TokenStream {
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
        Schema::Any => quote! { serde_json::Value },
        Schema::String {
            format: Some(format),
        } if format == "binary" => quote! { Vec<u8> },
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
        Schema::TypedEnum { base, .. } => schema_tokens(base),
        Schema::Const { base, .. } => schema_tokens(base),
    }
}

fn object_shape(
    schema: &Schema,
    spec: &ApiIr,
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
