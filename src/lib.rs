//! A small fluent-style API for transforming [`serde_json::Value`] in ETL-style
//! pipelines.
//!
//! `jstream` handles common JSON object shaping, validation, field access,
//! formatting, and simple text parsing. For domain-specific work such as dates,
//! regex parsing, or normalization rules, use [`JsonStream::try_map`] to drop in
//! ordinary Rust functions and your preferred libraries.
//!
//! ```
//! use jstream::{JsonGet, JsonStream, Value, format_value, json, value_from_str};
//!
//! let text = r#"{
//!     "raw": "Ada Lovelace (research)",
//!     "date": "1843-09-01",
//!     "age": 27,
//!     "internal_id": "hidden"
//! }"#;
//!
//! let value = value_from_str(text)?
//!     .parse_text("raw", "{name} ({/metadata/department})")
//!     .require_shape(json!({
//!         "name": "",
//!         "age": 0,
//!         "metadata": {
//!             "department": ""
//!         }
//!     }))
//!     .select(["name", "date", "age", "metadata", "raw"])
//!     .merge(json!({ "site": "corporate", "age": 0 }))
//!     .merge_eval(|value| {
//!         json!({
//!             "display_name": format_value(value, "{name} ({/metadata/department})"),
//!             "slug": format_value(value, "{name}").to_lowercase(),
//!             "adult": value.get_i64("age", 0) >= 18
//!         })
//!     })
//!     .copy("name", "display_name")
//!     .rename("name", "/person/name")
//!     .delete("date")
//!     .delete("raw")
//!     .add("site", "corporate")?;
//!
//! assert_eq!(
//!     value,
//!     json!({
//!         "adult": true,
//!         "age": 27,
//!         "display_name": "Ada Lovelace",
//!         "metadata": {
//!             "department": "research"
//!         },
//!         "person": {
//!             "name": "Ada Lovelace"
//!         },
//!         "site": "corporate",
//!         "slug": "ada lovelace"
//!     })
//! );
//! # Ok::<(), jstream::Error>(())
//! ```
//!
//! [`JsonStream`] is implemented for [`Value`], [`Result<Value>`], and
//! [`serde_json::Result<Value>`], so parse errors and transform errors can flow
//! through a chain and be handled with one final `?`.
//!
//! Use [`format_value`] to expand `{field}` placeholders from JSON values, and
//! [`parse_string`] or [`JsonStream::parse_text`] to extract string fields using
//! the same notation. Placeholders starting with `/`, such as
//! `{/metadata/department}`, are resolved as JSON Pointers.
//!
//! [`JsonGet`] provides typed accessors with explicit defaults, such as
//! [`JsonGet::get_str`], [`JsonGet::get_i64`], and [`JsonGet::get_bool`].
//! Collection accessors also have option-returning forms:
//! [`JsonGet::get_array_opt`] and [`JsonGet::get_object_opt`].
//!
use std::{error, fmt};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Map;
pub use serde_json::{Value, json};

/// Result type used by jstream operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by jstream operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A serde_json parse, serialization, or deserialization error.
    Json(serde_json::Error),
    /// A value did not match the required example shape.
    Shape(String),
    /// Text did not match a `parse_string` template.
    ParseString(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Json(error) => write!(f, "{error}"),
            Error::Shape(message) => write!(f, "{message}"),
            Error::ParseString(message) => write!(f, "{message}"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Error::Json(error) => Some(error),
            Error::Shape(_) | Error::ParseString(_) => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error::Json(error)
    }
}

fn resolve_value<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    if key.starts_with('/') {
        value.pointer(key)
    } else {
        value.get(key)
    }
}

