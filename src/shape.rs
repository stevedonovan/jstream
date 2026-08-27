use serde_json::{Map, Value};

use crate::{Error, JsonKind, Result};

pub(crate) fn require_shape_value(value: &Value, shape: &Value) -> Result<()> {
    require_shape_at(value, shape, "$")
}

pub(crate) fn select_shape_value(value: &Value, shape: &Value) -> Value {
    match (value, shape) {
        (Value::Object(value_object), Value::Object(shape_object)) => {
            let mut selected = Map::new();

            for (key, shape_value) in shape_object {
                if let Some(value) = value_object.get(key) {
                    selected.insert(key.clone(), select_shape_value(value, shape_value));
                }
            }

            Value::Object(selected)
        }
        _ => value.clone(),
    }
}

fn require_shape_at(value: &Value, shape: &Value, path: &str) -> Result<()> {
    match shape {
        Value::Object(shape_object) => {
            let Some(value_object) = value.as_object() else {
                return Err(shape_error(path, JsonKind::Object, Some(json_kind(value))));
            };

            for (key, expected) in shape_object {
                let field_path = format!("{path}.{key}");
                let Some(actual) = value_object.get(key) else {
                    return Err(shape_error(&field_path, json_kind(expected), None));
                };

                require_shape_at(actual, expected, &field_path)?;
            }

            Ok(())
        }
        Value::Array(_) => {
            if value.is_array() {
                Ok(())
            } else {
                Err(shape_error(path, JsonKind::Array, Some(json_kind(value))))
            }
        }
        expected => {
            if same_json_kind(value, expected) {
                Ok(())
            } else {
                Err(shape_error(
                    path,
                    json_kind(expected),
                    Some(json_kind(value)),
                ))
            }
        }
    }
}

fn same_json_kind(left: &Value, right: &Value) -> bool {
    matches!(
        (left, right),
        (Value::Null, Value::Null)
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Array(_), Value::Array(_))
            | (Value::Object(_), Value::Object(_))
    )
}

fn json_kind(value: &Value) -> JsonKind {
    match value {
        Value::Null => JsonKind::Null,
        Value::Bool(_) => JsonKind::Bool,
        Value::Number(_) => JsonKind::Number,
        Value::String(_) => JsonKind::String,
        Value::Array(_) => JsonKind::Array,
        Value::Object(_) => JsonKind::Object,
    }
}

fn shape_error(path: &str, expected: JsonKind, actual: Option<JsonKind>) -> Error {
    Error::Shape {
        path: path.to_owned(),
        expected,
        actual,
    }
}
