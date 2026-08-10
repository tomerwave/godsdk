use crate::{ApiIr, Operation, ParameterLocation, Schema, rust_identifier};

use super::{
    has_error_responses, operation_response_name, ordered_parameters, schema_model_name,
    schema_type_name, ts_identifier, type_identifier,
};

pub(super) fn render_operation(operation: &Operation, _spec: &ApiIr) -> String {
    let method = ts_identifier(&operation.operation_id);
    let parameters = public_parameters(operation);
    let arguments = public_arguments(operation);
    let (return_type, success, error_handling) = public_result(operation);
    format!(
        "  async {method}({parameters}): Promise<{return_type}> {{\n    const result = await this.native.{method}({arguments});\n    if (!result.ok) {{\n{error_handling}    }}\n{success}  }}\n\n"
    )
}

fn public_parameters(operation: &Operation) -> String {
    let mut parameters = Vec::new();
    if let Some(body) = operation.request_body_details.as_ref() {
        let ty = body
            .schema
            .as_ref()
            .map(schema_type_name)
            .unwrap_or_else(|| "NativeValue".to_string());
        parameters.push(if body.required {
            format!("requestBody: {ty}")
        } else {
            format!("requestBody?: {ty}")
        });
    }
    parameters.extend(ordered_parameters(operation).into_iter().map(|parameter| {
        let name = ts_identifier(&parameter.name);
        let ty = schema_type_name(&parameter.schema);
        if parameter.required {
            format!("{name}: {ty}")
        } else {
            format!("{name}?: {ty}")
        }
    }));
    parameters.join(", ")
}

fn public_arguments(operation: &Operation) -> String {
    let mut arguments = Vec::new();
    if let Some(body) = operation.request_body_details.as_ref() {
        let schema = body
            .schema
            .as_ref()
            .map(schema_type_name)
            .unwrap_or_else(|| "NativeValue".to_string());
        arguments.push(if body.required {
            format!("JSON.stringify({schema}Schema.parse(requestBody))")
        } else {
            format!("requestBody === undefined ? undefined : JSON.stringify({schema}Schema.parse(requestBody))")
        });
    }
    arguments.extend(
        ordered_parameters(operation)
            .into_iter()
            .map(|parameter| ts_identifier(&parameter.name)),
    );
    arguments.join(", ")
}

fn public_result(operation: &Operation) -> (String, String, String) {
    let response = operation
        .responses
        .iter()
        .find(|response| response.status.starts_with('2'))
        .and_then(|response| response.schema.as_ref());
    let return_type = response
        .map(|response| {
            schema_model_name(response).unwrap_or_else(|| operation_response_name(operation))
        })
        .unwrap_or_else(|| "void".to_string());
    let success = response.map_or_else(
        || "    return;\n".to_string(),
        |response| {
            let model =
                schema_model_name(response).unwrap_or_else(|| operation_response_name(operation));
            format!("    return {model}Schema.parse(result.value);\n")
        },
    );
    let error = has_error_responses(operation)
        .then(|| format!("{}Error", type_identifier(&operation.operation_id)));
    let error_handling = error.map_or_else(
        || "      throw new SdkHttpError(result.status, result.body);\n".to_string(),
        |error| format!("      throw {error}.from(result);\n"),
    );
    (return_type, success, error_handling)
}

pub(super) fn render_native_operation(operation: &Operation, crate_name: &str) -> String {
    let (parameters, conversions, arguments) = native_inputs(operation, crate_name);
    let method = rust_identifier(&operation.operation_id);
    let body = native_call_body(operation, &method, &arguments);
    format!(
        "    #[napi]\n    pub async fn {method}(&self{parameters}) -> Result<serde_json::Value> {{\n{conversions}{body}\n    }}\n\n"
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
    if let Some(body) = native_body_input(operation, crate_name) {
        conversions.push(body.0);
        fields.push(format!("request_body: {}", body.1));
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
        format!(", {name}: serde_json::Value")
    } else {
        format!(", {name}: Option<serde_json::Value>")
    };
    let conversion = if parameter.required {
        format!(
            "        let {name}: {ty} = serde_json::from_value({name}).map_err(to_napi_error)?;\n"
        )
    } else {
        format!(
            "        let {name}: Option<{ty}> = {name}.map(serde_json::from_value).transpose().map_err(to_napi_error)?;\n"
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
                "        let request_body: {ty} = serde_json::from_str(&request_body).map_err(to_napi_error)?;\n"
            ),
            "request_body".to_string(),
        ))
    } else {
        Some((
            format!(
                "        let request_body: Option<{ty}> = request_body.map(|value| serde_json::from_str(&value)).transpose().map_err(to_napi_error)?;\n"
            ),
            "request_body".to_string(),
        ))
    }
}

fn native_call_body(operation: &Operation, method: &str, arguments: &str) -> String {
    if has_error_responses(operation) {
        let error_type = format!("{}Error", type_identifier(&operation.operation_id));
        let arms = native_error_arms(operation, &error_type);
        format!(
            "        match self.inner.{method}({arguments}).await {{\n            Ok(value) => Ok(serde_json::json!({{\"ok\": true, \"value\": value}})),\n            Err({error_type}::Unexpected {{ status, body }}) => Ok(serde_json::json!({{\"ok\": false, \"status\": status, \"body\": body}})),\n            Err({error_type}::Transport(error)) => Err(to_napi_error(error)),\n{arms}        }}"
        )
    } else {
        "        match self.inner.{method}({arguments}).await {{\n            Ok(value) => Ok(serde_json::json!({{\"ok\": true, \"value\": value}})),\n            Err(SdkError::Http {{ status, body }}) => Ok(serde_json::json!({{\"ok\": false, \"status\": status, \"body\": body}})),\n            Err(error) => Err(to_napi_error(error)),\n        }}".to_string()
    }
}

fn native_error_arms(operation: &Operation, error_type: &str) -> String {
    operation.responses.iter().filter(|response| !response.status.starts_with('2')).filter_map(|response| {
        let status = response.status.parse::<u16>().ok()?;
        let variant = format!("Status{status}");
        let (pattern, body) = response.schema.as_ref().map_or_else(|| (format!("{error_type}::{variant}"), "serde_json::Value::Null".to_string()), |_| (format!("{error_type}::{variant}(value)"), "value".to_string()));
        Some(format!("            Err({pattern}) => Ok(serde_json::json!({{\"ok\": false, \"status\": {status}, \"body\": {body}}})),\n"))
    }).collect()
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
