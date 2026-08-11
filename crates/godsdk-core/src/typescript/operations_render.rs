use crate::code_writer::{CodeWriter, concatenate};
use crate::rust_ast::{inline_parameter_type_name, inline_request_body_type_name};
use crate::{ApiIr, Operation, ParameterLocation, Schema, rust_identifier};
use proc_macro2::TokenStream;
use quote::quote;

use super::{
    has_error_responses, operation_response_name, ordered_parameters, schema_model_name,
    schema_type_name, ts_identifier, type_identifier,
};

pub(super) fn native_schema_type_name(schema: &Schema, spec: &ApiIr) -> String {
    match schema {
        Schema::String {
            format: Some(format),
        } if format == "binary" => "Uint8Array".to_string(),
        Schema::String { .. } => "string".to_string(),
        Schema::Integer { .. } | Schema::Number { .. } => "number".to_string(),
        Schema::Boolean => "boolean".to_string(),
        _ => {
            let _ = spec;
            "NativeValue".to_string()
        }
    }
}

fn schema_validator_name(schema: &Schema, spec: &ApiIr) -> String {
    match schema {
        Schema::Reference(name) if spec.schemas.contains_key(name) => {
            concatenate(&[name, "Schema"])
        }
        _ => "NativeValueSchema".to_string(),
    }
}

pub(super) fn render_operation(operation: &Operation, _spec: &ApiIr) -> String {
    let method = ts_identifier(&operation.operation_id);
    let parameters = public_parameters(operation, _spec);
    let arguments = public_arguments(operation, _spec);
    let (return_type, success, error_handling) = public_result(operation, _spec);
    CodeWriter::from_parts([
        "  async ".to_string(),
        method.clone(),
        "(".to_string(),
        parameters,
        "): Promise<".to_string(),
        return_type,
        "> {\n".to_string(),
        "    const result = await this.native.".to_string(),
        method,
        "(".to_string(),
        arguments,
        ");\n    if (!result.ok) {\n".to_string(),
        error_handling,
        "    }\n".to_string(),
        success,
        "  }\n\n".to_string(),
    ])
}

fn public_parameters(operation: &Operation, spec: &ApiIr) -> String {
    let mut parameters = Vec::new();
    if let Some(body) = operation
        .request_body_details
        .as_ref()
        .filter(|body| body.required)
    {
        let ty = body
            .schema
            .as_ref()
            .map(|schema| schema_type_name(schema, spec))
            .unwrap_or_else(|| "NativeValue".to_string());
        parameters.push(concatenate(&["requestBody: ", &ty]));
    }
    parameters.extend(
        ordered_parameters(operation)
            .into_iter()
            .filter(|parameter| parameter.required)
            .map(|parameter| {
                let name = ts_identifier(&parameter.name);
                let ty = schema_type_name(&parameter.schema, spec);
                concatenate(&[&name, ": ", &ty])
            }),
    );
    if let Some(body) = operation
        .request_body_details
        .as_ref()
        .filter(|body| !body.required)
    {
        let ty = body
            .schema
            .as_ref()
            .map(|schema| schema_type_name(schema, spec))
            .unwrap_or_else(|| "NativeValue".to_string());
        parameters.push(concatenate(&["requestBody?: ", &ty]));
    }
    parameters.extend(
        ordered_parameters(operation)
            .into_iter()
            .filter(|parameter| !parameter.required)
            .map(|parameter| {
                let name = ts_identifier(&parameter.name);
                let ty = schema_type_name(&parameter.schema, spec);
                concatenate(&[&name, "?: ", &ty])
            }),
    );
    parameters.join(", ")
}

