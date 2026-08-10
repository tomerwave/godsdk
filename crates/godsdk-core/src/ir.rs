use std::collections::BTreeMap;

use crate::Schema;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiIr {
    pub openapi_version: String,
    pub title: String,
    pub version: String,
    pub operations: Vec<Operation>,
    pub schemas: BTreeMap<String, Schema>,
    pub security: Option<Vec<SecurityRequirement>>,
    pub security_schemes: BTreeMap<String, SecurityScheme>,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub operation_id: String,
    pub method: HttpMethod,
    pub path: String,
    pub parameters: Vec<Parameter>,
    pub request_body: bool,
    pub request_body_schema: Option<Schema>,
    pub request_body_details: Option<RequestBody>,
    pub response_statuses: Vec<String>,
    pub responses: Vec<Response>,
    pub security: Option<Vec<SecurityRequirement>>,
}

impl Operation {
    pub fn canonical_key(&self) -> String {
        format!("{} {}", self.method.as_str(), self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: String,
    pub schema: Option<Schema>,
    pub content_type: Option<String>,
    pub headers: Vec<ResponseHeader>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestBody {
    pub required: bool,
    pub content_type: String,
    pub schema: Option<Schema>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseHeader {
    pub name: String,
    pub required: bool,
    pub schema: Option<Schema>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityScheme {
    pub name: String,
    pub kind: SecuritySchemeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecuritySchemeKind {
    Http {
        scheme: String,
        bearer_format: Option<String>,
    },
    ApiKey {
        name: String,
        location: ParameterLocation,
    },
    OAuth2 {
        flows: Vec<OAuth2Flow>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth2Flow {
    pub flow: String,
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub refresh_url: Option<String>,
    pub scopes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityRequirement {
    pub schemes: Vec<RequiredSecurityScheme>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredSecurityScheme {
    pub name: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HttpMethod {
    Delete,
    Get,
    Head,
    Options,
    Patch,
    Post,
    Put,
    Trace,
}

impl HttpMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Delete => "DELETE",
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Patch => "PATCH",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Trace => "TRACE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub location: ParameterLocation,
    pub required: bool,
    pub schema: Schema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterLocation {
    Query,
    Header,
    Path,
    Cookie,
}