/// A convenient wrapper around `serde_json::from_str::<Value>(text)`
pub fn value_from_str(s: &str) -> Result<Value> {
    let val = serde_json::from_str(s)?;
    Ok(val)
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

/// Extract string fields from text using the same `{field}` placeholder syntax
/// as [`format_value`].
///
/// Placeholders starting with `/` are treated as JSON Pointer output paths.
/// Other placeholders are written as top-level object fields. Literal template
/// text must match the input in order, and each placeholder captures the text
/// between surrounding literal segments.
///
/// ```
/// use jstream::parse_string;
/// use serde_json::json;
///
/// let value = parse_string("Ada Lovelace (corporate)", "{first_name} {last_name} ({/site/name})")?;
///
/// assert_eq!(
///     value,
///     json!({
///         "first_name": "Ada",
///         "last_name": "Lovelace",
///         "site": {
///             "name": "corporate"
///         }
///     })
/// );
/// # Ok::<(), jstream::Error>(())
/// ```
pub fn parse_string(text: &str, template: &str) -> Result<Value> {
    let template = parse_template(template)?;
    let mut remaining = text;
    let mut fields = Map::new();

    for index in 0..template.len() {
        match &template[index] {
            TemplatePart::Literal(literal) => {
                let Some(next) = remaining.strip_prefix(literal) else {
                    return Err(parse_error(format!(
                        "expected literal {literal:?} at {remaining:?}"
                    )));
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
                            return Err(parse_error(format!(
                                "expected literal {literal:?} after field {key:?}"
                            )));
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
        return Err(parse_error(format!(
            "unparsed trailing input {remaining:?}"
        )));
    }

    Ok(Value::Object(fields))
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

    /// Return an array field.
    fn get_array_opt(&self, key: &str) -> Option<&[Value]>;

    /// Return an object field, or the provided default.
    fn get_object<'a>(
        &'a self,
        key: &str,
        default: &'a Map<String, Value>,
    ) -> &'a Map<String, Value>;

    /// Return an object field.
    fn get_object_opt(&self, key: &str) -> Option<&Map<String, Value>>;
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
        self.get_array_opt(key).unwrap_or(default)
    }

    fn get_array_opt(&self, key: &str) -> Option<&[Value]> {
        resolve_value(self, key)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
    }

    fn get_object<'a>(
        &'a self,
        key: &str,
        default: &'a Map<String, Value>,
    ) -> &'a Map<String, Value> {
        self.get_object_opt(key).unwrap_or(default)
    }

    fn get_object_opt(&self, key: &str) -> Option<&Map<String, Value>> {
        resolve_value(self, key).and_then(Value::as_object)
    }
}

/// Fluent transformations over JSON values.
///
/// The trait is implemented for [`Value`], [`Result<Value>`], and
/// [`serde_json::Result<Value>`],
/// so a parsed value can be transformed with a single `?` at the end of the
/// chain:
///
/// ```
/// use jstream::{JsonGet, JsonStream};
/// use serde_json::{Value, json};
///
/// # fn run() -> jstream::Result<Value> {
/// let value = serde_json::from_str::<Value>(r#"{"name":"Ada","age":37}"#)
///     .require_shape(json!({ "name": "", "age": 0 }))
///     .select(["name", "date", "age"])
///     .merge(serde_json::json!({ "site": "corporate", "age": 38 }))
///     .merge_eval(|value| serde_json::json!({ "slug": value.get_str("name", "").to_lowercase() }))
///     .delete("date")
///     .add("site", "corporate")?;
/// # Ok(value)
/// # }
/// ```
pub trait JsonStream {
    /// Keep only the requested fields.
    ///
    /// Missing fields are included with a JSON `null` value. Non-object inputs
    /// are treated as empty objects.
    fn select<K, I>(self, keys: I) -> Result<Value>
    where
        K: AsRef<str>,
        I: IntoIterator<Item = K>;

    /// Add or replace a field with any value that can be serialized to JSON.
    ///
    /// Non-object inputs are converted into a new object containing the added
    /// field.
    fn add<K, V>(self, key: K, value: V) -> Result<Value>
    where
        K: Into<String>,
        V: Serialize;

    /// Merge fields from another object without replacing existing fields.
    ///
    /// This is a shallow merge. Non-object inputs are treated as empty objects.
    fn merge<V>(self, value: V) -> Result<Value>
    where
        V: Serialize;

    /// Calculate fields from the current value and merge them without replacing
    /// existing fields.
    fn merge_eval<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Value;

    /// Parse a string field with [`parse_string`] and merge the extracted
    /// fields without replacing existing fields.
    ///
    /// The field can be a top-level key or JSON Pointer path. Missing, null, or
    /// non-string fields are parsed as an empty string.
    fn parse_text<K>(self, key: K, template: &str) -> Result<Value>
    where
        K: AsRef<str>;

    /// Transform the current value with a fallible function.
    fn try_map<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Result<Value>;

    /// Delete a field or JSON Pointer path.
    ///
    /// Keys starting with `/` are resolved as JSON Pointers. Other keys delete
    /// top-level object fields. Missing paths are no-ops.
    fn delete<K>(self, key: K) -> Result<Value>
    where
        K: AsRef<str>;

