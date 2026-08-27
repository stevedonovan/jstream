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
//! Use [`JsonStream::replace_eval`] when calculated fields should overwrite
//! existing fields, for example after parsing text fields that need numeric
//! conversion.
//!
use crate::{
    Result,
    access::resolve_value,
    shape::{require_shape_value, select_shape_value},
    template::{format_field, parse_string},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Map;
pub use serde_json::{Value, json};

/// A convenient wrapper around `serde_json::from_str::<Value>(text)`
pub fn value_from_str(s: &str) -> Result<Value> {
    let val = serde_json::from_str(s)?;
    Ok(val)
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
///     .replace_eval(|value| serde_json::json!({ "age": value.get_i64("age", 0) }))
///     .delete("date")
///     .add("site", "corporate")?;
/// # Ok(value)
/// # }
/// ```
pub trait JsonStream {
    /// Pass the current value through when `predicate` returns `true`, or
    /// replace it with JSON `null` when it returns `false`.
    ///
    /// JSON `null` is an absorbing pipeline value: subsequent [`JsonStream`]
    /// methods preserve it without invoking their closures.
    fn filter<F>(self, predicate: F) -> Self
    where
        Self: Sized,
        F: FnOnce(&Value) -> bool;

    /// Pass the current value through when a fallible `predicate` returns
    /// `Ok(true)`, replace it with JSON `null` for `Ok(false)`, or propagate
    /// the predicate error.
    fn try_filter<F>(self, predicate: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Result<bool>;

    /// Keep only the requested fields.
    ///
    /// Missing fields are included with a JSON `null` value. Non-null,
    /// non-object inputs are treated as empty objects. They may be renamed with
    /// `old as new`.
    /// Fields may be JSON pointers - without a rename, the new key becomes
    /// the last part of the pointer (so '/path/to/data' becomes 'data')
    fn select<K, I>(self, keys: I) -> Result<Value>
    where
        K: AsRef<str>,
        I: IntoIterator<Item = K>;

    /// Add or replace a field with any value that can be serialized to JSON.
    ///
    /// Non-null, non-object inputs are converted into a new object containing
    /// the added field.
    fn add<K, V>(self, key: K, value: V) -> Result<Value>
    where
        K: Into<String>,
        V: Serialize;

    /// Merge fields from another object without replacing existing fields.
    ///
    /// This is a shallow merge. Non-null, non-object inputs are treated as
    /// empty objects.
    fn merge<V>(self, value: V) -> Result<Value>
    where
        V: Serialize;

    /// Calculate fields from the current value and merge them without replacing
    /// existing fields.
    fn merge_eval<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Value;

    /// Calculate fields from the current value and merge them, replacing
    /// existing fields.
    fn replace_eval<F>(self, f: F) -> Result<Value>
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

    /// Transform the current value with an infallible function.
    fn transform<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Value;

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
    fn filter<F>(self, predicate: F) -> Self
    where
        F: FnOnce(&Value) -> bool,
    {
        if self.is_null() || predicate(&self) {
            self
        } else {
            Value::Null
        }
    }

    fn try_filter<F>(self, predicate: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Result<bool>,
    {
        if self.is_null() || predicate(&self)? {
            Ok(self)
        } else {
            Ok(Value::Null)
        }
    }

    fn select<K, I>(self, keys: I) -> Result<Value>
    where
        K: AsRef<str>,
        I: IntoIterator<Item = K>,
    {
        if self.is_null() {
            return Ok(self);
        }

        let selected = keys
            .into_iter()
            .map(|key| {
                let key = key.as_ref();
                let (key, out_key) = if let Some(idx) = key.find(" as ") {
                    (key[0..idx].trim(), key[idx + 4..].trim())
                } else {
                    if key.starts_with('/') {
                        let idx = key.rfind('/').unwrap();
                        let last = &key[idx + 1..];
                        (key, last)
                    } else {
                        (key, key)
                    }
                };
                let value = resolve_value(&self, key).cloned().unwrap_or(Value::Null);
                (out_key.to_owned(), value)
            })
            .collect();

        Ok(Value::Object(selected))
    }

    fn add<K, V>(self, key: K, value: V) -> Result<Value>
    where
        K: Into<String>,
        V: Serialize,
    {
        if self.is_null() {
            return Ok(self);
        }

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
        if self.is_null() {
            return Ok(self);
        }

        merge_values(self, serde_json::to_value(value)?)
    }

    fn merge_eval<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Value,
    {
        if self.is_null() {
            return Ok(self);
        }

        let fields = f(&self);
        merge_values(self, fields)
    }

    fn replace_eval<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Value,
    {
        if self.is_null() {
            return Ok(self);
        }

        let fields = f(&self);
        replace_values(self, fields)
    }

    fn parse_text<K>(self, key: K, template: &str) -> Result<Value>
    where
        K: AsRef<str>,
    {
        if self.is_null() {
            return Ok(self);
        }

        let parsed = parse_string(format_field(&self, key.as_ref()).as_str(), template)?;
        merge_values(self, parsed)
    }

    fn transform<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Value,
    {
        if self.is_null() {
            Ok(self)
        } else {
            Ok(f(self))
        }
    }

    fn try_map<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Result<Value>,
    {
        if self.is_null() { Ok(self) } else { f(self) }
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
        if self.is_null() {
            return Ok(self);
        }

        serde_json::from_value::<T>(self.clone())?;
        Ok(self)
    }

    fn require_shape<V>(self, shape: V) -> Result<Value>
    where
        V: Serialize,
    {
        if self.is_null() {
            return Ok(self);
        }

        let shape = serde_json::to_value(shape)?;
        require_shape_value(&self, &shape)?;
        Ok(self)
    }

    fn select_shape<V>(self, shape: V) -> Result<Value>
    where
        V: Serialize,
    {
        if self.is_null() {
            return Ok(self);
        }

        let shape = serde_json::to_value(shape)?;
        require_shape_value(&self, &shape)?;
        Ok(select_shape_value(&self, &shape))
    }
}

impl JsonStream for Result<Value> {
    fn filter<F>(self, predicate: F) -> Self
    where
        F: FnOnce(&Value) -> bool,
    {
        self.map(|value| value.filter(predicate))
    }

    fn try_filter<F>(self, predicate: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Result<bool>,
    {
        self.and_then(|value| value.try_filter(predicate))
    }

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

    fn replace_eval<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Value,
    {
        self.and_then(|json| json.replace_eval(f))
    }

    fn parse_text<K>(self, key: K, template: &str) -> Result<Value>
    where
        K: AsRef<str>,
    {
        self.and_then(|json| json.parse_text(key, template))
    }

    fn transform<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Value,
    {
        self.and_then(|json| json.transform(f))
    }

    fn try_map<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Result<Value>,
    {
        self.and_then(|json| json.try_map(f))
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
    fn filter<F>(self, predicate: F) -> Self
    where
        F: FnOnce(&Value) -> bool,
    {
        self.map(|value| value.filter(predicate))
    }

    fn try_filter<F>(self, predicate: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Result<bool>,
    {
        match self {
            Ok(value) => value.try_filter(predicate),
            Err(error) => Err(error.into()),
        }
    }

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

    fn replace_eval<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(&Value) -> Value,
    {
        match self {
            Ok(json) => json.replace_eval(f),
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

    fn transform<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Value,
    {
        match self {
            Ok(json) => json.transform(f),
            Err(error) => Err(error.into()),
        }
    }

    fn try_map<F>(self, f: F) -> Result<Value>
    where
        F: FnOnce(Value) -> Result<Value>,
    {
        match self {
            Ok(json) => json.try_map(f),
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

fn replace_values(base: Value, fields: Value) -> Result<Value> {
    let mut object = match base {
        Value::Object(object) => object,
        _ => Map::new(),
    };

    if let Value::Object(fields) = fields {
        for (key, value) in fields {
            object.insert(key, value);
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
            if let Ok(index) = token.parse::<usize>()
                && index < array.len()
            {
                array.remove(index);
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
