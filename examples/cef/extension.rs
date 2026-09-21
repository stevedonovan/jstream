//! Parsing for the CEF extension's space-separated `key=value` fields.

use serde_json::{Map, Value};

/// Parse CEF extension fields into a JSON object.
///
/// Values may contain spaces. CEF escape sequences such as `\=`, `\\`, `\n`,
/// and `\r` are decoded. Parsing stops at the first malformed field.
pub fn parse_extension(input: &str) -> Map<String, Value> {
    let mut result = Map::new();
    let mut rest = input.trim();

    while !rest.is_empty() {
        let Some(equals) = find_unescaped(rest, '=') else {
            break;
        };
        let key = &rest[..equals];
        if key.is_empty() || key.chars().any(char::is_whitespace) {
            break;
        }

        let value_start = equals + 1;
        let value_end = find_next_key(&rest[value_start..])
            .map(|offset| value_start + offset)
            .unwrap_or(rest.len());

        result.insert(
            key.to_owned(),
            Value::String(unescape(rest[value_start..value_end].trim_end())),
        );
        rest = rest[value_end..].trim_start();
    }

    result
}

fn find_unescaped(input: &str, wanted: char) -> Option<usize> {
    let mut escaped = false;
    for (index, character) in input.char_indices() {
        if character == wanted && !escaped {
            return Some(index);
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    None
}

// An extension value ends at whitespace followed by another plausible key and
// an unescaped '='. This allows ordinary spaces inside values.
fn find_next_key(input: &str) -> Option<usize> {
    for (index, character) in input.char_indices() {
        if !character.is_whitespace() {
            continue;
        }

        let candidate = input[index..].trim_start();
        let key_end = find_unescaped(candidate, '=')?;
        let key = &candidate[..key_end];
        if !key.is_empty()
            && key
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character))
        {
            return Some(index);
        }
    }
    None
}

fn unescape(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut characters = input.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            match characters.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some(escaped) => result.push(escaped),
                None => result.push('\\'),
            }
        } else {
            result.push(character);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_fields_with_spaces_and_escapes() {
        assert_eq!(
            Value::Object(parse_extension(
                r#"src=10.0.0.4 act=blocked msg=Login failed reason\=bad password"#,
            )),
            json!({
                "act": "blocked",
                "msg": "Login failed reason=bad password",
                "src": "10.0.0.4"
            })
        );
    }
}
