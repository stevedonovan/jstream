use jstream::{JsonGet, JsonStream, format_value};
use serde_json::{Value, json};

fn main() -> serde_json::Result<()> {
    let text = r#"{
        "first_name": "Ada",
        "last_name": "Lovelace",
        "age": 27,
        "source": "original"
    }"#;

    let value = serde_json::from_str::<Value>(text)
        .require_shape(json!({
            "first_name": "",
            "last_name": "",
            "age": 0
        }))
        .select(["first_name", "last_name", "age", "source"])
        .merge(json!({
            "source": "imported",
            "site": "corporate"
        }))
        .eval(|value| {
            json!({
                "display_name": format_value(value, "{first_name} {last_name}"),
                "slug": format_value(value, "{first_name}-{last_name}").to_lowercase(),
                "adult": value.get_i64("age", 0) >= 18
            })
        })?;

    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}
