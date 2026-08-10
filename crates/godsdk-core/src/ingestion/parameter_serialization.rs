use crate::{IngestionError, Parameter, ParameterLocation, ParameterSerialization, ParameterStyle};

pub(super) fn parse_location(
    location: &str,
    path: &str,
) -> Result<ParameterLocation, IngestionError> {
    match location {
        "query" => Ok(ParameterLocation::Query),
        "header" => Ok(ParameterLocation::Header),
        "path" => Ok(ParameterLocation::Path),
        "cookie" => Ok(ParameterLocation::Cookie),
        _ => Err(IngestionError::Parse(format!(
            "unsupported parameter location {location} at {path}"
        ))),
    }
}

pub(super) fn path_parameters(path: &str) -> Vec<String> {
    path.split('{')
        .skip(1)
        .filter_map(|segment| segment.split('}').next())
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn order(path: &str, parameter: &Parameter) -> (u8, usize, String) {
    if parameter.location == ParameterLocation::Path {
        let position = path_parameters(path)
            .iter()
            .position(|name| name == &parameter.name)
            .unwrap_or(usize::MAX);
        return (0, position, parameter.name.clone());
    }
    let location = match parameter.location {
        ParameterLocation::Query => 1,
        ParameterLocation::Header => 2,
        ParameterLocation::Cookie => 3,
        ParameterLocation::Path => unreachable!("path parameters return above"),
    };
    (location, 0, parameter.name.clone())
}

pub(super) fn normalize(
    style: Option<&str>,
    explode: Option<bool>,
    location: &str,
    path: &str,
) -> Result<ParameterSerialization, IngestionError> {
    let default = default_style(location, path)?;
    let style = parse_style(style.unwrap_or(default), location, path)?;
    if !supports_style(location, style) {
        return Err(IngestionError::UnsupportedParameterStyle {
            style: style_name(style).to_string(),
            location: location.to_string(),
            path: path.to_string(),
        });
    }
    Ok(ParameterSerialization {
        style,
        explode: explode.unwrap_or(matches!(
            style,
            ParameterStyle::Form | ParameterStyle::DeepObject
        )),
    })
}

fn default_style(location: &str, path: &str) -> Result<&'static str, IngestionError> {
    match location {
        "path" | "header" => Ok("simple"),
        "query" | "cookie" => Ok("form"),
        _ => Err(IngestionError::Parse(format!(
            "unsupported parameter location {location} at {path}"
        ))),
    }
}

fn parse_style(value: &str, location: &str, path: &str) -> Result<ParameterStyle, IngestionError> {
    let style = match value {
        "simple" => ParameterStyle::Simple,
        "form" => ParameterStyle::Form,
        "label" => ParameterStyle::Label,
        "matrix" => ParameterStyle::Matrix,
        "spaceDelimited" => ParameterStyle::SpaceDelimited,
        "pipeDelimited" => ParameterStyle::PipeDelimited,
        "deepObject" => ParameterStyle::DeepObject,
        value => {
            return Err(IngestionError::UnsupportedParameterStyle {
                style: value.to_string(),
                location: location.to_string(),
                path: path.to_string(),
            });
        }
    };
    Ok(style)
}

fn supports_style(location: &str, style: ParameterStyle) -> bool {
    match location {
        "path" => matches!(
            style,
            ParameterStyle::Simple | ParameterStyle::Label | ParameterStyle::Matrix
        ),
        "query" => matches!(
            style,
            ParameterStyle::Form
                | ParameterStyle::SpaceDelimited
                | ParameterStyle::PipeDelimited
                | ParameterStyle::DeepObject
        ),
        "header" => matches!(style, ParameterStyle::Simple),
        "cookie" => matches!(style, ParameterStyle::Form),
        _ => false,
    }
}

fn style_name(style: ParameterStyle) -> &'static str {
    match style {
        ParameterStyle::Simple => "simple",
        ParameterStyle::Form => "form",
        ParameterStyle::Label => "label",
        ParameterStyle::Matrix => "matrix",
        ParameterStyle::SpaceDelimited => "spaceDelimited",
        ParameterStyle::PipeDelimited => "pipeDelimited",
        ParameterStyle::DeepObject => "deepObject",
    }
}
