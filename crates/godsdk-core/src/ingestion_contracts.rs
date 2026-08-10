use std::collections::BTreeMap;

use crate::ingestion::IngestionError;
use crate::ir::{RequestBody, Response, ResponseHeader};
use crate::schema::schema_from_value;

pub(crate) fn normalize_operation_contract(
    request_body: Option<&serde_json::Value>,
    responses: &BTreeMap<String, serde_json::Value>,
    path: &str,
) -> Result<(Option<RequestBody>, Vec<Response>), IngestionError> {
    let request_body = normalize_request_body(request_body, path)?;
    let responses = responses
        .iter()
        .map(|(status, response)| normalize_response(status, response, path))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((request_body, responses))
}

fn normalize_request_body(
    request_body: Option<&serde_json::Value>,
    path: &str,
) -> Result<Option<RequestBody>, IngestionError> {
    let Some(body) = request_body else {
        return Ok(None);
    };
    let Some(content) = body.get("content").and_then(first_content) else {
        return Ok(None);
    };
    let schema = content
        .1
        .map(|schema| schema_from_value(schema, &format!("{path}.requestBody")))
        .transpose()?;
    Ok(Some(RequestBody {
        required: body
            .get("required")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        content_type: content.0.to_string(),
        schema,
    }))
}

fn normalize_response(
    status: &str,
    response: &serde_json::Value,
    path: &str,
) -> Result<Response, IngestionError> {
    let content = response.get("content").and_then(first_content);
    let schema = content
        .and_then(|(_, schema)| schema)
        .map(|schema| schema_from_value(schema, &format!("{path} responses {status}")))
        .transpose()?;
    let headers = response
        .get("headers")
        .and_then(serde_json::Value::as_object)
        .map(|headers| normalize_response_headers(headers, path, status))
        .transpose()?
        .unwrap_or_default();
    Ok(Response {
        status: status.to_string(),
        schema,
        content_type: content.map(|(content_type, _)| content_type.to_string()),
        headers,
    })
}

fn normalize_response_headers(
    headers: &serde_json::Map<String, serde_json::Value>,
    path: &str,
    status: &str,
) -> Result<Vec<ResponseHeader>, IngestionError> {
    headers
        .iter()
        .map(|(name, header)| {
            let schema = header
                .get("schema")
                .map(|schema| {
                    schema_from_value(schema, &format!("{path} response {status} header {name}"))
                })
                .transpose()?;
            Ok(ResponseHeader {
                name: name.clone(),
                required: header
                    .get("required")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                schema,
            })
        })
        .collect()
}

fn first_content(content: &serde_json::Value) -> Option<(&str, Option<&serde_json::Value>)> {
    content
        .as_object()?
        .iter()
        .min_by_key(|(media_type, _)| *media_type)
        .map(|(media_type, media)| (media_type.as_str(), media.get("schema")))
}