fn public_arguments(operation: &Operation, spec: &ApiIr) -> String {
    let mut arguments = Vec::new();
    if let Some(body) = operation
        .request_body_details
        .as_ref()
        .filter(|body| body.required)
    {
        let schema = body
            .schema
            .as_ref()
            .map(|schema| schema_validator_name(schema, spec))
            .unwrap_or_else(|| "NativeValueSchema".to_string());
        arguments.push(native_json_argument(body, &schema, "requestBody"));
    }
    arguments.extend(
        ordered_parameters(operation)
            .into_iter()
            .filter(|parameter| parameter.required)
            .map(|parameter| ts_identifier(&parameter.name)),
    );
    if let Some(body) = operation
        .request_body_details
        .as_ref()
        .filter(|body| !body.required)
    {
        let schema = body
            .schema
            .as_ref()
            .map(|schema| schema_validator_name(schema, spec))
            .unwrap_or_else(|| "NativeValueSchema".to_string());
        let argument = native_json_argument(body, &schema, "requestBody");
        arguments.push(concatenate(&[
            "requestBody === undefined ? undefined : ",
            &argument,
        ]));
    }
    arguments.extend(
        ordered_parameters(operation)
            .into_iter()
            .filter(|parameter| !parameter.required)
            .map(|parameter| ts_identifier(&parameter.name)),
    );
    arguments.join(", ")
}

fn native_json_argument(body: &crate::RequestBody, schema: &str, variable: &str) -> String {
    if body.content_type == "multipart/form-data" || body.content_type == "application/octet-stream"
    {
        concatenate(&[
            "JSON.stringify(",
            schema,
            ".parse(",
            variable,
            "), (_key, value) => value instanceof Uint8Array ? Array.from(value) : value)",
        ])
    } else {
        concatenate(&["JSON.stringify(", schema, ".parse(", variable, "))"])
    }
}

fn public_result(operation: &Operation, spec: &ApiIr) -> (String, String, String) {
    let response = operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .and_then(|response| response.schema.as_ref());
    let return_type = response
        .map(|response| {
            schema_model_name(response, spec)
                .map(|name| super::type_alias_name(&name))
                .unwrap_or_else(|| operation_response_name(operation))
        })
        .unwrap_or_else(|| "void".to_string());
    let success = response.map_or_else(
        || "    return;\n".to_string(),
        |response| {
            let model = schema_model_name(response, spec)
                .unwrap_or_else(|| operation_response_name(operation));
            concatenate(&["    return ", &model, "Schema.parse(result.value);\n"])
        },
    );
    let error = has_error_responses(operation)
        .then(|| concatenate(&[&type_identifier(&operation.operation_id), "Error"]));
    let error_handling = error.map_or_else(
        || "      throw new SdkHttpError(result.status, result.body);\n".to_string(),
        |error| concatenate(&["      throw ", &error, ".from(result);\n"]),
    );
    (return_type, success, error_handling)
}

pub(super) fn render_native_operation(operation: &Operation, crate_name: &str) -> TokenStream {
    let (parameters, conversions, arguments) = native_inputs(operation, crate_name);
    let method = rust_identifier(&operation.operation_id);
    let body = native_call_body(operation, &method, &arguments);
    let source = CodeWriter::from_parts([
        "    #[napi]\n    pub async fn ".to_string(),
        method,
        "(&self".to_string(),
        parameters,
        ") -> Result<serde_json::Value> {\n".to_string(),
        conversions,
        body,
        "    }\n\n".to_string(),
    ]);
    let item = syn::parse_str::<syn::ImplItemFn>(&source).unwrap_or_else(|error| {
        panic!("TypeScript native operation emitted invalid Rust: {error}")
    });
    quote! { #item }
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
            concatenate(&[&name, ": ", &name])
        })
        .collect::<Vec<_>>();
    if let Some(body) = native_body_input(operation, crate_name) {
        conversions.push(body.0);
        fields.push(concatenate(&["request_body: ", &body.1]));
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
        concatenate(&[
            crate_name,
            "::",
            &type_identifier(&operation.operation_id),
            "Request { ",
            &fields.join(", "),
            " }",
        ]),
    )
}