    /// Move a field or JSON Pointer path to a new field or path.
    ///
    /// Missing sources are no-ops. Existing targets are overwritten.
    fn rename<F, T>(self, from: F, to: T) -> Result<Value>
    where
        F: AsRef<str>,
        T: AsRef<str>;

    /// Copy a field or JSON Pointer path to a new field or path.
    ///
    /// Missing sources are no-ops. Existing targets are overwritten.
    fn copy<F, T>(self, from: F, to: T) -> Result<Value>
    where
        F: AsRef<str>,
        T: AsRef<str>;

    /// Validate the current value by attempting to deserialize it as `T`.
    ///
    /// The original JSON value is returned unchanged when validation succeeds.
    fn validate_as<T>(self) -> Result<Value>
    where
        T: DeserializeOwned;

    /// Require that the current value contains at least the fields and JSON
    /// types present in the example shape.
    ///
    /// Extra fields are allowed. Object shapes recurse. Array shapes only
    /// require that the input value is an array.
    fn require_shape<V>(self, shape: V) -> Result<Value>
    where
        V: Serialize;

    /// Require that the current value matches the example shape, then return
    /// only fields present in that shape.
    fn select_shape<V>(self, shape: V) -> Result<Value>
    where
        V: Serialize;
}

impl JsonStream for Value {
    fn select<K, I>(self, keys: I) -> Result<Value>
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

    fn add<K, V>(self, key: K, value: V) -> Result<Value>
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

    fn merge<V>(self, value: V) -> Result<Value>
    where
        V: Serialize,
    {
        merge_values(self, serde_json::to_value(value)?)
    }

    fn merge_eval<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Value,
    {
        let fields = f(&self);
        merge_values(self, fields)
    }

    fn parse_text<K>(self, key: K, template: &str) -> Result<Value>
    where
        K: AsRef<str>,
    {
        let parsed = parse_string(format_field(&self, key.as_ref()).as_str(), template)?;
        merge_values(self, parsed)
    }

    fn try_map<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Result<Value>,
    {
        f(self)
    }

    fn delete<K>(self, key: K) -> Result<Value>
    where
        K: AsRef<str>,
    {
        Ok(delete_value(self, key.as_ref()))
    }

    fn rename<F, T>(self, from: F, to: T) -> Result<Value>
    where
        F: AsRef<str>,
        T: AsRef<str>,
    {
        Ok(rename_value(self, from.as_ref(), to.as_ref()))
    }

    fn copy<F, T>(self, from: F, to: T) -> Result<Value>
    where
        F: AsRef<str>,
        T: AsRef<str>,
    {
        Ok(copy_value(self, from.as_ref(), to.as_ref()))
    }

    fn validate_as<T>(self) -> Result<Value>
    where
        T: DeserializeOwned,
    {
        serde_json::from_value::<T>(self.clone())?;
        Ok(self)
    }

    fn require_shape<V>(self, shape: V) -> Result<Value>
    where
        V: Serialize,
    {
        let shape = serde_json::to_value(shape)?;
        require_shape_value(&self, &shape)?;
        Ok(self)
    }

    fn select_shape<V>(self, shape: V) -> Result<Value>
    where
        V: Serialize,
    {
        let shape = serde_json::to_value(shape)?;
        require_shape_value(&self, &shape)?;
        Ok(select_shape_value(&self, &shape))
    }
}

impl JsonStream for Result<Value> {
    fn select<K, I>(self, keys: I) -> Result<Value>
    where
        K: AsRef<str>,
        I: IntoIterator<Item = K>,
    {
        self.and_then(|value| value.select(keys))
    }

    fn add<K, V>(self, key: K, value: V) -> Result<Value>
    where
        K: Into<String>,
        V: Serialize,
    {
        self.and_then(|json| json.add(key, value))
    }

    fn merge<V>(self, value: V) -> Result<Value>
    where
        V: Serialize,
    {
        self.and_then(|json| json.merge(value))
    }

    fn merge_eval<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Value,
    {
        self.and_then(|json| json.merge_eval(f))
    }

    fn parse_text<K>(self, key: K, template: &str) -> Result<Value>
    where
        K: AsRef<str>,
    {
        self.and_then(|json| json.parse_text(key, template))
    }

    fn try_map<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Result<Value>,
    {
        self.and_then(f)
    }

