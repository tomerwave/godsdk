#[path = "python/errors_render.rs"]
mod errors_render;
#[path = "python/models_render.rs"]
mod models_render;
#[path = "python/operations_render.rs"]
mod operations_render;

use super::code_writer::CodeWriter;
use crate::{ApiIr, Operation, Schema, rust_identifier};
use errors_render::{python_error_contract_lines, python_error_file_lines};
use models_render::render_models;
use operations_render::{client_method, native_method};

pub(crate) fn render_python_files(spec: &ApiIr) -> Vec<(String, String)> {
    let package = package_name(spec);
    vec![
        (
            "sdk/python/pyproject.toml".to_string(),
            pyproject(spec, &package),
        ),
        (format!("sdk/python/{package}/__init__.py"), init_file(spec)),
        (
            format!("sdk/python/{package}/models.py"),
            render_models(spec),
        ),
        (format!("sdk/python/{package}/errors.py"), errors(spec)),
        (format!("sdk/python/{package}/client.py"), client(spec)),
        (
            "sdk/python/native/Cargo.toml".to_string(),
            native_cargo(spec),
        ),
        (
            "sdk/python/native/src/lib.rs".to_string(),
            native_rust(spec),
        ),
        (
            "sdk/python/tests/test_models.py".to_string(),
            model_test(spec, &package),
        ),
        ("sdk/python/README.md".to_string(), python_readme(spec)),
    ]
}

fn pyproject(spec: &ApiIr, package: &str) -> String {
    let project = slug(&spec.title);
    CodeWriter::from_lines([
        "[build-system]".to_string(),
        "requires = [\"maturin>=1.8,<2\"]".to_string(),
        "build-backend = \"maturin\"".to_string(),
        String::new(),
        "[project]".to_string(),
        format!("name = \"{project}-sdk\""),
        "version = \"0.1.0\"".to_string(),
        "dependencies = [\"pydantic>=2.0,<3\"]".to_string(),
        String::new(),
        "[tool.maturin]".to_string(),
        "manifest-path = \"native/Cargo.toml\"".to_string(),
        format!("module-name = \"{package}._native\""),
        "python-source = \".\"".to_string(),
    ])
}

fn init_file(spec: &ApiIr) -> String {
    let mut exports = vec!["Client".to_string(), "SdkHttpError".to_string()];
    exports.extend(
        spec.operations
            .iter()
            .filter(|operation| has_error_responses(operation))
            .map(|operation| format!("{}Error", type_identifier(&operation.operation_id))),
    );
    exports.extend(spec.schemas.keys().cloned());
    exports.extend(
        spec.operations
            .iter()
            .filter(|operation| inline_success_schema(operation).is_some())
            .map(operation_response_name),
    );
    let error_count = 1 + spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
        .count();
    let error_imports = exports
        .iter()
        .skip(1)
        .take(error_count)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let imports = exports
        .iter()
        .skip(1 + error_count)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    CodeWriter::from_lines([
        "from .client import Client".to_string(),
        format!("from .errors import {error_imports}"),
        format!("from .models import {imports}"),
        String::new(),
        format!("__all__ = {exports:?}"),
    ])
}

fn has_error_responses(operation: &Operation) -> bool {
    operation
        .responses
        .iter()
        .any(|response| !response.status.starts_with('2'))
}

fn errors(spec: &ApiIr) -> String {
    let imports = spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
        .flat_map(|operation| operation.responses.iter())
        .filter(|response| !response.status.starts_with('2'))
        .filter_map(|response| response.schema.as_ref().and_then(schema_model_name))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|name| format!("from .models import {name}"))
        .collect::<Vec<_>>();
    let contracts = spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
        .map(error_contract)
        .collect::<String>();
    CodeWriter::from_lines(python_error_file_lines(&imports, &contracts))
}

fn error_contract(operation: &Operation) -> String {
    let operation_name = type_identifier(&operation.operation_id);
    let name = format!("{operation_name}Error");
    let subclasses = operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
        .filter_map(|response| {
            let status = response.status.parse::<u16>().ok()?;
            let body_type = response
                .schema
                .as_ref()
                .and_then(schema_model_name)
                .unwrap_or_else(|| "JsonValue".to_string());
            Some(vec![
                format!("class {operation_name}Status{status}Error({name}):"),
                format!("    body: {body_type}"),
                String::new(),
            ])
        })
        .flatten()
        .collect::<Vec<_>>();
    let arms = operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
        .filter_map(|response| {
            let status = response.status.parse::<u16>().ok()?;
            let constructor = response
                .schema
                .as_ref()
                .and_then(schema_model_name)
                .map_or_else(
                    || format!("{operation_name}Status{status}Error(status, body)"),
                    |model| {
                        format!("{operation_name}Status{status}Error(status, {model}.model_validate(body))")
                    },
                );
            Some(vec![
                format!("        if status == {status}:"),
                format!("            return {constructor}"),
            ])
        })
        .flatten()
        .collect::<Vec<_>>();
    CodeWriter::from_lines(python_error_contract_lines(&name, &arms, &subclasses))
}

