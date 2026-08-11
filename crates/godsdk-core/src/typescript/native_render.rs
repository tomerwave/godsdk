use super::operations_render::render_native_operation;
use super::{has_error_responses, type_identifier};
use crate::ApiIr;
use crate::code_writer::CodeWriter;

use super::identifiers::slug;

pub(super) fn render_native_cargo(spec: &ApiIr) -> String {
    let crate_name = rust_crate_name(spec);
    let package = slug(&spec.title);
    CodeWriter::from_lines([
        "[package]".to_string(),
        ["name = \"", &package, "-typescript-native\""].concat(),
        "version = \"0.1.0\"".to_string(),
        "edition = \"2024\"".to_string(),
        "rust-version = \"1.97\"".to_string(),
        String::new(),
        "[lib]".to_string(),
        "crate-type = [\"cdylib\"]".to_string(),
        String::new(),
        "[dependencies]".to_string(),
        "napi = { version = \"3.12\", features = [\"napi9\", \"tokio_rt\", \"serde-json\"] }"
            .to_string(),
        "napi-derive = \"3.6\"".to_string(),
        "serde_json = \"1\"".to_string(),
        [
            crate_name,
            " = { package = \"".to_string(),
            package,
            "-sdk\", path = \"../../rust\" }".to_string(),
        ]
        .concat(),
    ])
}

pub(super) fn render_native_package() -> String {
    "{\n  \"type\": \"commonjs\"\n}\n".to_string()
}

pub(super) fn render_native_rust(spec: &ApiIr) -> String {
    let crate_name = rust_crate_name(spec);
    let methods = spec
        .operations
        .iter()
        .map(|operation| render_native_operation(operation, &crate_name))
        .collect::<String>();
    CodeWriter::from_parts([
        native_rust_header(spec, &crate_name),
        methods,
        native_rust_footer(),
    ])
}

fn native_rust_header(spec: &ApiIr, crate_name: &str) -> String {
    let mut imports = String::from("use ");
    imports.push_str(crate_name);
    imports.push_str("::{Client as RustClient");
    if spec
        .operations
        .iter()
        .any(|operation| !has_error_responses(operation))
    {
        imports.push_str(", SdkError");
    }
    for operation in spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
    {
        imports.push_str(", ");
        imports.push_str(&type_identifier(&operation.operation_id));
        imports.push_str("Error");
    }
    imports.push_str("};");
    CodeWriter::from_lines([
        "use napi::bindgen_prelude::*;",
        "use napi_derive::napi;",
        imports.as_str(),
        "",
        "#[napi]",
        "pub struct NativeClient {",
        "    inner: RustClient,",
        "}",
        "",
        "#[napi]",
        "impl NativeClient {",
        "    #[napi(constructor)]",
        "    pub fn new(base_url: String) -> Result<Self> {",
        "        let inner = RustClient::builder(base_url).build().map_err(to_napi_error)?;",
        "        Ok(Self { inner })",
        "    }",
    ])
}

fn native_rust_footer() -> String {
    CodeWriter::from_lines([
        "}",
        "",
        "fn to_napi_error(error: impl std::fmt::Display) -> Error {",
        "    Error::from_reason(error.to_string())",
        "}",
    ])
}

fn rust_crate_name(spec: &ApiIr) -> String {
    slug(&spec.title).replace('-', "_")
}
