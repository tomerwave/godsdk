use std::collections::BTreeMap;

use crate::Schema;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiIr {
    pub openapi_version: String,
    pub title: String,
    pub version: String,
    pub operations: Vec<Operation>,
    pub schemas: BTreeMap<String, Schema>,
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
    pub response_statuses: Vec<String>,
    pub responses: Vec<Response>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterLocation {
    Query,
    Header,
    Path,
    Cookie,
}