fn client(spec: &ApiIr) -> String {
    let methods = spec
        .operations
        .iter()
        .map(client_method)
        .collect::<String>();
    let mut lines = vec![
        "from __future__ import annotations".to_string(),
        String::new(),
        "import json".to_string(),
        "from typing import cast".to_string(),
        "from . import _native".to_string(),
        "from .errors import *".to_string(),
        "from .models import *".to_string(),
        String::new(),
        "class Client:".to_string(),
        "    def __init__(self, base_url: str) -> None:".to_string(),
        "        self._native = _native.NativeClient(base_url)".to_string(),
        String::new(),
    ];
    lines.extend(methods.trim_end().lines().map(str::to_string));
    CodeWriter::from_lines(lines)
}

fn native_cargo(spec: &ApiIr) -> String {
    let package = slug(&spec.title);
    let crate_name = package.replace('-', "_");
    CodeWriter::from_lines([
        "[package]".to_string(),
        ["name = \"", &package, "_python_native\""].concat(),
        "version = \"0.1.0\"".to_string(),
        "edition = \"2024\"".to_string(),
        "rust-version = \"1.97\"".to_string(),
        String::new(),
        "[lib]".to_string(),
        "name = \"_native\"".to_string(),
        "crate-type = [\"cdylib\"]".to_string(),
        String::new(),
        "[dependencies]".to_string(),
        "pyo3 = { version = \"0.24\", features = [\"abi3-py38\", \"extension-module\"] }"
            .to_string(),
        "serde_json = \"1\"".to_string(),
        "tokio = { version = \"1\", features = [\"rt\", \"time\"] }".to_string(),
        [
            crate_name,
            "_sdk = { package = \"".to_string(),
            package,
            "-sdk\", path = \"../../rust\" }".to_string(),
        ]
        .concat(),
    ])
}

fn native_rust(spec: &ApiIr) -> String {
    let crate_name = package_name(spec);
    let rust_crate_name = [crate_name.as_str(), "_sdk"].concat();
    let methods = spec
        .operations
        .iter()
        .map(|operation| native_method(operation, &rust_crate_name))
        .collect::<String>();
    let helpers = spec
        .operations
        .iter()
        .map(native_result_helper)
        .collect::<String>();
    let error_imports = spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
        .map(|operation| format!("{}Error", type_identifier(&operation.operation_id)))
        .collect::<Vec<_>>();
    let imports = native_imports(spec, &crate_name, &error_imports);
    CodeWriter::from_parts([native_header(&imports), methods, native_footer(&helpers)])
}

fn native_imports(spec: &ApiIr, crate_name: &str, errors: &[String]) -> String {
    let mut imports = String::from("use ");
    imports.push_str(crate_name);
    imports.push_str("_sdk::{Client as RustClient");
    if spec
        .operations
        .iter()
        .any(|operation| !has_error_responses(operation))
    {
        imports.push_str(", SdkError");
    }
    if !errors.is_empty() {
        imports.push_str(", ");
        imports.push_str(&errors.join(", "));
    }
    imports.push_str("};");
    imports
}

fn native_header(imports: &str) -> String {
    CodeWriter::from_lines([
        "use pyo3::prelude::*;".to_string(),
        imports.to_string(),
        String::new(),
        "#[pyclass]".to_string(),
        "struct NativeClient {".to_string(),
        "    inner: RustClient,".to_string(),
        "}".to_string(),
        String::new(),
        "#[pymethods]".to_string(),
        "impl NativeClient {".to_string(),
        "    #[new]".to_string(),
        "    fn new(base_url: String) -> PyResult<Self> {".to_string(),
        "        let inner = RustClient::builder(base_url).build().map_err(to_python_error)?;"
            .to_string(),
        "        Ok(Self { inner })".to_string(),
        "    }".to_string(),
    ])
}

