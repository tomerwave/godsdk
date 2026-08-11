use crate::code_writer::CodeWriter;
use crate::rust_ast::{inline_parameter_type_name, inline_request_body_type_name};
use crate::{Operation, ParameterLocation, Schema, rust_identifier};

use super::{
    has_error_responses, operation_response_name, python_identifier, schema_model_name,
    type_identifier,
};

pub(super) fn client_method(operation: &Operation) -> String {
    let (method, parameters, return_type) = client_signature(operation);
    let arguments = native_arguments(operation);
    let body = client_method_body(operation, &method, &arguments, &return_type);
    CodeWriter::from_parts([
        "    def ".to_string(),
        method,
        "(self, ".to_string(),
        parameters,
        ") -> ".to_string(),
        return_type,
        ":\n".to_string(),
        body,
        "\n".to_string(),
    ])
}

fn client_signature(operation: &Operation) -> (String, String, String) {
    let method = python_identifier(&rust_identifier(&operation.operation_id));
    let parameters = python_parameters(operation);
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
    (method, parameters, return_type)
}

fn python_parameters(operation: &Operation) -> String {
    let mut parameters = required_body_parameter(operation)
        .into_iter()
        .collect::<Vec<_>>();
    parameters.extend(parameter_signatures(operation, true));
    parameters.extend(optional_body_parameter(operation));
    parameters.extend(parameter_signatures(operation, false));
    parameters.join(", ")
}

fn body_parameter(operation: &Operation, required: bool) -> Option<String> {
    let body = operation
        .request_body_details
        .as_ref()
        .filter(|body| body.required == required)?;
    let ty = body
        .schema
        .as_ref()
        .map(python_schema_type)
        .unwrap_or_else(|| "JsonValue".to_string());
    Some(if required {
        ["request_body: ", &ty].concat()
    } else {
        ["request_body: ", &ty, " | None = None"].concat()
    })
}

fn required_body_parameter(operation: &Operation) -> Option<String> {
    body_parameter(operation, true)
}

fn optional_body_parameter(operation: &Operation) -> Option<String> {
    body_parameter(operation, false)
}

fn parameter_signatures(operation: &Operation, required: bool) -> Vec<String> {
    ordered_parameters(operation)
        .into_iter()
        .filter(|parameter| parameter.required == required)
        .map(|parameter| {
            let name = python_identifier(&parameter.name);
            let ty = if parameter.location == ParameterLocation::Path {
                "str".to_string()
            } else {
                python_schema_type(&parameter.schema)
            };
            if required {
                [name, ": ".to_string(), ty].concat()
            } else {
                [name, ": ".to_string(), ty, " | None = None".to_string()].concat()
            }
        })
        .collect()
}

fn client_method_body(
    operation: &Operation,
    method: &str,
    arguments: &str,
    return_type: &str,
) -> String {
    if return_type == "None" {
        return CodeWriter::from_parts([
            "        raw = cast(dict[str, JsonValue], json.loads(self._native.".to_string(),
            method.to_string(),
            "(".to_string(),
            arguments.to_string(),
            ")))\n".to_string(),
            "        if raw[\"ok\"] is not True:\n".to_string(),
            "            raise SdkHttpError(int(raw[\"status\"]), raw[\"body\"])\n".to_string(),
        ]);
    }
    let error = has_error_responses(operation).then(|| {
        [
            type_identifier(&operation.operation_id),
            "Error".to_string(),
        ]
        .concat()
    });
    let error_handling = error.map_or_else(
        || "            raise SdkHttpError(int(raw[\"status\"]), raw[\"body\"])".to_string(),
        |error| {
            [
                "            raise ",
                error.as_str(),
                ".from_native(int(raw[\"status\"]), raw[\"body\"])",
            ]
            .concat()
        },
    );
    let response = python_response_expression(operation, return_type);
    CodeWriter::from_parts([
        "        raw = cast(dict[str, JsonValue], json.loads(self._native.".to_string(),
        method.to_string(),
        "(".to_string(),
        arguments.to_string(),
        ")))\n".to_string(),
        "        if raw[\"ok\"] is not True:\n".to_string(),
        error_handling,
        "\n        return ".to_string(),
        response,
        "\n".to_string(),
    ])
}

#[allow(clippy::useless_format)]
fn python_response_expression(operation: &Operation, return_type: &str) -> String {
    let schema = operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .and_then(|response| response.schema.as_ref());
    match schema {
        Some(Schema::String {
            format: Some(format),
        }) if format == "binary" => format!("bytes(raw[\"value\"])"),
        Some(Schema::String { .. })
        | Some(Schema::Integer { .. })
        | Some(Schema::Number { .. })
        | Some(Schema::Boolean)
        | Some(Schema::TypedEnum { .. }) => "raw[\"value\"]".to_string(),
        Some(Schema::Enum(_)) => format!("{return_type}(raw[\"value\"])"),
        _ => format!("{return_type}.model_validate(raw[\"value\"])"),
    }
}

