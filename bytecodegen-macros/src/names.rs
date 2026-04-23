pub(crate) fn is_upper_camel_case(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) fn upper_camel_to_snake(value: &str) -> String {
    let mut result = String::new();
    for (i, c) in value.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

pub(crate) fn snake_to_upper_camel(value: &str) -> String {
    let mut result = String::new();
    for segment in value.split('_') {
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            result.push(first.to_ascii_uppercase());
            result.push_str(chars.as_str());
        }
    }
    result
}