    fn delete<K>(self, key: K) -> Result<Value>
    where
        K: AsRef<str>,
    {
        self.and_then(|json| json.delete(key))
    }

    fn rename<F, T>(self, from: F, to: T) -> Result<Value>
    where
        F: AsRef<str>,
        T: AsRef<str>,
    {
        self.and_then(|json| json.rename(from, to))
    }

    fn copy<F, T>(self, from: F, to: T) -> Result<Value>
    where
        F: AsRef<str>,
        T: AsRef<str>,
    {
        self.and_then(|json| json.copy(from, to))
    }

    fn validate_as<T>(self) -> Result<Value>
    where
        T: DeserializeOwned,
    {
        self.and_then(|json| json.validate_as::<T>())
    }

    fn require_shape<V>(self, shape: V) -> Result<Value>
    where
        V: Serialize,
    {
        self.and_then(|json| json.require_shape(shape))
    }

    fn select_shape<V>(self, shape: V) -> Result<Value>
    where
        V: Serialize,
    {
        self.and_then(|json| json.select_shape(shape))
    }
}

impl JsonStream for serde_json::Result<Value> {
    fn select<K, I>(self, keys: I) -> Result<Value>
    where
        K: AsRef<str>,
        I: IntoIterator<Item = K>,
    {
        match self {
            Ok(value) => value.select(keys),
            Err(error) => Err(error.into()),
        }
    }

    fn add<K, V>(self, key: K, value: V) -> Result<Value>
    where
        K: Into<String>,
        V: Serialize,
    {
        match self {
            Ok(json) => json.add(key, value),
            Err(error) => Err(error.into()),
        }
    }

    fn merge<V>(self, value: V) -> Result<Value>
    where
        V: Serialize,
    {
        match self {
            Ok(json) => json.merge(value),
            Err(error) => Err(error.into()),
        }
    }

    fn merge_eval<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Value,
    {
        match self {
            Ok(json) => json.merge_eval(f),
            Err(error) => Err(error.into()),
        }
    }

    fn parse_text<K>(self, key: K, template: &str) -> Result<Value>
    where
        K: AsRef<str>,
    {
        match self {
            Ok(json) => json.parse_text(key, template),
            Err(error) => Err(error.into()),
        }
    }

    fn try_map<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Result<Value>,
    {
        match self {
            Ok(json) => f(json),
            Err(error) => Err(error.into()),
        }
    }

    fn delete<K>(self, key: K) -> Result<Value>
    where
        K: AsRef<str>,
    {
        match self {
            Ok(json) => json.delete(key),
            Err(error) => Err(error.into()),
        }
    }

    fn rename<F, T>(self, from: F, to: T) -> Result<Value>
    where
        F: AsRef<str>,
        T: AsRef<str>,
    {
        match self {
            Ok(json) => json.rename(from, to),
            Err(error) => Err(error.into()),
        }
    }

    fn copy<F, T>(self, from: F, to: T) -> Result<Value>
    where
        F: AsRef<str>,
        T: AsRef<str>,
    {
        match self {
            Ok(json) => json.copy(from, to),
            Err(error) => Err(error.into()),
        }
    }

    fn validate_as<T>(self) -> Result<Value>
    where
        T: DeserializeOwned,
    {
        match self {
            Ok(json) => json.validate_as::<T>(),
            Err(error) => Err(error.into()),
        }
    }

    fn require_shape<V>(self, shape: V) -> Result<Value>
    where
        V: Serialize,
    {
        match self {
            Ok(json) => json.require_shape(shape),
            Err(error) => Err(error.into()),
        }
    }

    fn select_shape<V>(self, shape: V) -> Result<Value>
    where
        V: Serialize,
    {
        match self {
            Ok(json) => json.select_shape(shape),
            Err(error) => Err(error.into()),
        }
    }
}