fn native_arguments(operation: &Operation) -> String {
    let mut arguments = required_body_argument(operation)
        .into_iter()
        .collect::<Vec<_>>();
    arguments.extend(parameter_arguments(operation, true));
    arguments.extend(optional_body_argument(operation));
    arguments.extend(parameter_arguments(operation, false));
    arguments.join(", ")
}

fn required_body_argument(operation: &Operation) -> Option<String> {
    operation
        .request_body_details
        .as_ref()
        .filter(|body| body.required)
        .map(|body| body_json_expression(body, "request_body"))
}

fn optional_body_argument(operation: &Operation) -> Option<String> {
    operation
        .request_body_details
        .as_ref()
        .filter(|body| !body.required)
        .map(|body| {
            format!(
                "None if request_body is None else {}",
                body_json_expression(body, "request_body")
            )
        })
}

fn parameter_arguments(operation: &Operation, required: bool) -> Vec<String> {
    ordered_parameters(operation)
        .into_iter()
        .filter(|parameter| parameter.required == required)
        .map(|parameter| {
            let name = python_identifier(&parameter.name);
            if parameter.location == ParameterLocation::Path {
                name
            } else if required {
                ["json.dumps(", &name, ")"].concat()
            } else {
                ["None if ", &name, " is None else json.dumps(", &name, ")"].concat()
            }
        })
        .collect()
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
        Schema::String {
            format: Some(format),
        } if format == "binary" => "bytes".to_string(),
        Schema::String { .. } => "str".to_string(),
        Schema::Integer { .. } => "int".to_string(),
        Schema::Number { .. } => "float".to_string(),
        Schema::Boolean => "bool".to_string(),
        Schema::Array(item) => ["list[", python_schema_type(item).as_str(), "]"].concat(),
        _ => "JsonValue".to_string(),
    }
}

fn body_json_expression(body: &crate::RequestBody, variable: &str) -> String {
    let Some(schema) = body.schema.as_ref() else {
        return format!("json.dumps({variable})");
    };
    if body.content_type == "multipart/form-data" || body.content_type == "application/octet-stream"
    {
        return format!(
            "json.dumps({variable}.model_dump(mode=\"python\") if hasattr({variable}, \"model_dump\") else {variable}, default=lambda value: list(value) if isinstance(value, bytes) else value)"
        );
    }
    if python_schema_type(schema) == "JsonValue" {
        format!("json.dumps({variable})")
    } else if schema_requires_alias(schema) {
        format!("{variable}.model_dump_json(by_alias=True)")
    } else {
        format!("{variable}.model_dump_json()")
    }
}

fn schema_requires_alias(schema: &Schema) -> bool {
    match schema {
        Schema::Object { properties, .. } => properties.keys().any(|name| {
            matches!(
                python_identifier(name).as_str(),
                "and_"
                    | "as_"
                    | "assert_"
                    | "async_"
                    | "await_"
                    | "break_"
                    | "case_"
                    | "class_"
                    | "continue_"
                    | "def_"
                    | "del_"
                    | "elif_"
                    | "else_"
                    | "except_"
                    | "finally_"
                    | "for_"
                    | "from_"
                    | "global_"
                    | "if_"
                    | "import_"
                    | "in_"
                    | "is_"
                    | "lambda_"
                    | "match_"
                    | "none_"
                    | "nonlocal_"
                    | "not_"
                    | "or_"
                    | "pass_"
                    | "raise_"
                    | "return_"
                    | "try_"
                    | "while_"
                    | "with_"
                    | "yield_"
            )
        }),
        Schema::AllOf(parts) => parts.iter().any(schema_requires_alias),
        _ => false,
    }
}

pub(super) fn native_method(operation: &Operation, crate_name: &str) -> String {
    let method = rust_identifier(&operation.operation_id);
    let (parameters, conversions, arguments) = native_inputs(operation, crate_name);
    let body = native_call_body(operation, &method, &arguments);
    CodeWriter::from_parts([
        "    fn ".to_string(), method, "(&self".to_string(), parameters,
        ") -> PyResult<String> {\n".to_string(),
        "        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(to_python_error)?;\n".to_string(),
        conversions, body, "    }\n\n".to_string(),
    ])
}

fn native_inputs(operation: &Operation, crate_name: &str) -> (String, String, String) {
    let inputs = ordered_parameters(operation)
        .iter()
        .map(|parameter| native_parameter(operation, parameter, crate_name))
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
            [name.clone(), ": ".to_string(), name].concat()
        })
        .collect::<Vec<_>>();
    if let Some(input) = native_body_input(operation, crate_name) {
        conversions.push(input.0);
        fields.push(["request_body: ".to_string(), input.1].concat());
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
        [
            crate_name.to_string(),
            "::".to_string(),
            type_identifier(&operation.operation_id),
            "Request { ".to_string(),
            fields.join(", "),
            " }".to_string(),
        ]
        .concat(),
    )
}

