use std::io;

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

fn resolve_value<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    if key.starts_with('/') {
        value.pointer(key)
    } else {
        value.get(key)
    }
}

/// Expand `{field}` placeholders from a JSON value into a string.
///
/// Placeholders starting with `/` are resolved as JSON Pointers. Other
/// placeholders are resolved as top-level fields. Missing values, nulls, and
/// top-level fields on non-object inputs expand to empty strings. String values
/// are inserted directly; other JSON values are inserted using their compact
/// JSON representation.
///
/// ```
/// use jstream::format_value;
/// use serde_json::json;
///
/// let value = json!({
///     "first_name": "Ada",
///     "last_name": "Lovelace",
///     "site": {
///         "name": "corporate"
///     }
/// });
///
/// assert_eq!(
///     format_value(&value, "{first_name} {last_name} ({/site/name})"),
///     "Ada Lovelace (corporate)"
/// );
/// assert_eq!(format_value(&value, "{unknown}"), "");
/// ```
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

/// Ergonomic typed accessors for JSON values.
///
/// Keys starting with `/` are resolved as JSON Pointers. Other keys are
/// resolved as top-level fields. Missing values, nulls, and values of the wrong
/// type return the provided default.
///
/// ```
/// use jstream::JsonGet;
/// use serde_json::json;
///
/// let value = json!({
///     "name": "Ada",
///     "age": 37,
///     "metadata": {
///         "active": true
///     }
/// });
///
/// assert_eq!(value.get_str("name", "unknown"), "Ada");
/// assert_eq!(value.get_i64("age", 0), 37);
/// assert_eq!(value.get_bool("/metadata/active", false), true);
/// assert_eq!(value.get_str("missing", "unknown"), "unknown");
/// ```
pub trait JsonGet {
    /// Return a string field, or the provided default.
    fn get_str<'a>(&'a self, key: &str, default: &'a str) -> &'a str;

    /// Return an i64 field, or the provided default.
    fn get_i64(&self, key: &str, default: i64) -> i64;

    /// Return an f64 field, or the provided default.
    fn get_f64(&self, key: &str, default: f64) -> f64;

    /// Return a bool field, or the provided default.
    fn get_bool(&self, key: &str, default: bool) -> bool;

    /// Return an array field, or the provided default.
    fn get_array<'a>(&'a self, key: &str, default: &'a [Value]) -> &'a [Value];

    /// Return an object field, or the provided default.
    fn get_object<'a>(
        &'a self,
        key: &str,
        default: &'a Map<String, Value>,
    ) -> &'a Map<String, Value>;
}

impl JsonGet for Value {
    fn get_str<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        resolve_value(self, key)
            .and_then(Value::as_str)
            .unwrap_or(default)
    }

    fn get_i64(&self, key: &str, default: i64) -> i64 {
        resolve_value(self, key)
            .and_then(Value::as_i64)
            .unwrap_or(default)
    }

    fn get_f64(&self, key: &str, default: f64) -> f64 {
        resolve_value(self, key)
            .and_then(Value::as_f64)
            .unwrap_or(default)
    }

    fn get_bool(&self, key: &str, default: bool) -> bool {
        resolve_value(self, key)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    fn get_array<'a>(&'a self, key: &str, default: &'a [Value]) -> &'a [Value] {
        resolve_value(self, key)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(default)
    }

    fn get_object<'a>(
        &'a self,
        key: &str,
        default: &'a Map<String, Value>,
    ) -> &'a Map<String, Value> {
        resolve_value(self, key)
            .and_then(Value::as_object)
            .unwrap_or(default)
    }
}

/// Fluent transformations over JSON values.
///
/// The trait is implemented for both [`Value`] and [`serde_json::Result<Value>`],
/// so a parsed value can be transformed with a single `?` at the end of the
/// chain:
///
/// ```
/// use jstream::JsonStream;
/// use serde_json::{Value, json};
///
/// # fn run() -> serde_json::Result<Value> {
/// let value = serde_json::from_str::<Value>(r#"{"name":"Ada","age":37}"#)
///     .require_shape(json!({ "name": "", "age": 0 }))
///     .select(["name", "date", "age"])
///     .merge(serde_json::json!({ "site": "corporate", "age": 38 }))
///     .add("site", "corporate")?;
/// # Ok(value)
/// # }
/// ```
pub trait JsonStream {
    /// Keep only the requested fields.
    ///
    /// Missing fields are included with a JSON `null` value. Non-object inputs
    /// are treated as empty objects.
    fn select<K, I>(self, keys: I) -> serde_json::Result<Value>
    where
        K: AsRef<str>,
        I: IntoIterator<Item = K>;