fn native_parameter(
    operation: &Operation,
    parameter: &crate::Parameter,
    crate_name: &str,
) -> (String, String, String) {
    let name = rust_identifier(&parameter.name);
    if parameter.location == ParameterLocation::Path {
        return (concatenate(&[", ", &name, ": String"]), String::new(), name);
    }
    let ty = native_inline_type(&parameter.schema, crate_name, || {
        inline_parameter_type_name(operation, parameter)
    })
    .unwrap_or_else(|| native_rust_schema_type(&parameter.schema, crate_name));
    let signature = if parameter.required {
        concatenate(&[", ", &name, ": serde_json::Value"])
    } else {
        concatenate(&[", ", &name, ": Option<serde_json::Value>"])
    };
    let conversion = if parameter.required {
        concatenate(&[
            "        let ",
            &name,
            ": ",
            &ty,
            " = serde_json::from_value(",
            &name,
            ").map_err(to_napi_error)?;\n",
        ])
    } else {
        concatenate(&[
            "        let ",
            &name,
            ": Option<",
            &ty,
            "> = ",
            &name,
            ".map(serde_json::from_value).transpose().map_err(to_napi_error)?;\n",
        ])
    };
    (signature, conversion, name)
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
            concatenate(&[
                "        let request_body: ",
                &ty,
                " = serde_json::from_str(&request_body).map_err(to_napi_error)?;\n",
            ]),
            "request_body".to_string(),
        ))
    } else {
        Some((
            concatenate(&[
                "        let request_body: Option<",
                &ty,
                "> = request_body.map(|value| serde_json::from_str(&value)).transpose().map_err(to_napi_error)?;\n",
            ]),
            "request_body".to_string(),
        ))
    }
}

fn native_call_body(operation: &Operation, method: &str, arguments: &str) -> String {
    if has_error_responses(operation) {
        let error_type = concatenate(&[&type_identifier(&operation.operation_id), "Error"]);
        CodeWriter::from_parts([
            "        match self.inner.".to_string(), method.to_string(), "(".to_string(), arguments.to_string(), ").await {\n".to_string(),
            "            Ok(value) => Ok(serde_json::json!({\"ok\": true, \"value\": value})),\n            Err(".to_string(),
            error_type.clone(), "::Unexpected { status, body }) => Ok(serde_json::json!({\"ok\": false, \"status\": status, \"body\": body})),\n            Err(".to_string(), error_type.clone(),
            "::Transport(error)) => Err(to_napi_error(error)),\n".to_string(), native_error_arms(operation, &error_type), "        }".to_string(),
        ])
    } else {
        CodeWriter::from_parts([
            "        match self.inner.".to_string(), method.to_string(), "(".to_string(), arguments.to_string(), ").await {\n".to_string(),
            "            Ok(value) => Ok(serde_json::json!({\"ok\": true, \"value\": value})),\n            Err(SdkError::Http { status, body }) => Ok(serde_json::json!({\"ok\": false, \"status\": status, \"body\": body})),\n            Err(error) => Err(to_napi_error(error)),\n        }".to_string(),
        ])
    }
}

fn native_error_arms(operation: &Operation, error_type: &str) -> String {
    let mut writer = CodeWriter::default();
    for response in operation
        .responses
        .iter()
        .filter(|response| !response.status.starts_with('2'))
    {
        if let Some(arm) = native_error_arm(response, error_type) {
            writer.push(&arm);
        }
    }
    writer.finish()
}

fn native_error_arm(response: &crate::Response, error_type: &str) -> Option<String> {
    let status = response.status.parse::<u16>().ok()?.to_string();
    let body = if response.schema.is_some() {
        [
            "(value)) => Ok(serde_json::json!({\"ok\": false, \"status\": ",
            &status,
            ", \"body\": value})),\n",
        ]
        .concat()
    } else {
        [
            ") => Ok(serde_json::json!({\"ok\": false, \"status\": ",
            &status,
            ", \"body\": serde_json::Value::Null})),\n",
        ]
        .concat()
    };
    Some(["            Err(", error_type, "::Status", &status, &body].concat())
}

fn native_rust_schema_type(schema: &Schema, crate_name: &str) -> String {
    match schema {
        Schema::Reference(name) => concatenate(&[crate_name, "::", &type_identifier(name)]),
        Schema::String {
            format: Some(format),
        } if format == "binary" => "Vec<u8>".to_string(),
        Schema::String { .. } => "String".to_string(),
        Schema::TypedEnum { base, .. } => native_rust_schema_type(base, crate_name),
        Schema::Integer { .. } => "i64".to_string(),
        Schema::Number { .. } => "f64".to_string(),
        Schema::Boolean => "bool".to_string(),
        Schema::Array(item) => {
            concatenate(&["Vec<", &native_rust_schema_type(item, crate_name), ">"])
        }
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