fn native_footer(helpers: &str) -> String {
    CodeWriter::from_lines([
        "}".to_string(),
        String::new(),
        "fn encode_success_value(value: serde_json::Value) -> PyResult<String> {".to_string(),
        "    serde_json::to_string(&serde_json::json!({\"ok\": true, \"value\": value})).map_err(to_python_error)".to_string(),
        "}".to_string(),
        String::new(),
        "fn encode_http_error(status: u16, body: serde_json::Value) -> PyResult<String> {".to_string(),
        "    serde_json::to_string(&serde_json::json!({\"ok\": false, \"status\": status, \"body\": body})).map_err(to_python_error)".to_string(),
        "}".to_string(),
        String::new(),
        helpers.to_string(),
        "#[pymodule]".to_string(),
        "fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {".to_string(),
        "    m.add_class::<NativeClient>()?;".to_string(),
        "    Ok(())".to_string(),
        "}".to_string(),
        String::new(),
        "fn to_python_error(error: impl std::fmt::Display) -> PyErr {".to_string(),
        "    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(error.to_string())".to_string(),
        "}".to_string(),
    ])
}

fn native_result_helper(operation: &Operation) -> String {
    if !has_error_responses(operation) {
        return String::new();
    }
    let method = rust_identifier(&operation.operation_id);
    let error_type = format!("{}Error", type_identifier(&operation.operation_id));
    let arms = native_result_arms(operation, &error_type);
    CodeWriter::from_parts([
        "fn encode_".to_string(),
        method,
        "_error(error: ".to_string(),
        error_type.clone(),
        ") -> PyResult<String> {\n    match error {\n        ".to_string(),
        error_type.clone(),
        "::Unexpected { status, body } => encode_http_error(status, serde_json::Value::String(body)),\n        ".to_string(),
        error_type,
        "::Transport(error) => Err(to_python_error(error)),\n".to_string(),
        arms,
        "    }\n}\n\n".to_string(),
    ])
}

fn native_result_arms(operation: &Operation, error_type: &str) -> String {
    let mut writer = CodeWriter::default();
    for response in operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
    {
        if let Some(arm) = native_result_arm(response, error_type) {
            writer.push(&arm);
        }
    }
    writer.finish()
}

fn native_result_arm(response: &crate::Response, error_type: &str) -> Option<String> {
    let status = response.status.parse::<u16>().ok()?;
    let status = status.to_string();
    let body = if response.schema.is_some() {
        [
            "(value) => encode_http_error(",
            &status,
            ", serde_json::to_value(value).map_err(to_python_error)?),\n",
        ]
        .concat()
    } else {
        [
            " => encode_http_error(",
            &status,
            ", serde_json::Value::Null),\n",
        ]
        .concat()
    };
    Some(["        ", error_type, "::Status", &status, &body].concat())
}

fn model_test(spec: &ApiIr, package: &str) -> String {
    let name = spec
        .schemas
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| "BaseModel".to_string());
    CodeWriter::from_lines([
        ["from ", package, ".models import ", &name].concat(),
        String::new(),
        "def test_models_reject_unknown_fields() -> None:".to_string(),
        "    try:".to_string(),
        ["        ", &name, "(unexpected=\"value\")"].concat(),
        "    except Exception as error:".to_string(),
        "        assert \"unexpected\" in str(error).lower()".to_string(),
        "        return".to_string(),
        "    raise AssertionError(\"generated model accepted an unknown field\")".to_string(),
    ])
}

fn python_readme(spec: &ApiIr) -> String {
    CodeWriter::from_lines([
        ["# ", &spec.title, " Python SDK"].concat(),
        String::new(),
        "Build with `maturin develop`.".to_string(),
    ])
}

fn schema_model_name(schema: &Schema) -> Option<String> {
    match schema {
        Schema::Reference(name) => Some(name.clone()),
        _ => None,
    }
}

fn inline_success_schema(operation: &Operation) -> Option<&Schema> {
    operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .and_then(|response| response.schema.as_ref())
        .filter(|schema| schema_model_name(schema).is_none())
}

fn operation_response_name(operation: &Operation) -> String {
    format!("{}Response", type_identifier(&operation.operation_id))
}

fn package_name(spec: &ApiIr) -> String {
    slug(&spec.title).replace('-', "_")
}

fn type_identifier(value: &str) -> String {
    python_identifier(&super::rust_identifier(value))
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

fn python_identifier(value: &str) -> String {
    let mut output = value
        .split(['-', '_', ' ', '.'])
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("_");
    if output.is_empty() {
        output.push_str("value");
    }
    if output
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        output.insert(0, '_');
    }
    output
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