fn merge_values(base: Value, fields: Value) -> Result<Value> {
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

fn delete_value(mut value: Value, key: &str) -> Value {
    if key.starts_with('/') {
        delete_pointer(&mut value, key);
        return value;
    }

    if let Value::Object(object) = &mut value {
        object.remove(key);
    }

    value
}

fn rename_value(mut value: Value, from: &str, to: &str) -> Value {
    let Some(field) = take_field(&mut value, from) else {
        return value;
    };

    insert_field(&mut value, to, field);
    value
}

fn copy_value(mut value: Value, from: &str, to: &str) -> Value {
    let Some(field) = resolve_value(&value, from).cloned() else {
        return value;
    };

    insert_field(&mut value, to, field);
    value
}

fn take_field(value: &mut Value, key: &str) -> Option<Value> {
    if key.starts_with('/') {
        take_pointer(value, key)
    } else {
        match value {
            Value::Object(object) => object.remove(key),
            _ => None,
        }
    }
}

fn insert_field(value: &mut Value, key: &str, field: Value) {
    if key.starts_with('/') {
        insert_pointer_value(value, key, field);
        return;
    }

    if !value.is_object() {
        *value = Value::Object(Map::new());
    }

    let object = value
        .as_object_mut()
        .expect("value was just ensured to be an object");
    object.insert(key.to_owned(), field);
}

fn take_pointer(value: &mut Value, pointer: &str) -> Option<Value> {
    let mut tokens = pointer
        .split('/')
        .skip(1)
        .map(unescape_pointer_token)
        .peekable();
    let mut token = tokens.next()?;
    let mut current = value;

    while tokens.peek().is_some() {
        current = match current {
            Value::Object(object) => object.get_mut(&token)?,
            Value::Array(array) => array.get_mut(token.parse::<usize>().ok()?)?,
            _ => return None,
        };

        token = tokens.next().expect("peek confirmed a next token");
    }

    match current {
        Value::Object(object) => object.remove(&token),
        Value::Array(array) => {
            let index = token.parse::<usize>().ok()?;
            if index < array.len() {
                Some(array.remove(index))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn insert_pointer_value(value: &mut Value, pointer: &str, field: Value) {
    let mut tokens = pointer
        .split('/')
        .skip(1)
        .map(unescape_pointer_token)
        .peekable();
    let Some(mut token) = tokens.next() else {
        *value = field;
        return;
    };

    if !value.is_object() {
        *value = Value::Object(Map::new());
    }

    let mut current = value
        .as_object_mut()
        .expect("value was just ensured to be an object");

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

    current.insert(token, field);
}

fn delete_pointer(value: &mut Value, pointer: &str) {
    let mut tokens = pointer
        .split('/')
        .skip(1)
        .map(unescape_pointer_token)
        .peekable();
    let Some(mut token) = tokens.next() else {
        return;
    };

    let mut current = value;

    while tokens.peek().is_some() {
        current = match current {
            Value::Object(object) => {
                let Some(next) = object.get_mut(&token) else {
                    return;
                };
                next
            }
            Value::Array(array) => {
                let Ok(index) = token.parse::<usize>() else {
                    return;
                };
                let Some(next) = array.get_mut(index) else {
                    return;
                };
                next
            }
            _ => return,
        };

        token = tokens.next().expect("peek confirmed a next token");
    }

    match current {
        Value::Object(object) => {
            object.remove(&token);
        }
        Value::Array(array) => {
            if let Ok(index) = token.parse::<usize>() {
                if index < array.len() {
                    array.remove(index);
                }
            }
        }
        _ => {}
    }
}

fn unescape_pointer_token(token: &str) -> String {
    let mut unescaped = String::new();
    let mut chars = token.chars();

    while let Some(ch) = chars.next() {
        if ch != '~' {
            unescaped.push(ch);
            continue;
        }

        match chars.next() {
            Some('0') => unescaped.push('~'),
            Some('1') => unescaped.push('/'),
            Some(ch) => {
                unescaped.push('~');
                unescaped.push(ch);
            }
            None => unescaped.push('~'),
        }
    }

    unescaped
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
            return Err(parse_error("unmatched opening brace in template"));
        };

        let key = &placeholder[..end];
        if key.is_empty() {
            return Err(parse_error("empty field placeholder in template"));
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
    let mut tokens = pointer
        .split('/')
        .skip(1)
        .map(unescape_pointer_token)
        .peekable();
    let Some(mut token) = tokens.next() else {
        return Err(parse_error("JSON Pointer field must not be empty"));
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

fn require_shape_value(value: &Value, shape: &Value) -> Result<()> {
    require_shape_at(value, shape, "$").map_err(shape_error)
}

fn select_shape_value(value: &Value, shape: &Value) -> Value {
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

fn require_shape_at(value: &Value, shape: &Value, path: &str) -> std::result::Result<(), String> {
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

fn shape_error(message: String) -> Error {
    Error::Shape(message)
}

fn parse_error(message: impl Into<String>) -> Error {
    Error::ParseString(message.into())
}

#[cfg(test)]
mod tests;
