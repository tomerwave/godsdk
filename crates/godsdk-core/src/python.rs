use crate::{ApiIr, Operation, ParameterLocation, Schema, rust_identifier};

pub(crate) fn render_python_files(spec: &ApiIr) -> Vec<(String, String)> {
    let package = package_name(spec);
    vec![
        (
            "sdk/python/pyproject.toml".to_string(),
            pyproject(spec, &package),
        ),
        (format!("sdk/python/{package}/__init__.py"), init_file(spec)),
        (format!("sdk/python/{package}/models.py"), models(spec)),
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
    format!(
        "[build-system]\nrequires = [\"maturin>=1.8,<2\"]\nbuild-backend = \"maturin\"\n\n[project]\nname = \"{}-sdk\"\nversion = \"0.1.0\"\ndependencies = [\"pydantic>=2.0,<3\"]\n\n[tool.maturin]\nmanifest-path = \"native/Cargo.toml\"\nmodule-name = \"{}._native\"\npython-source = \".\"\n",
        slug(&spec.title),
        package,
    )
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
    format!(
        "from .client import Client\nfrom .errors import {error_imports}\nfrom .models import {imports}\n\n__all__ = {exports:?}\n"
    )
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
        .map(|name| format!("from .models import {name}\n"))
        .collect::<String>();
    let contracts = spec
        .operations
        .iter()
        .filter(|operation| has_error_responses(operation))
        .map(error_contract)
        .collect::<String>();
    format!(
        "from __future__ import annotations\n\nfrom typing import TypeAlias\n\n{imports}\nJsonValue: TypeAlias = None | bool | int | float | str | list[\"JsonValue\"] | dict[str, \"JsonValue\"]\n\nclass SdkHttpError(Exception):\n    status: int\n    body: JsonValue\n\n    def __init__(self, status: int, body: JsonValue) -> None:\n        super().__init__(f\"API returned HTTP {{status}}\")\n        self.status = status\n        self.body = body\n\n{contracts}"
    )
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
            Some(format!(
                "class {operation_name}Status{status}Error({name}):\n    body: {body_type}\n\n"
            ))
        })
        .collect::<String>();
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
            Some(format!(
                "        if status == {status}:\n            return {constructor}\n"
            ))
        })
        .collect::<String>();
    format!(
        "class {name}(SdkHttpError):\n    @classmethod\n    def from_native(cls, status: int, body: JsonValue) -> {name}:\n{arms}        return cls(status, body)\n\n{subclasses}"
    )
}

fn models(spec: &ApiIr) -> String {
    let mut output = String::from(
        "from __future__ import annotations\n\nfrom enum import Enum\nfrom pydantic import BaseModel, ConfigDict\n\n",
    );
    for (name, schema) in &spec.schemas {
        output.push_str(&model(name, schema, spec));
    }
    for operation in &spec.operations {
        if let Some(schema) = inline_success_schema(operation) {
            output.push_str(&model(&operation_response_name(operation), schema, spec));
        }
    }
    output
}

fn model(name: &str, schema: &Schema, spec: &ApiIr) -> String {
    if let Schema::Enum(values) = schema {
        let variants = values
            .iter()
            .map(|value| format!("    {} = {value:?}\n", enum_identifier(value)))
            .collect::<String>();
        return format!("class {name}(str, Enum):\n{variants}\n");
    }
    let fields = object_fields(schema, spec)
        .into_iter()
        .map(|(property, property_schema, required)| {
            let annotation = python_type(&property_schema);
            if required {
                format!("    {}: {annotation}\n", python_identifier(&property))
            } else {
                format!(
                    "    {}: {annotation} | None = None\n",
                    python_identifier(&property)
                )
            }
        })
        .collect::<String>();
    format!("class {name}(BaseModel):\n    model_config = ConfigDict(extra=\"forbid\")\n{fields}\n")
}

fn client(spec: &ApiIr) -> String {
    let methods = spec
        .operations
        .iter()
        .map(client_method)
        .collect::<String>();
    format!(
        "from __future__ import annotations\n\nimport json\nfrom typing import cast\nfrom . import _native\nfrom .errors import *\nfrom .models import *\n\nclass Client:\n    def __init__(self, base_url: str) -> None:\n        self._native = _native.NativeClient(base_url)\n\n{methods}"
    )
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

fn object_fields(schema: &Schema, spec: &ApiIr) -> Vec<(String, Schema, bool)> {
    match schema {
        Schema::Object {
            properties,
            required,
            ..
        } => properties
            .iter()
            .map(|(name, schema)| (name.clone(), schema.clone(), required.contains(name)))
            .collect(),
        Schema::AllOf(parts) => parts
            .iter()
            .flat_map(|part| object_fields(part, spec))
            .collect(),
        Schema::Reference(name) => spec
            .schemas
            .get(name)
            .map(|schema| object_fields(schema, spec))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn python_type(schema: &Schema) -> String {
    match schema {
        Schema::String { .. } => "str".to_string(),
        Schema::Integer { .. } => "int".to_string(),
        Schema::Number { .. } => "float".to_string(),
        Schema::Boolean => "bool".to_string(),
        Schema::Null => "None".to_string(),
        Schema::Array(item) => format!("list[{}]", python_type(item)),
        Schema::Object { .. } => "dict[str, object]".to_string(),
        Schema::Enum(_) | Schema::Reference(_) => {
            schema_model_name(schema).unwrap_or_else(|| "str".to_string())
        }
        Schema::Nullable(inner) => format!("{} | None", python_type(inner)),
        Schema::OneOf(values) | Schema::AnyOf(values) | Schema::AllOf(values) => values
            .iter()
            .map(python_type)
            .collect::<Vec<_>>()
            .join(" | "),
    }
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

fn enum_identifier(value: &str) -> String {
    type_identifier(value).to_ascii_uppercase()
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
