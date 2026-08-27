use serde_json::{Map, Value};

pub(crate) fn resolve_value<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    if key.starts_with('/') {
        value.pointer(key)
    } else {
        value.get(key)
    }
}

/// Ergonomic typed accessors for JSON values.
///
/// Keys starting with `/` are resolved as JSON Pointers. Other keys are
/// resolved as top-level fields. Missing values, nulls, and values of the wrong
/// type return the provided default.
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
