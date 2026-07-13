# jstream

`jstream` is a small fluent-style API for transforming `serde_json::Value`.

It handles common JSON object shaping, validation, field access,
formatting, and simple text parsing. For domain-specific work such as dates, regex parsing, or
normalization rules, use `try_map` to drop in ordinary Rust functions and your preferred libraries.

```rust
use jstream::{JsonGet, JsonStream, format_value, Value, json};

fn main() -> jstream::Result<()> {
    let text = r#"{
        "raw": "Ada Lovelace (research)",
        "date": "1843-09-01",
        "age": 27,
        "internal_id": "hidden"
    }"#;

    let value = serde_json::from_str::<Value>(text)
        .parse_text("raw", "{name} ({/metadata/department})")
        .require_shape(json!({
            "name": "",
            "age": 0,
            "metadata": {
                "department": ""
            }
        }))
        .select(["name", "date", "age", "metadata", "raw"])
        .merge(json!({ "site": "corporate", "age": 0 }))
        .merge_eval(|value| {
            json!({
                "display_name": format_value(value, "{name} ({/metadata/department})"),
                "slug": format_value(value, "{name}").to_lowercase(),
                "adult": value.get_i64("age", 0) >= 18
            })
        })
        .copy("name", "display_name")
        .rename("name", "/person/name")
        .delete("date")
        .delete("raw")
        .add("site", "corporate")?;

    assert_eq!(
        value,
        json!({
            "adult": true,
            "age": 27,
            "display_name": "Ada Lovelace",
            "metadata": {
                "department": "research"
            },
            "person": {
                "name": "Ada Lovelace"
            },
            "site": "corporate",
            "slug": "ada lovelace"
        })
    );

    Ok(())
}
```

## JsonStream API

`JsonStream` is implemented for `serde_json::Value`, `jstream::Result<Value>`, and
`serde_json::Result<Value>`, so parse errors and transform errors can flow through a chain and be
handled with one final `?`.

- `select(["a", "b"])`: keep requested top-level fields, inserting `null` for missing fields.
- `add("field", value)`: add or replace a top-level field with any serializable value.
- `merge(json!({...}))`: shallow merge object fields without replacing existing fields.
- `merge_eval(|value| json!({...}))`: calculate fields from the current value, then merge them.
- `replace_eval(|value| json!({...}))`: calculate fields from the current value, then merge them with replacement.
- `parse_text("field", "{name} <{email}>")`: parse a string field and merge extracted fields.
- `delete("field")` / `delete("/path/to/field")`: remove a top-level field or JSON Pointer path.
- `rename(from, to)` and `copy(from, to)`: move or duplicate top-level fields or JSON Pointer paths.
- `require_shape(json!({...}))`: require fields and JSON types described by an example shape.
- `select_shape(json!({...}))`: validate a shape, then keep only fields described by that shape.
- `validate_as::<T>()`: validate by attempting to deserialize into a Rust type; the value is passed through.
- `try_map(function)`: apply a fallible operation to the data.

Note that `merge`, `merge_eval`, and `replace_eval` silently skip calculated values that are not
objects.

This gives us a way to conditionally add fields:

```rust
use jstream::{JsonGet, JsonStream, Value, json};

let value = json!({"num": 1})
    .merge_eval(|value| {
        if value.get_i64("num", 0) > 0 {
            json!({"is_non_zero": true})
        } else {
            Value::Null
        }
    })?;

assert_eq!(value, json!({"num": 1, "is_non_zero": true}));
# Ok::<(), jstream::Error>(())
```

Use `replace_eval` when calculated fields should overwrite existing values. This is useful after
parsing text into string fields and then converting those strings:

```rust
use jstream::{JsonGet, JsonStream, json};

let value = json!({ "date": "2025-05-20" })
    .parse_text("date", "{year}-{month}-{day}")
    .replace_eval(|value| {
        let part = |field| value.get_str(field, "0").parse::<i64>().unwrap_or_default();

        json!({
            "year": part("year"),
            "month": part("month"),
            "day": part("day")
        })
    })?;

assert_eq!(
    value,
    json!({
        "date": "2025-05-20",
        "year": 2025,
        "month": 5,
        "day": 20
    })
);
# Ok::<(), jstream::Error>(())
```

## Text Helpers

`format_value(value, template)` expands `{field}` placeholders from a JSON value. Placeholders that
start with `/`, such as `{/metadata/department}`, are resolved as JSON Pointers.

`parse_string(text, template)` uses the same placeholder notation to extract string fields from text
into a JSON object. Note that the last placeholder picks up the _rest of the text_, for instance
`parse_string("one,2 two three","{name},{value} {rest}") => {"name":"one","rest":"two three","value":"2"}`
(This was inspired by the `parse` command in `Nushell`)

## Typed Access

`JsonGet` provides typed accessors with explicit defaults:

```rust
use jstream::JsonGet;

let name = value.get_str("name", "unknown");
let age = value.get_i64("age", 0);
let active = value.get_bool("/metadata/active", false);
let metadata = value.get_object_opt("metadata");
```

The same top-level-or-JSON-Pointer lookup rules apply to `get_f64`,
`get_array`, `get_object`, `get_array_opt`, and `get_object_opt`.

## Sub-Pipelines

Use `try_map` to create a processing pipeline with composition:

```rust
use jstream::{JsonGet, JsonStream, Result};
use serde_json::{Value, json};

fn normalize_person(value: Value) -> Result<Value> {
    value
        .require_shape(json!({ "name": "", "age": 0 }))
        .select(["name", "age"])
        .merge_eval(|value| {
            json!({
                "adult": value.get_i64("age", 0) >= 18
            })
        })
}

fn main() -> Result<()> {
    let value = serde_json::from_str::<Value>(r#"{"name":"Ada","age":27}"#)
        .try_map(normalize_person)?;

    assert_eq!(
        value,
        json!({
            "adult": true,
            "age": 27,
            "name": "Ada"
        })
    );

    Ok(())
}
```

## Authorship

This was mostly generated by Codex running GPT 5.5; I specified the vision, thought about corner cases,
and worried about ergonmics; the agent did most of the actual coding, and came up with some useful ideas, 
like implementing the `JsonStream` trait both on `Value` and `Result<Value>`. (This is an example of where
the extra boilerplate involved might have discouraged me from following this ideal manually).

Full transcript is here: codex-transcript.txt
