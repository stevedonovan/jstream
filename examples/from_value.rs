use jstream::JsonStream;
use serde_json::json;

fn main() -> jstream::Result<()> {
    let value = json!({
        "name": "Grace Hopper",
        "role": "computer scientist",
        "private_notes": "not included"
    })
    .select(["name", "role", "site"])
    .add("active", true)?;

    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}
