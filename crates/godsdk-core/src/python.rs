#[path = "python/errors_render.rs"]
mod errors_render;
#[path = "python/models_render.rs"]
mod models_render;

use super::code_writer::CodeWriter;
use crate::{ApiIr, Operation, ParameterLocation, Schema, rust_identifier};
use errors_render::{python_error_contract_lines, python_error_file_lines};
use models_render::render_models;

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
        (
            "sdk/python/README.md".to_string(),
            format!(
                "# {} Python SDK\n\nBuild with `maturin develop`.\n",
                spec.title
            ),
        ),
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

fn client_method(operation: &Operation) -> String {
    let method = python_identifier(&super::rust_identifier(&operation.operation_id));
    let parameters = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|parameter| format!("{}: str", python_identifier(&parameter.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|parameter| python_identifier(&parameter.name))
        .collect::<Vec<_>>()
        .join(", ");
    let response = operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .and_then(|response| response.schema.as_ref());
    let return_type = response
        .and_then(|schema| {
            schema_model_name(schema).or_else(|| Some(operation_response_name(operation)))
        })
        .unwrap_or_else(|| "None".to_string());
    let body = if return_type == "None" {
        format!(
            "        raw = cast(dict[str, JsonValue], json.loads(self._native.{method}({arguments})))\n        if raw[\"ok\"] is not True:\n            raise SdkHttpError(int(raw[\"status\"]), raw[\"body\"])\n"
        )
    } else {
        let error = has_error_responses(operation)
            .then(|| format!("{}Error", type_identifier(&operation.operation_id)));
        let error_handling = error.map_or_else(
            || "            raise SdkHttpError(int(raw[\"status\"]), raw[\"body\"])".to_string(),
            |error| {
                format!(
                    "            raise {error}.from_native(int(raw[\"status\"]), raw[\"body\"])"
                )
            },
        );
        format!(
            "        raw = cast(dict[str, JsonValue], json.loads(self._native.{method}({arguments})))\n        if raw[\"ok\"] is not True:\n{error_handling}\n        return {return_type}.model_validate(raw[\"value\"])\n"
        )
    };
    format!("    def {method}(self, {parameters}) -> {return_type}:\n{body}\n")
}

fn native_cargo(spec: &ApiIr) -> String {
    format!(
        "[package]\nname = \"{}_python_native\"\nversion = \"0.1.0\"\nedition = \"2024\"\nrust-version = \"1.97\"\n\n[lib]\nname = \"_native\"\ncrate-type = [\"cdylib\"]\n\n[dependencies]\npyo3 = {{ version = \"0.24\", features = [\"abi3-py38\", \"extension-module\"] }}\nserde_json = \"1\"\ntokio = {{ version = \"1\", features = [\"rt\", \"time\"] }}\n{}_sdk = {{ package = \"{}-sdk\", path = \"../../rust\" }}\n",
        slug(&spec.title),
        slug(&spec.title).replace('-', "_"),
        slug(&spec.title),
    )
}

fn native_rust(spec: &ApiIr) -> String {
    let methods = spec
        .operations
        .iter()
        .map(native_method)
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
    let error_imports = if error_imports.is_empty() {
        String::new()
    } else {
        format!(", {}", error_imports.join(", "))
    };
    format!(
        "use pyo3::prelude::*;\nuse {}_sdk::{{Client as RustClient, SdkError{error_imports}}};\n\n#[pyclass]\nstruct NativeClient {{\n    inner: RustClient,\n}}\n\n#[pymethods]\nimpl NativeClient {{\n    #[new]\n    fn new(base_url: String) -> PyResult<Self> {{\n        let inner = RustClient::builder(base_url).build().map_err(to_python_error)?;\n        Ok(Self {{ inner }})\n    }}\n{methods}}}\n\nfn encode_success_value(value: serde_json::Value) -> PyResult<String> {{\n    serde_json::to_string(&serde_json::json!({{\"ok\": true, \"value\": value}})).map_err(to_python_error)\n}}\n\nfn encode_http_error(status: u16, body: serde_json::Value) -> PyResult<String> {{\n    serde_json::to_string(&serde_json::json!({{\"ok\": false, \"status\": status, \"body\": body}})).map_err(to_python_error)\n}}\n\n{helpers}#[pymodule]\nfn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {{\n    m.add_class::<NativeClient>()?;\n    Ok(())\n}}\n\nfn to_python_error(error: impl std::fmt::Display) -> PyErr {{\n    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(error.to_string())\n}}\n",
        slug(&spec.title).replace('-', "_"),
    )
}

fn native_method(operation: &Operation) -> String {
    let method = rust_identifier(&operation.operation_id);
    let parameters = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|parameter| format!(", {}: String", rust_identifier(&parameter.name)))
        .collect::<String>();
    let arguments = operation
        .parameters
        .iter()
        .filter(|parameter| parameter.location == ParameterLocation::Path)
        .map(|parameter| format!("&{}", rust_identifier(&parameter.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let body = if has_error_responses(operation) {
        format!(
            "        match runtime.block_on(self.inner.{method}({arguments})) {{\n            Ok(value) => encode_success_value(serde_json::to_value(value).map_err(to_python_error)?),\n            Err(error) => encode_{method}_error(error),\n        }}"
        )
    } else {
        format!(
            "        match runtime.block_on(self.inner.{method}({arguments})) {{\n            Ok(value) => encode_success_value(serde_json::to_value(value).map_err(to_python_error)?),\n            Err(SdkError::Http {{ status, body }}) => encode_http_error(status, serde_json::Value::String(body)),\n            Err(error) => Err(to_python_error(error)),\n        }}"
        )
    };
    format!(
        "    fn {method}(&self{parameters}) -> PyResult<String> {{\n        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(to_python_error)?;\n{body}\n    }}\n\n"
    )
}

fn native_result_helper(operation: &Operation) -> String {
    if !has_error_responses(operation) {
        return String::new();
    }
    let method = rust_identifier(&operation.operation_id);
    let error_type = format!("{}Error", type_identifier(&operation.operation_id));
    let arms = operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
        .filter_map(|response| {
            let status = response.status.parse::<u16>().ok()?;
            let variant = format!("Status{status}");
            Some(response.schema.as_ref().map_or_else(
                || format!("        {error_type}::{variant} => encode_http_error({status}, serde_json::Value::Null),\n"),
                |_| format!("        {error_type}::{variant}(value) => encode_http_error({status}, serde_json::to_value(value).map_err(to_python_error)?),\n"),
            ))
        })
        .collect::<String>();
    format!(
        "fn encode_{method}_error(error: {error_type}) -> PyResult<String> {{\n    match error {{\n        {error_type}::Unexpected {{ status, body }} => encode_http_error(status, serde_json::Value::String(body)),\n        {error_type}::Transport(error) => Err(to_python_error(error)),\n{arms}    }}\n}}\n\n"
    )
}

fn model_test(spec: &ApiIr, package: &str) -> String {
    let name = spec
        .schemas
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| "BaseModel".to_string());
    format!(
        "from {package}.models import {name}\n\ndef test_models_reject_unknown_fields() -> None:\n    try:\n        {name}(unexpected=\"value\")\n    except Exception as error:\n        assert \"unexpected\" in str(error).lower()\n        return\n    raise AssertionError(\"generated model accepted an unknown field\")\n"
    )
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
