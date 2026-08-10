pub(super) fn type_identifier(value: &str) -> String {
    ts_identifier(value)
        .split(' ')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

pub(super) fn ts_property(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        value.to_string()
    } else {
        format!("{value:?}")
    }
}

pub(super) fn ts_identifier(value: &str) -> String {
    let mut output = String::new();
    for part in value
        .split(['-', '_', ' ', '.'])
        .filter(|part| !part.is_empty())
    {
        append_identifier_part(&mut output, part);
    }
    if output.is_empty() {
        "value".to_string()
    } else {
        output
    }
}

fn append_identifier_part(output: &mut String, part: &str) {
    if output.is_empty() {
        output.push_str(part);
        return;
    }
    let mut chars = part.chars();
    if let Some(first) = chars.next() {
        output.push(first.to_ascii_uppercase());
    }
    output.push_str(chars.as_str());
}

pub(super) fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
