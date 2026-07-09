use jstream::{JsonStream, parse_string};

fn main() -> jstream::Result<()> {
    let value = parse_string(
        "Ada Lovelace <ada@example.test> (research)",
        "{name} <{email}> ({/metadata/department})",
    )?
    .add("source", "directory")?;

    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}
