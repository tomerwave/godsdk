use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn render() -> TokenStream {
    let helpers = [
        value_helpers(),
        pair_helpers(),
        path_helpers(),
        serialization_tests(),
    ];
    quote! { #(#helpers)* }
}

fn value_helpers() -> TokenStream {
    quote! {
        fn parameter_scalar(value: &serde_json::Value) -> Result<String, SdkError> {
            match value {
                serde_json::Value::String(value) => Ok(value.clone()),
                serde_json::Value::Number(value) => Ok(value.to_string()),
                serde_json::Value::Bool(value) => Ok(value.to_string()),
                serde_json::Value::Null => Ok(String::new()),
                _ => Err(SdkError::Serialization("parameter value must be scalar".to_string())),
            }
        }

        fn parameter_components(
            value: &serde_json::Value,
        ) -> Result<Vec<(String, String)>, SdkError> {
            match value {
                serde_json::Value::Array(values) => values
                    .iter()
                    .map(|value| parameter_scalar(value).map(|value| (String::new(), value)))
                    .collect(),
                serde_json::Value::Object(values) => values
                    .iter()
                    .map(|(name, value)| parameter_scalar(value).map(|value| (name.clone(), value)))
                    .collect(),
                _ => parameter_scalar(value).map(|value| vec![(String::new(), value)]),
            }
        }

        fn parameter_join(values: &[(String, String)], delimiter: &str) -> String {
            values
                .iter()
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>()
                .join(delimiter)
        }

        fn parameter_join_object(
            values: &[(String, String)],
            delimiter: &str,
            separator: &str,
        ) -> String {
            values
                .iter()
                .map(|(name, value)| format!("{name}{separator}{value}"))
                .collect::<Vec<_>>()
                .join(delimiter)
        }
    }
}

fn pair_helpers() -> TokenStream {
    let helpers = [
        parameter_helpers(),
        parameter_value_helpers(),
        cookie_helpers(),
    ];
    quote! { #(#helpers)* }
}

fn parameter_helpers() -> TokenStream {
    quote! {
        pub(crate) fn serialize_parameter(
            name: &str,
            value: serde_json::Value,
            style: &str,
            explode: bool,
        ) -> Result<Vec<(String, String)>, SdkError> {
            let components = parameter_components(&value)?;
            match (&value, style, explode) {
                (serde_json::Value::Array(_), "form", true) => Ok(components
                    .into_iter()
                    .map(|(_, value)| (name.to_string(), value))
                    .collect()),
                (serde_json::Value::Array(_), "spaceDelimited", _) => {
                    Ok(vec![(name.to_string(), parameter_join(&components, " "))])
                }
                (serde_json::Value::Array(_), "pipeDelimited", _) => {
                    Ok(vec![(name.to_string(), parameter_join(&components, "|"))])
                }
                (serde_json::Value::Array(_), _, _) => {
                    Ok(vec![(name.to_string(), parameter_join(&components, ","))])
                }
                (serde_json::Value::Object(_), "deepObject", _) => Ok(components
                    .into_iter()
                    .map(|(key, value)| (format!("{name}[{key}]"), value))
                    .collect()),
                (serde_json::Value::Object(_), "form", true) => Ok(components),
                (serde_json::Value::Object(_), _, true) => Ok(vec![(
                    name.to_string(),
                    parameter_join_object(&components, ",", "="),
                )]),
                (serde_json::Value::Object(_), _, false) => Ok(vec![(
                    name.to_string(),
                    parameter_join_object(&components, ",", ","),
                )]),
                _ => Ok(vec![(name.to_string(), parameter_scalar(&value)?)]),
            }
        }

    }
}

fn parameter_value_helpers() -> TokenStream {
    quote! {
        pub(crate) fn serialize_parameter_value<T: serde::Serialize>(
            name: &str,
            value: &T,
            style: &str,
            explode: bool,
        ) -> Result<Vec<(String, String)>, SdkError> {
            let value = serde_json::to_value(value)
                .map_err(|error| SdkError::Serialization(error.to_string()))?;
            serialize_parameter(name, value, style, explode)
        }
    }
}

fn cookie_helpers() -> TokenStream {
    quote! {
        pub(crate) fn serialize_cookie(
            name: &str,
            value: serde_json::Value,
            explode: bool,
        ) -> Result<String, SdkError> {
            let components = parameter_components(&value)?;
            match (&value, explode) {
                (serde_json::Value::Object(_), true) => Ok(components
                    .into_iter()
                    .map(|(name, value)| format!("{name}={value}"))
                    .collect::<Vec<_>>()
                    .join("; ")),
                (serde_json::Value::Object(_), false) => Ok(format!(
                    "{name}={}",
                    parameter_join_object(&components, ",", ",")
                )),
                (serde_json::Value::Array(_), true) => Ok(components
                    .into_iter()
                    .map(|(_, value)| format!("{name}={value}"))
                    .collect::<Vec<_>>()
                    .join("; ")),
                (serde_json::Value::Array(_), false) => Ok(format!(
                    "{name}={}",
                    parameter_join(&components, ",")
                )),
                _ => Ok(format!("{name}={}", parameter_scalar(&value)?)),
            }
        }

        pub(crate) fn serialize_cookie_value<T: serde::Serialize>(
            name: &str,
            value: &T,
            explode: bool,
        ) -> Result<String, SdkError> {
            let value = serde_json::to_value(value)
                .map_err(|error| SdkError::Serialization(error.to_string()))?;
            serialize_cookie(name, value, explode)
        }
    }
}

fn path_helpers() -> TokenStream {
    let helpers = [
        path_parameter_helpers(),
        path_value_helpers(),
        path_array_helpers(),
        path_object_helpers(),
    ];
    quote! { #(#helpers)* }
}

fn path_parameter_helpers() -> TokenStream {
    quote! {
        pub(crate) fn serialize_path_parameter(
            value: serde_json::Value,
            name: &str,
            style: &str,
            explode: bool,
        ) -> Result<String, SdkError> {
            let components = parameter_components(&value)?;
            match value {
                serde_json::Value::Array(_) => serialize_path_array(&components, name, style, explode),
                serde_json::Value::Object(_) => serialize_path_object(&components, name, style, explode),
                value => serialize_path_scalar(&value, name, style),
            }
        }

        pub(crate) fn serialize_path_parameter_value<T: serde::Serialize>(
            value: &T,
            name: &str,
            style: &str,
            explode: bool,
        ) -> Result<String, SdkError> {
            let value = serde_json::to_value(value)
                .map_err(|error| SdkError::Serialization(error.to_string()))?;
            serialize_path_parameter(value, name, style, explode)
        }

    }
}

fn path_value_helpers() -> TokenStream {
    quote! {
        fn serialize_path_scalar(
            value: &serde_json::Value,
            name: &str,
            style: &str,
        ) -> Result<String, SdkError> {
            let value = encode_path_segment(&parameter_scalar(value)?);
            Ok(match style {
                "label" => format!(".{value}"),
                "matrix" => format!(";{name}={value}"),
                _ => value,
            })
        }

    }
}

fn path_array_helpers() -> TokenStream {
    quote! {
        fn serialize_path_array(
            components: &[(String, String)],
            name: &str,
            style: &str,
            explode: bool,
        ) -> Result<String, SdkError> {
            let values = components
                .iter()
                .map(|(_, value)| encode_path_segment(value))
                .collect::<Vec<_>>();
            Ok(match (style, explode) {
                ("label", true) => format!(".{}", values.join(".")),
                ("label", false) => format!(".{}", values.join(",")),
                ("matrix", true) => values
                    .iter()
                    .map(|value| format!(";{name}={value}"))
                    .collect(),
                ("matrix", false) => format!(";{name}={}", values.join(",")),
                _ => values.join(","),
            })
        }

    }
}

fn path_object_helpers() -> TokenStream {
    let helpers = [
        path_object_main(),
        path_object_label(),
        path_object_matrix(),
        path_object_default(),
        path_object_pairs(),
    ];
    quote! { #(#helpers)* }
}

fn path_object_main() -> TokenStream {
    quote! {
        fn serialize_path_object(
            components: &[(String, String)],
            name: &str,
            style: &str,
            explode: bool,
        ) -> Result<String, SdkError> {
            let values = components
                .iter()
                .map(|(key, value)| (key, encode_path_segment(value)))
                .collect::<Vec<_>>();
            Ok(match style {
                "label" => serialize_path_object_label(&values, explode),
                "matrix" => serialize_path_object_matrix(&values, name, explode),
                _ => serialize_path_object_default(&values, explode),
            })
        }

    }
}

fn path_object_label() -> TokenStream {
    quote! {
        fn serialize_path_object_label(
            values: &[(&String, String)],
            explode: bool,
        ) -> String {
            if explode {
                format!(
                    ".{}",
                    values
                        .iter()
                        .map(|(key, value)| format!("{key}={value}"))
                        .collect::<Vec<_>>()
                        .join(".")
                )
            } else {
                format!(".{}", serialize_path_object_pairs(values, ","))
            }
        }

    }
}

fn path_object_matrix() -> TokenStream {
    quote! {
        fn serialize_path_object_matrix(
            values: &[(&String, String)],
            name: &str,
            explode: bool,
        ) -> String {
            if explode {
                values
                    .iter()
                    .map(|(key, value)| format!(";{key}={value}"))
                    .collect()
            } else {
                format!(";{name}={}", serialize_path_object_pairs(values, ","))
            }
        }

    }
}

fn path_object_default() -> TokenStream {
    quote! {
        fn serialize_path_object_default(
            values: &[(&String, String)],
            explode: bool,
        ) -> String {
            if explode {
                values
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            } else {
                serialize_path_object_pairs(values, ",")
            }
        }

    }
}

fn path_object_pairs() -> TokenStream {
    quote! {
        fn serialize_path_object_pairs(
            values: &[(&String, String)],
            delimiter: &str,
        ) -> String {
            values
                .iter()
                .flat_map(|(key, value)| [key.to_string(), value.clone()])
                .collect::<Vec<_>>()
                .join(delimiter)
        }
    }
}

fn serialization_tests() -> TokenStream {
    quote! {
        #[cfg(test)]
        mod parameter_serialization_tests {
            use super::*;

            #[test]
            fn serializes_openapi_parameter_styles() {
                let array = vec!["red", "blue"];
                assert_eq!(
                    serialize_parameter_value("tags", &array, "pipeDelimited", false)
                        .unwrap(),
                    vec![("tags".to_string(), "red|blue".to_string())]
                );

                let object = serde_json::json!({"role": "admin", "active": true});
                assert_eq!(
                    serialize_parameter("filter", object, "deepObject", true).unwrap(),
                    vec![
                        ("filter[active]".to_string(), "true".to_string()),
                        ("filter[role]".to_string(), "admin".to_string())
                    ]
                );

                assert_eq!(
                    serialize_path_parameter_value(&array, "tag", "label", false).unwrap(),
                    ".red,blue"
                );
                assert_eq!(
                    serialize_cookie_value("tags", &array, true).unwrap(),
                    "tags=red; tags=blue"
                );
            }
        }
    }
}
