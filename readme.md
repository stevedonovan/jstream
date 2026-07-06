# jstream

`jstream` is a fluent API for chaining operations on JSON data.

```Rust
use jstream::{JsonGet, JsonStream, format_value};
use serde_json::{Value, json};

let value = serde_json::from_str::<Value>(text)
    .require_shape(json!({
        "name": "",
        "age": 0,
        "metadata": {
            "department": ""
        }
    }))
    .select(["name", "date", "age"])  // null if a requested field is not present
    .merge(json!({ "site": "corporate", "age": 0 }))
    .eval(|value| {
        json!({
            "display_name": format_value(value, "{name} ({/metadata/department})"),
            "slug": format_value(value, "{name}").to_lowercase(),
            "adult": value.get_i64("age", 0) >= 18
        })
    })
    .add("site", "corporate")?;
```

This crate defines a trait over `serde_json::Value` which implements common operations like `select`
and `add`. The same trait is also implemented for `serde_json::Result<Value>`, so parse errors and
serialization errors can flow through the chain and be handled with one final `?`.

`merge` performs a shallow object merge without overriding existing fields. `eval` receives the
current pipeline value, returns a JSON object, and merges it with the same non-overriding behavior.
`format_value` expands top-level `{field}` placeholders from the current value; unknown fields expand
to empty strings. Placeholders that start with `/`, such as `{/metadata/department}`, are resolved as
JSON Pointers.
`JsonGet` provides typed accessors such as `get_str`, `get_i64`, `get_f64`, `get_bool`,
`get_array`, and `get_object`, using the same top-level-or-JSON-Pointer lookup rules.
`require_shape` checks that required fields and JSON types are present before transformation; extra
input fields are allowed.

	   