fn native_parameter(
    operation: &Operation,
    parameter: &crate::Parameter,
    crate_name: &str,
) -> (String, String, String) {
    let name = rust_identifier(&parameter.name);
    if parameter.location == ParameterLocation::Path {
        return (
            [", ".to_string(), name.clone(), ": String".to_string()].concat(),
            String::new(),
            name,
        );
    }
    let ty = native_inline_type(&parameter.schema, crate_name, || {
        inline_parameter_type_name(operation, parameter)
    })
    .unwrap_or_else(|| native_rust_schema_type(&parameter.schema, crate_name));
    let signature = if parameter.required {
        [", ".to_string(), name.clone(), ": String".to_string()].concat()
    } else {
        [
            ", ".to_string(),
            name.clone(),
            ": Option<String>".to_string(),
        ]
        .concat()
    };
    let conversion = native_parameter_conversion(&name, &ty, parameter.required);
    (signature, conversion, name)
}

fn native_parameter_conversion(name: &str, ty: &str, required: bool) -> String {
    if required {
        [
            "        let ",
            name,
            ": ",
            ty,
            " = serde_json::from_str(&",
            name,
            ").map_err(to_python_error)?;\n",
        ]
        .concat()
    } else {
        [
            "        let ",
            name,
            ": Option<",
            ty,
            "> = ",
            name,
            ".map(|value| serde_json::from_str(&value)).transpose().map_err(to_python_error)?;\n",
        ]
        .concat()
    }
}

fn native_body_input(operation: &Operation, crate_name: &str) -> Option<(String, String)> {
    let body = operation.request_body_details.as_ref()?;
    let schema = body.schema.as_ref()?;
    let ty = native_inline_type(schema, crate_name, || {
        inline_request_body_type_name(operation)
    })
    .unwrap_or_else(|| native_rust_schema_type(schema, crate_name));
    if body.required {
        Some((
            [
                "        let request_body: ",
                ty.as_str(),
                " = serde_json::from_str(&request_body).map_err(to_python_error)?;\n",
            ]
            .concat(),
            "request_body".to_string(),
        ))
    } else {
        Some((
            [
                "        let request_body: Option<", ty.as_str(),
                "> = request_body.map(|value| serde_json::from_str(&value)).transpose().map_err(to_python_error)?;\n",
            ].concat(),
            "request_body".to_string(),
        ))
    }
}

fn native_call_body(operation: &Operation, method: &str, arguments: &str) -> String {
    if has_error_responses(operation) {
        CodeWriter::from_parts([
            "        match runtime.block_on(self.inner.".to_string(), method.to_string(), "(".to_string(), arguments.to_string(), ")) {\n".to_string(),
            "            Ok(value) => encode_success_value(serde_json::to_value(value).map_err(to_python_error)?),\n".to_string(),
            "            Err(error) => encode_".to_string(), method.to_string(), "_error(error),\n        }".to_string(),
        ])
    } else {
        CodeWriter::from_parts([
            "        match runtime.block_on(self.inner.".to_string(), method.to_string(), "(".to_string(), arguments.to_string(), ")) {\n".to_string(),
            "            Ok(value) => encode_success_value(serde_json::to_value(value).map_err(to_python_error)?),\n".to_string(),
            "            Err(SdkError::Http { status, body }) => encode_http_error(status, serde_json::Value::String(body)),\n".to_string(),
            "            Err(error) => Err(to_python_error(error)),\n        }".to_string(),
        ])
    }
}

fn native_rust_schema_type(schema: &Schema, crate_name: &str) -> String {
    match schema {
        Schema::Reference(name) => [
            crate_name.to_string(),
            "::".to_string(),
            type_identifier(name),
        ]
        .concat(),
        Schema::String {
            format: Some(format),
        } if format == "binary" => "Vec<u8>".to_string(),
        Schema::String { .. } => "String".to_string(),
        Schema::Integer { .. } => "i64".to_string(),
        Schema::Number { .. } => "f64".to_string(),
        Schema::Boolean => "bool".to_string(),
        Schema::Array(item) => [
            "Vec<".to_string(),
            native_rust_schema_type(item, crate_name),
            ">".to_string(),
        ]
        .concat(),
        _ => "serde_json::Value".to_string(),
    }
}

fn native_inline_type<F>(schema: &Schema, crate_name: &str, name: F) -> Option<String>
where
    F: FnOnce() -> syn::Ident,
{
    if matches!(schema, Schema::TypedEnum { .. } | Schema::Const { .. }) {
        let name = name().to_string();
        Some([crate_name, "::", &name].concat())
    } else {
        None
    }
}