    /// Add or replace a field with any value that can be serialized to JSON.
    ///
    /// Non-object inputs are converted into a new object containing the added
    /// field.
    fn add<K, V>(self, key: K, value: V) -> serde_json::Result<Value>
    where
        K: Into<String>,
        V: Serialize;

    /// Merge fields from another object without replacing existing fields.
    ///
    /// This is a shallow merge. Non-object inputs are treated as empty objects.
    fn merge<V>(self, value: V) -> serde_json::Result<Value>
    where
        V: Serialize;

    /// Calculate fields from the current value and merge them without replacing
    /// existing fields.
    fn eval<F>(self, f: F) -> serde_json::Result<Value>
    where
        F: FnOnce(&Value) -> Value;

    /// Validate the current value by attempting to deserialize it as `T`.
    ///
    /// The original JSON value is returned unchanged when validation succeeds.
    fn validate_as<T>(self) -> serde_json::Result<Value>
    where
        T: DeserializeOwned;

    /// Require that the current value contains at least the fields and JSON
    /// types present in the example shape.
    ///
    /// Extra fields are allowed. Object shapes recurse. Array shapes only
    /// require that the input value is an array.
    fn require_shape<V>(self, shape: V) -> serde_json::Result<Value>
    where
        V: Serialize;
}

impl JsonStream for Value {
    fn select<K, I>(self, keys: I) -> serde_json::Result<Value>
    where
        K: AsRef<str>,
        I: IntoIterator<Item = K>,
    {
        let source = match self {
            Value::Object(object) => object,
            _ => Map::new(),
        };

        let selected = keys
            .into_iter()
            .map(|key| {
                let key = key.as_ref().to_owned();
                let value = source.get(&key).cloned().unwrap_or(Value::Null);
                (key, value)
            })
            .collect();

        Ok(Value::Object(selected))
    }

    fn add<K, V>(self, key: K, value: V) -> serde_json::Result<Value>
    where
        K: Into<String>,
        V: Serialize,
    {
        let mut object = match self {
            Value::Object(object) => object,
            _ => Map::new(),
        };

        object.insert(key.into(), serde_json::to_value(value)?);
        Ok(Value::Object(object))
    }

    fn merge<V>(self, value: V) -> serde_json::Result<Value>
    where
        V: Serialize,
    {
        merge_values(self, serde_json::to_value(value)?)
    }

    fn eval<F>(self, f: F) -> serde_json::Result<Value>
    where
        F: FnOnce(&Value) -> Value,
    {
        let fields = f(&self);
        merge_values(self, fields)
    }

    fn validate_as<T>(self) -> serde_json::Result<Value>
    where
        T: DeserializeOwned,
    {
        serde_json::from_value::<T>(self.clone())?;
        Ok(self)
    }

    fn require_shape<V>(self, shape: V) -> serde_json::Result<Value>
    where
        V: Serialize,
    {
        let shape = serde_json::to_value(shape)?;
        require_shape_value(&self, &shape)?;
        Ok(self)
    }
}

impl JsonStream for serde_json::Result<Value> {
    fn select<K, I>(self, keys: I) -> serde_json::Result<Value>
    where
        K: AsRef<str>,
        I: IntoIterator<Item = K>,
    {
        self.and_then(|value| value.select(keys))
    }

    fn add<K, V>(self, key: K, value: V) -> serde_json::Result<Value>
    where
        K: Into<String>,
        V: Serialize,
    {
        self.and_then(|json| json.add(key, value))
    }

    fn merge<V>(self, value: V) -> serde_json::Result<Value>
    where
        V: Serialize,
    {
        self.and_then(|json| json.merge(value))
    }

