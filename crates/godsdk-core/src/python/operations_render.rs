use crate::{Operation, ParameterLocation, Schema, rust_identifier};

use super::{
    has_error_responses, operation_response_name, python_identifier, schema_model_name,
    type_identifier,
};

pub(super) fn client_method(operation: &Operation) -> String {
    let method = python_identifier(&rust_identifier(&operation.operation_id));
    let mut parameters = Vec::new();
    if let Some(body) = operation.request_body_details.as_ref() {
        let ty = body
            .schema
            .as_ref()
            .map(python_schema_type)
            .unwrap_or_else(|| "JsonValue".to_string());
        parameters.push(if body.required {
            format!("request_body: {ty}")
        } else {
            format!("request_body: {ty} | None = None")
        });
    }
    parameters.extend(ordered_parameters(operation).into_iter().map(|parameter| {
        let name = python_identifier(&parameter.name);
        let ty = if parameter.location == ParameterLocation::Path {
            "str".to_string()
        } else {
            python_schema_type(&parameter.schema)
        };
        if parameter.required {
            format!("{name}: {ty}")
        } else {
            format!("{name}: {ty} | None = None")
        }
    }));
    let parameters = parameters.join(", ");
    let arguments = native_arguments(operation);
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
    let body = client_method_body(operation, &method, &arguments, &return_type);
    format!("    def {method}(self, {parameters}) -> {return_type}:\n{body}\n")
}

fn client_method_body(
    operation: &Operation,
    method: &str,
    arguments: &str,
    return_type: &str,
) -> String {
    if return_type == "None" {
        return format!(
            "        raw = cast(dict[str, JsonValue], json.loads(self._native.{method}({arguments})))\n        if raw[\"ok\"] is not True:\n            raise SdkHttpError(int(raw[\"status\"]), raw[\"body\"])\n"
        );
    }
    let error = has_error_responses(operation)
        .then(|| format!("{}Error", type_identifier(&operation.operation_id)));
    let error_handling = error.map_or_else(
        || "            raise SdkHttpError(int(raw[\"status\"]), raw[\"body\"])".to_string(),
        |error| {
            format!("            raise {error}.from_native(int(raw[\"status\"]), raw[\"body\"])")
        },
    );
    format!(
        "        raw = cast(dict[str, JsonValue], json.loads(self._native.{method}({arguments})))\n        if raw[\"ok\"] is not True:\n{error_handling}\n        return {return_type}.model_validate(raw[\"value\"])\n"
    )
}

fn native_arguments(operation: &Operation) -> String {
    let mut arguments = Vec::new();
    if let Some(body) = operation.request_body_details.as_ref() {
        arguments.push(if body.required {
            "request_body.model_dump_json()".to_string()
        } else {
            "None if request_body is None else request_body.model_dump_json()".to_string()
        });
    }
    arguments.extend(ordered_parameters(operation).into_iter().map(|parameter| {
        let name = python_identifier(&parameter.name);
        if parameter.location == ParameterLocation::Path {
            name
        } else if parameter.required {
            format!("json.dumps({name})")
        } else {
            format!("None if {name} is None else json.dumps({name})")
        }
    }));
    arguments.join(", ")
}

fn ordered_parameters(operation: &Operation) -> Vec<&crate::Parameter> {
    operation
        .parameters
        .iter()
        .filter(|parameter| parameter.required)
        .chain(
            operation
                .parameters
                .iter()
                .filter(|parameter| !parameter.required),
        )
        .collect()
}

fn python_schema_type(schema: &Schema) -> String {
    match schema {
        Schema::Reference(name) => type_identifier(name),
        Schema::String { .. } => "str".to_string(),
        Schema::Integer { .. } => "int".to_string(),
        Schema::Number { .. } => "float".to_string(),
        Schema::Boolean => "bool".to_string(),
        Schema::Array(item) => format!("list[{}]", python_schema_type(item)),
        _ => "JsonValue".to_string(),
    }
}

pub(super) fn native_method(operation: &Operation, crate_name: &str) -> String {
    let method = rust_identifier(&operation.operation_id);
    let (parameters, conversions, arguments) = native_inputs(operation, crate_name);
    let body = native_call_body(operation, &method, &arguments);
    format!(
        "    fn {method}(&self{parameters}) -> PyResult<String> {{\n        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(to_python_error)?;\n{conversions}{body}\n    }}\n\n"
    )
}

