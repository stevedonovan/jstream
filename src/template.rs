use serde_json::{Map, Value};

use crate::{Error, ParseStringErrorKind, Result, access::resolve_value, pointer::pointer_tokens};

/// Expand `{field}` placeholders from a JSON value into a string.
///
/// Placeholders starting with `/` are resolved as JSON Pointers. Other
/// placeholders are resolved as top-level fields. Missing values and nulls
/// expand to empty strings.
pub fn format_value(value: &Value, template: &str) -> String {
    let mut formatted = String::new();
    let mut rest = template;

    while let Some(start) = rest.find('{') {
        formatted.push_str(&rest[..start]);

        let placeholder = &rest[start + 1..];
        let Some(end) = placeholder.find('}') else {
            formatted.push_str(&rest[start..]);
            return formatted;
        };

        let key = &placeholder[..end];
        formatted.push_str(&format_field(value, key));
        rest = &placeholder[end + 1..];
    }

    formatted.push_str(rest);
    formatted
}

/// Extract string fields from text using the same `{field}` placeholder syntax
/// as [`format_value`].
pub fn parse_string(text: &str, template: &str) -> Result<Value> {
    let template = parse_template(template)?;
    let mut remaining = text;
    let mut fields = Map::new();

    for index in 0..template.len() {
        match &template[index] {
            TemplatePart::Literal(literal) => {
                let Some(next) = remaining.strip_prefix(literal) else {
                    return Err(parse_error(
                        ParseStringErrorKind::ExpectedLiteral {
                            literal: literal.clone(),
                            after_field: None,
                        },
                        Some(text.len() - remaining.len()),
                    ));
                };
                remaining = next;
            }
            TemplatePart::Field(key) => {
                let next_literal = template[index + 1..].iter().find_map(|part| match part {
                    TemplatePart::Literal(literal) if !literal.is_empty() => Some(literal.as_str()),
                    _ => None,
                });

                let captured = match next_literal {
                    Some(literal) => {
                        let Some(end) = remaining.find(literal) else {
                            return Err(parse_error(
                                ParseStringErrorKind::ExpectedLiteral {
                                    literal: literal.to_owned(),
                                    after_field: Some(key.clone()),
                                },
                                Some(text.len() - remaining.len()),
                            ));
                        };
                        let captured = &remaining[..end];
                        remaining = &remaining[end..];
                        captured
                    }
                    None => {
                        let captured = remaining;
                        remaining = "";
                        captured
                    }
                };

                insert_parsed_field(&mut fields, key, captured.to_owned())?;
            }
        }
    }

    if !remaining.is_empty() {
        return Err(parse_error(
            ParseStringErrorKind::TrailingInput,
            Some(text.len() - remaining.len()),
        ));
    }

    Ok(Value::Object(fields))
}

pub(crate) fn format_field(value: &Value, key: &str) -> String {
    let Some(field) = resolve_value(value, key) else {
        return String::new();
    };

    match field {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        value => value.to_string(),
    }
}

enum TemplatePart {
    Literal(String),
    Field(String),
}

fn parse_template(template: &str) -> Result<Vec<TemplatePart>> {
    let mut parts = Vec::new();
    let mut rest = template;

    while let Some(start) = rest.find('{') {
        if start > 0 {
            parts.push(TemplatePart::Literal(rest[..start].to_owned()));
        }

        let placeholder = &rest[start + 1..];
        let Some(end) = placeholder.find('}') else {
            return Err(parse_error(
                ParseStringErrorKind::UnmatchedOpeningBrace,
                Some(template.len() - rest.len() + start),
            ));
        };

        let key = &placeholder[..end];
        if key.is_empty() {
            return Err(parse_error(
                ParseStringErrorKind::EmptyPlaceholder,
                Some(template.len() - rest.len() + start),
            ));
        }

        parts.push(TemplatePart::Field(key.to_owned()));
        rest = &placeholder[end + 1..];
    }

    if !rest.is_empty() {
        parts.push(TemplatePart::Literal(rest.to_owned()));
    }

    Ok(parts)
}

fn insert_parsed_field(fields: &mut Map<String, Value>, key: &str, value: String) -> Result<()> {
    if key.starts_with('/') {
        insert_pointer_field(fields, key, Value::String(value))
    } else {
        fields.insert(key.to_owned(), Value::String(value));
        Ok(())
    }
}

fn insert_pointer_field(
    fields: &mut Map<String, Value>,
    pointer: &str,
    value: Value,
) -> Result<()> {
    let mut tokens = pointer_tokens(pointer).peekable();
    let Some(mut token) = tokens.next() else {
        return Err(parse_error(ParseStringErrorKind::EmptyPointer, None));
    };

    let mut current = fields;

    while tokens.peek().is_some() {
        let entry = current
            .entry(token)
            .or_insert_with(|| Value::Object(Map::new()));

        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }

        current = entry
            .as_object_mut()
            .expect("entry was just ensured to be an object");
        token = tokens.next().expect("peek confirmed a next token");
    }

    current.insert(token, value);
    Ok(())
}

fn parse_error(kind: ParseStringErrorKind, position: Option<usize>) -> Error {
    Error::ParseString { kind, position }
}