    fn eval<F>(self, f: F) -> serde_json::Result<Value>
    where
        F: FnOnce(&Value) -> Value,
    {
        self.and_then(|json| json.eval(f))
    }

    fn validate_as<T>(self) -> serde_json::Result<Value>
    where
        T: DeserializeOwned,
    {
        self.and_then(|json| json.validate_as::<T>())
    }

    fn require_shape<V>(self, shape: V) -> serde_json::Result<Value>
    where
        V: Serialize,
    {
        self.and_then(|json| json.require_shape(shape))
    }
}

fn merge_values(base: Value, fields: Value) -> serde_json::Result<Value> {
    let mut object = match base {
        Value::Object(object) => object,
        _ => Map::new(),
    };

    if let Value::Object(fields) = fields {
        for (key, value) in fields {
            object.entry(key).or_insert(value);
        }
    }

    Ok(Value::Object(object))
}

fn format_field(value: &Value, key: &str) -> String {
    let Some(field) = resolve_value(value, key) else {
        return String::new();
    };

    match field {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        value => value.to_string(),
    }
}

fn require_shape_value(value: &Value, shape: &Value) -> serde_json::Result<()> {
    require_shape_at(value, shape, "$").map_err(shape_error)
}

fn require_shape_at(value: &Value, shape: &Value, path: &str) -> Result<(), String> {
    match shape {
        Value::Object(shape_object) => {
            let Some(value_object) = value.as_object() else {
                return Err(format!(
                    "{path}: expected object, found {}",
                    json_kind(value)
                ));
            };

            for (key, expected) in shape_object {
                let field_path = format!("{path}.{key}");
                let Some(actual) = value_object.get(key) else {
                    return Err(format!("{field_path}: missing required field"));
                };

                require_shape_at(actual, expected, &field_path)?;
            }

            Ok(())
        }
        Value::Array(_) => {
            if value.is_array() {
                Ok(())
            } else {
                Err(format!(
                    "{path}: expected array, found {}",
                    json_kind(value)
                ))
            }
        }
        expected => {
            if same_json_kind(value, expected) {
                Ok(())
            } else {
                Err(format!(
                    "{path}: expected {}, found {}",
                    json_kind(expected),
                    json_kind(value)
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

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn shape_error(message: String) -> serde_json::Error {
    serde_json::Error::io(io::Error::new(io::ErrorKind::InvalidData, message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[test]
    fn select_keeps_requested_fields_and_fills_missing_ones() {
        let value = json!({
            "name": "Ada",
            "age": 37,
            "ignored": true
        });

        let selected = value.select(["name", "date", "age"]).unwrap();

        assert_eq!(
            selected,
            json!({
                "name": "Ada",
                "date": null,
                "age": 37
            })
        );
    }

    #[test]
    fn select_treats_non_objects_as_empty_objects() {
        let selected = json!(42).select(["name"]).unwrap();

        assert_eq!(selected, json!({ "name": null }));
    }

    #[test]
    fn add_accepts_serializable_values() {
        #[derive(Serialize)]
        struct Site {
            name: String,
            active: bool,
        }

        let value = json!({ "name": "Ada" })
            .add(
                "site",
                Site {
                    name: "corporate".to_owned(),
                    active: true,
                },
            )
            .unwrap();

        assert_eq!(
            value,
            json!({
                "name": "Ada",
                "site": {
                    "name": "corporate",
                    "active": true
                }
            })
        );
    }

    #[test]
    fn add_treats_non_objects_as_empty_objects() {
        let value = json!(42).add("site", "corporate").unwrap();

        assert_eq!(value, json!({ "site": "corporate" }));
    }

    #[test]
    fn merge_adds_fields_without_overriding_existing_fields() {
        let value = json!({
            "name": "Ada",
            "site": "original"
        })
        .merge(json!({
            "site": "corporate",
            "age": 37
        }))
        .unwrap();

        assert_eq!(
            value,
            json!({
                "name": "Ada",
                "site": "original",
                "age": 37
            })
        );
    }

    #[test]
    fn merge_treats_non_object_inputs_as_empty_objects() {
        let value = json!(42).merge(json!({ "site": "corporate" })).unwrap();

        assert_eq!(value, json!({ "site": "corporate" }));
    }

    #[test]
    fn merge_ignores_non_object_fields() {
        let value = json!({ "name": "Ada" }).merge(json!(42)).unwrap();

        assert_eq!(value, json!({ "name": "Ada" }));
    }

    #[test]
    fn eval_calculates_and_merges_fields_without_overriding_existing_fields() {
        let value = json!({
            "first_name": "Ada",
            "last_name": "Lovelace",
            "display_name": "existing"
        })
        .eval(|value| {
            json!({
                "display_name": format_value(value, "{first_name} {last_name}"),
                "slug": format_value(value, "{first_name}-{last_name}").to_lowercase()
            })
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "first_name": "Ada",
                "last_name": "Lovelace",
                "display_name": "existing",
                "slug": "ada-lovelace"
            })
        );
    }

    #[test]
    fn parsed_results_can_be_transformed_without_intermediate_question_marks() {
        let value = serde_json::from_str::<Value>(r#"{"name":"Ada","age":37}"#)
            .require_shape(json!({ "name": "", "age": 0 }))
            .select(["name", "date", "age"])
            .merge(json!({ "age": 38, "site": "corporate" }))
            .eval(|value| json!({ "slug": format_value(value, "{name}").to_lowercase() }))
            .add("site", "corporate")
            .unwrap();

        assert_eq!(
            value,
            json!({
                "name": "Ada",
                "date": null,
                "age": 37,
                "site": "corporate",
                "slug": "ada"
            })
        );
    }

    #[test]
    fn validate_as_returns_original_value_when_deserialization_succeeds() {
        #[allow(dead_code)]
        #[derive(Deserialize)]
        struct Input {
            name: String,
            age: i64,
        }

        let value = json!({
            "name": "Ada",
            "age": 37,
            "extra": true
        });

        let validated = value.clone().validate_as::<Input>().unwrap();

        assert_eq!(validated, value);
    }

    #[test]
    fn validate_as_returns_deserialization_errors() {
        #[allow(dead_code)]
        #[derive(Deserialize)]
        struct Input {
            name: String,
            age: i64,
        }

        let error = json!({
            "name": "Ada",
            "age": "old"
        })
        .validate_as::<Input>()
        .unwrap_err();

        assert!(error.to_string().contains("invalid type"));
    }

    #[test]
    fn require_shape_returns_original_value_when_shape_matches() {
        let value = json!({
            "name": "Ada",
            "age": 37,
            "active": true,
            "metadata": {
                "department": "research",
                "ignored": true
            },
            "tags": ["math", "code"],
            "extra": false
        });

        let validated = value
            .clone()
            .require_shape(json!({
                "name": "",
                "age": 0,
                "active": false,
                "metadata": {
                    "department": ""
                },
                "tags": []
            }))
            .unwrap();

        assert_eq!(validated, value);
    }

    #[test]
    fn require_shape_fails_for_missing_fields() {
        let error = json!({
            "name": "Ada"
        })
        .require_shape(json!({
            "name": "",
            "age": 0
        }))
        .unwrap_err();

        assert!(error.to_string().contains("$.age: missing required field"));
    }

    #[test]
    fn require_shape_fails_for_wrong_field_types() {
        let error = json!({
            "name": "Ada",
            "age": "old"
        })
        .require_shape(json!({
            "name": "",
            "age": 0
        }))
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("$.age: expected number, found string")
        );
    }

    #[test]
    fn require_shape_recurses_through_objects() {
        let error = json!({
            "metadata": {
                "department": 42
            }
        })
        .require_shape(json!({
            "metadata": {
                "department": ""
            }
        }))
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("$.metadata.department: expected string, found number")
        );
    }

    #[test]
    fn require_shape_requires_arrays_but_not_array_element_shapes() {
        let value = json!({
            "tags": [1, true, "mixed"]
        });

        assert_eq!(
            value
                .clone()
                .require_shape(json!({ "tags": [""] }))
                .unwrap(),
            value
        );

        let error = json!({ "tags": "not an array" })
            .require_shape(json!({ "tags": [] }))
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("$.tags: expected array, found string")
        );
    }

    #[test]
    fn format_value_expands_string_fields() {
        let value = json!({
            "first_name": "Ada",
            "last_name": "Lovelace"
        });

        assert_eq!(
            format_value(&value, "{first_name} {last_name}"),
            "Ada Lovelace"
        );
    }

    #[test]
    fn format_value_expands_unknown_and_null_fields_as_empty_strings() {
        let value = json!({
            "name": "Ada",
            "nickname": null
        });

        assert_eq!(format_value(&value, "{name}:{unknown}:{nickname}"), "Ada::");
    }

    #[test]
    fn format_value_expands_non_string_fields_as_json() {
        let value = json!({
            "age": 37,
            "active": true,
            "tags": ["math", "code"]
        });

        assert_eq!(
            format_value(&value, "{age}:{active}:{tags}"),
            r#"37:true:["math","code"]"#
        );
    }

    #[test]
    fn format_value_expands_json_pointer_fields() {
        let value = json!({
            "name": {
                "first": "Ada",
                "last": "Lovelace"
            },
            "sites": ["personal", "corporate"]
        });

        assert_eq!(
            format_value(&value, "{/name/first} {/name/last}:{/sites/1}"),
            "Ada Lovelace:corporate"
        );
    }

    #[test]
    fn format_value_expands_unknown_and_null_json_pointers_as_empty_strings() {
        let value = json!({
            "name": {
                "first": null
            }
        });

        assert_eq!(format_value(&value, "{/missing}{/name/first}"), "");
    }

    #[test]
    fn format_value_supports_escaped_json_pointer_tokens() {
        let value = json!({
            "a/b": {
                "c~d": "found"
            }
        });

        assert_eq!(format_value(&value, "{/a~1b/c~0d}"), "found");
    }

    #[test]
    fn format_value_leaves_unmatched_opening_braces_literal() {
        let value = json!({ "name": "Ada" });

        assert_eq!(format_value(&value, "Hello {name"), "Hello {name");
    }

    #[test]
    fn typed_getters_return_fields() {
        let value = json!({
            "name": "Ada",
            "age": 37,
            "score": 99.5,
            "active": true
        });

        assert_eq!(value.get_str("name", "unknown"), "Ada");
        assert_eq!(value.get_i64("age", 0), 37);
        assert_eq!(value.get_f64("score", 0.0), 99.5);
        assert_eq!(value.get_bool("active", false), true);
    }

    #[test]
    fn typed_getters_return_defaults_for_missing_null_and_wrong_type_fields() {
        let value = json!({
            "name": null,
            "age": "old",
            "score": false,
            "active": 1
        });

        assert_eq!(value.get_str("name", "unknown"), "unknown");
        assert_eq!(value.get_i64("age", 0), 0);
        assert_eq!(value.get_f64("score", 1.5), 1.5);
        assert_eq!(value.get_bool("active", false), false);
        assert_eq!(json!(42).get_str("name", "unknown"), "unknown");
    }

    #[test]
    fn typed_getters_support_json_pointer_keys() {
        let value = json!({
            "metadata": {
                "name": "Ada",
                "active": true
            }
        });

        assert_eq!(value.get_str("/metadata/name", "unknown"), "Ada");
        assert_eq!(value.get_bool("/metadata/active", false), true);
        assert_eq!(value.get_str("/metadata/missing", "unknown"), "unknown");
    }

    #[test]
    fn collection_getters_return_fields_or_defaults() {
        let value = json!({
            "items": [1, 2],
            "metadata": {
                "source": "archive"
            }
        });
        let default_array = vec![json!("default")];
        let mut default_object = Map::new();
        default_object.insert("default".to_owned(), json!(true));

        assert_eq!(
            value.get_array("items", &default_array),
            &[json!(1), json!(2)]
        );
        assert_eq!(
            value.get_object("metadata", &default_object).get("source"),
            Some(&json!("archive"))
        );
        assert_eq!(value.get_array("missing", &default_array), default_array);
        assert_eq!(
            value.get_object("missing", &default_object),
            &default_object
        );
    }
}
