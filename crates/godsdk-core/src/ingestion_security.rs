use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::ingestion::IngestionError;
use crate::ir::{
    OAuth2Flow, ParameterLocation, RequiredSecurityScheme, SecurityRequirement, SecurityScheme,
    SecuritySchemeKind,
};

#[derive(Debug, Deserialize)]
pub(crate) struct RawSecurityScheme {
    #[serde(rename = "type")]
    scheme_type: String,
    scheme: Option<String>,
    #[serde(rename = "bearerFormat")]
    bearer_format: Option<String>,
    name: Option<String>,
    #[serde(rename = "in")]
    location: Option<String>,
    flows: Option<BTreeMap<String, RawOAuth2Flow>>,
}

#[derive(Debug, Deserialize)]
struct RawOAuth2Flow {
    #[serde(rename = "authorizationUrl")]
    authorization_url: Option<String>,
    #[serde(rename = "tokenUrl")]
    token_url: Option<String>,
    #[serde(rename = "refreshUrl")]
    refresh_url: Option<String>,
    #[serde(default)]
    scopes: BTreeMap<String, String>,
}

pub(crate) fn normalize_security_schemes(
    raw_schemes: BTreeMap<String, RawSecurityScheme>,
) -> Result<BTreeMap<String, SecurityScheme>, IngestionError> {
    raw_schemes
        .into_iter()
        .map(|(name, raw)| normalize_security_scheme(name, raw))
        .collect()
}

fn normalize_security_scheme(
    name: String,
    raw: RawSecurityScheme,
) -> Result<(String, SecurityScheme), IngestionError> {
    let kind = match raw.scheme_type.as_str() {
        "http" => normalize_http_scheme(&name, raw)?,
        "apiKey" => normalize_api_key_scheme(&name, raw)?,
        "oauth2" => normalize_oauth2_scheme(&name, raw)?,
        scheme_type => {
            return Err(IngestionError::UnsupportedSecurityScheme {
                name,
                detail: format!("security scheme type {scheme_type} is not supported"),
            });
        }
    };
    let scheme = SecurityScheme {
        name: name.clone(),
        kind,
    };
    Ok((name, scheme))
}

fn normalize_http_scheme(
    name: &str,
    raw: RawSecurityScheme,
) -> Result<SecuritySchemeKind, IngestionError> {
    let scheme = raw
        .scheme
        .ok_or_else(|| IngestionError::UnsupportedSecurityScheme {
            name: name.to_string(),
            detail: "HTTP schemes require a scheme value".to_string(),
        })?;
    Ok(SecuritySchemeKind::Http {
        scheme,
        bearer_format: raw.bearer_format,
    })
}

fn normalize_api_key_scheme(
    name: &str,
    raw: RawSecurityScheme,
) -> Result<SecuritySchemeKind, IngestionError> {
    let key_name = raw
        .name
        .ok_or_else(|| IngestionError::UnsupportedSecurityScheme {
            name: name.to_string(),
            detail: "API key schemes require a name".to_string(),
        })?;
    let raw_location = raw
        .location
        .ok_or_else(|| IngestionError::UnsupportedSecurityScheme {
            name: name.to_string(),
            detail: "API key schemes require an in value".to_string(),
        })?;
    let location = parse_security_location(&raw_location).ok_or_else(|| {
        IngestionError::UnsupportedSecurityScheme {
            name: name.to_string(),
            detail: format!("API key location {raw_location} must be header, query, or cookie"),
        }
    })?;
    Ok(SecuritySchemeKind::ApiKey {
        name: key_name,
        location,
    })
}

fn normalize_oauth2_scheme(
    name: &str,
    raw: RawSecurityScheme,
) -> Result<SecuritySchemeKind, IngestionError> {
    let flows = raw
        .flows
        .unwrap_or_default()
        .into_iter()
        .map(|(flow, raw_flow)| OAuth2Flow {
            flow,
            authorization_url: raw_flow.authorization_url,
            token_url: raw_flow.token_url,
            refresh_url: raw_flow.refresh_url,
            scopes: raw_flow.scopes,
        })
        .collect::<Vec<_>>();
    if flows.is_empty() {
        return Err(IngestionError::UnsupportedSecurityScheme {
            name: name.to_string(),
            detail: "OAuth2 schemes require at least one flow".to_string(),
        });
    }
    Ok(SecuritySchemeKind::OAuth2 { flows })
}

fn parse_security_location(location: &str) -> Option<ParameterLocation> {
    match location {
        "header" => Some(ParameterLocation::Header),
        "query" => Some(ParameterLocation::Query),
        "cookie" => Some(ParameterLocation::Cookie),
        _ => None,
    }
}

pub(crate) fn normalize_security_requirements(
    raw_requirements: &[BTreeMap<String, Vec<String>>],
    security_schemes: &BTreeMap<String, SecurityScheme>,
) -> Result<Vec<SecurityRequirement>, IngestionError> {
    raw_requirements
        .iter()
        .map(|raw_requirement| normalize_security_requirement(raw_requirement, security_schemes))
        .collect()
}

fn normalize_security_requirement(
    raw_requirement: &BTreeMap<String, Vec<String>>,
    security_schemes: &BTreeMap<String, SecurityScheme>,
) -> Result<SecurityRequirement, IngestionError> {
    let mut schemes = raw_requirement
        .iter()
        .map(|(name, scopes)| normalize_required_scheme(name, scopes, security_schemes))
        .collect::<Result<Vec<_>, _>>()?;
    schemes.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(SecurityRequirement { schemes })
}

fn normalize_required_scheme(
    name: &str,
    scopes: &[String],
    security_schemes: &BTreeMap<String, SecurityScheme>,
) -> Result<RequiredSecurityScheme, IngestionError> {
    let scheme =
        security_schemes
            .get(name)
            .ok_or_else(|| IngestionError::UnknownSecurityScheme {
                name: name.to_string(),
            })?;
    validate_security_scopes(scheme, scopes)?;
    Ok(RequiredSecurityScheme {
        name: name.to_string(),
        scopes: scopes.to_vec(),
    })
}

fn validate_security_scopes(
    scheme: &SecurityScheme,
    requested_scopes: &[String],
) -> Result<(), IngestionError> {
    let SecuritySchemeKind::OAuth2 { flows } = &scheme.kind else {
        return Ok(());
    };
    let declared_scopes = flows
        .iter()
        .flat_map(|flow| flow.scopes.keys())
        .collect::<BTreeSet<_>>();
    for scope in requested_scopes {
        if !declared_scopes.contains(scope) {
            return Err(IngestionError::UnknownSecurityScope {
                scheme: scheme.name.clone(),
                scope: scope.clone(),
            });
        }
    }
    Ok(())
}
