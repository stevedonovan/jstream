use jstream::JsonStream;
use serde_json::Value;

fn main() -> jstream::Result<()> {
    let text = r#"{
        "name": "Ada Lovelace",
        "date": "1843-09-01",
        "age": 27,
        "internal_id": "hidden"
    }"#;

    let value = serde_json::from_str::<Value>(text)
        .select(["name", "date", "age"])
        .merge(serde_json::json!({
            "site": "people",
            "source": "archive"
        }))
        .add("site", "corporate")?;

    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}