fn native_inputs(operation: &Operation, crate_name: &str) -> (String, String, String) {
    let inputs = ordered_parameters(operation)
        .iter()
        .map(|parameter| native_parameter(parameter, crate_name))
        .collect::<Vec<_>>();
    let mut parameters = inputs
        .iter()
        .map(|input| input.0.clone())
        .collect::<Vec<_>>();
    let mut conversions = inputs
        .iter()
        .map(|input| input.1.clone())
        .collect::<Vec<_>>();
    let mut fields = operation
        .parameters
        .iter()
        .map(|parameter| {
            let name = rust_identifier(&parameter.name);
            format!("{name}: {name}")
        })
        .collect::<Vec<_>>();
    if let Some(input) = native_body_input(operation, crate_name) {
        conversions.push(input.0);
        fields.push(format!("request_body: {}", input.1));
    }
    if let Some(body) = operation.request_body_details.as_ref() {
        parameters.insert(
            0,
            if body.required {
                ", request_body: String".to_string()
            } else {
                ", request_body: Option<String>".to_string()
            },
        );
    }
    (
        parameters.concat(),
        conversions.concat(),
        format!(
            "{crate_name}::{}Request {{ {} }}",
            type_identifier(&operation.operation_id),
            fields.join(", "),
        ),
    )
}

fn native_parameter(parameter: &crate::Parameter, crate_name: &str) -> (String, String, String) {
    let name = rust_identifier(&parameter.name);
    if parameter.location == ParameterLocation::Path {
        return (format!(", {name}: String"), String::new(), name);
    }
    let ty = native_rust_schema_type(&parameter.schema, crate_name);
    let signature = if parameter.required {
        format!(", {name}: String")
    } else {
        format!(", {name}: Option<String>")
    };
    let conversion = if parameter.required {
        format!(
            "        let {name}: {ty} = serde_json::from_str(&{name}).map_err(to_python_error)?;\n"
        )
    } else {
        format!(
            "        let {name}: Option<{ty}> = {name}.map(|value| serde_json::from_str(&value)).transpose().map_err(to_python_error)?;\n"
        )
    };
    (signature, conversion, name)
}

fn native_body_input(operation: &Operation, crate_name: &str) -> Option<(String, String)> {
    let body = operation.request_body_details.as_ref()?;
    let schema = body.schema.as_ref()?;
    let ty = native_rust_schema_type(schema, crate_name);
    if body.required {
        Some((
            format!(
                "        let request_body: {ty} = serde_json::from_str(&request_body).map_err(to_python_error)?;\n"
            ),
            "request_body".to_string(),
        ))
    } else {
        Some((
            format!(
                "        let request_body: Option<{ty}> = request_body.map(|value| serde_json::from_str(&value)).transpose().map_err(to_python_error)?;\n"
            ),
            "request_body".to_string(),
        ))
    }
}

fn native_call_body(operation: &Operation, method: &str, arguments: &str) -> String {
    if has_error_responses(operation) {
        format!(
            "        match runtime.block_on(self.inner.{method}({arguments})) {{\n            Ok(value) => encode_success_value(serde_json::to_value(value).map_err(to_python_error)?),\n            Err(error) => encode_{method}_error(error),\n        }}"
        )
    } else {
        format!(
            "        match runtime.block_on(self.inner.{method}({arguments})) {{\n            Ok(value) => encode_success_value(serde_json::to_value(value).map_err(to_python_error)?),\n            Err(SdkError::Http {{ status, body }}) => encode_http_error(status, serde_json::Value::String(body)),\n            Err(error) => Err(to_python_error(error)),\n        }}"
        )
    }
}

fn native_rust_schema_type(schema: &Schema, crate_name: &str) -> String {
    match schema {
        Schema::Reference(name) => format!("{crate_name}::{}", type_identifier(name)),
        Schema::String { .. } => "String".to_string(),
        Schema::Integer { .. } => "i64".to_string(),
        Schema::Number { .. } => "f64".to_string(),
        Schema::Boolean => "bool".to_string(),
        Schema::Array(item) => format!("Vec<{}>", native_rust_schema_type(item, crate_name)),
        _ => "serde_json::Value".to_string(),
    }
}
