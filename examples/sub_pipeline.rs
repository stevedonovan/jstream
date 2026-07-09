use jstream::{JsonGet, JsonStream, format_value};
use serde_json::{Value, json};

fn normalize_person(value: Value) -> jstream::Result<Value> {
    value
        .require_shape(json!({
            "first_name": "",
            "last_name": "",
            "age": 0
        }))
        .select(["first_name", "last_name", "age"])
        .merge_eval(|value| {
            json!({
                "display_name": format_value(value, "{first_name} {last_name}"),
                "adult": value.get_i64("age", 0) >= 18
            })
        })
}

fn main() -> jstream::Result<()> {
    let text = r#"{
        "first_name": "Ada",
        "last_name": "Lovelace",
        "age": 27,
        "internal_id": "hidden"
    }"#;

    let value = serde_json::from_str::<Value>(text)
        .try_map(normalize_person)?
        .add("site", "corporate")?;

    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}
