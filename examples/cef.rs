#[path = "cef/extension.rs"]
mod extension;

use chrono::{NaiveDateTime, SecondsFormat};
use jstream::{JsonGet, JsonStream, Value, format_value, json};

use extension::parse_extension;

fn iso_timestamp(value: &Value, year: i32) -> String {
    // Traditional syslog timestamps omit the year, so an ingestion service
    // must supply it from its own context.
    let timestamp = format!(
        "{year} {} {} {}",
        value.get_str("/syslog/month", ""),
        value.get_str("/syslog/day", ""),
        value.get_str("/syslog/time", "")
    );

    NaiveDateTime::parse_from_str(&timestamp, "%Y %b %d %H:%M:%S")
        .expect("the example contains valid syslog timestamps")
        .and_utc()
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn main() -> jstream::Result<()> {
    let lines = [
        r#"Sep 03 12:00:00 gateway CEF:0|Acme|Firewall|2.1|100|Blocked login|8|src=10.0.0.4 dst=10.0.0.9 act=blocked suser=alice msg=Login failed reason\=bad password"#,
        r#"Sep 03 12:00:01 gateway CEF:0|Acme|Firewall|2.1|101|Health check|3|src=10.0.0.5 dst=10.0.0.9 act=blocked msg=Routine probe"#,
        r#"Sep 03 12:00:02 gateway CEF:0|Acme|Firewall|2.1|102|Admin login|9|src=10.0.0.6 dst=10.0.0.9 act=allowed suser=bob"#,
    ];

    let events = lines
        .into_iter()
        .map(|line| {
            json!({ "raw": line })
                .parse_text(
                    "raw",
                    concat!(
                        "{/syslog/month} {/syslog/day} {/syslog/time} {/syslog/host} ",
                        "CEF:{/cef/version}|{/cef/device_vendor}|",
                        "{/cef/device_product}|{/cef/device_version}|",
                        "{/cef/event_class_id}|{/cef/name}|{/cef/severity}|{extension}"
                    ),
                )
                .replace_eval(|value| {
                    json!({
                        "extension": parse_extension(value.get_str("extension", ""))
                    })
                })?
                .merge_eval(|value| {
                    json!({
                        "timestamp": iso_timestamp(value, 2026)
                    })
                })
                .filter(|value| {
                    value
                        .get_str("/cef/severity", "0")
                        .parse::<i64>()
                        .unwrap_or_default()
                        >= 7
                })
                .filter(|value| value.get_str("/extension/act", "") == "blocked")
                .select([
                    "timestamp",
                    "/cef/name as event",
                    "/cef/severity as severity",
                    "/extension/src as source_ip",
                    "/extension/dst as destination_ip",
                    "/extension/suser as user",
                    "/extension/msg as message",
                ])
                .merge_eval(|value| {
                    json!({
                        "summary": format_value(value, "{event}: {source_ip} -> {destination_ip}")
                    })
                })
        })
        .collect::<jstream::Result<Vec<_>>>()?
        .into_iter()
        .filter(|event| !event.is_null())
        .collect::<Vec<_>>();

    assert_eq!(
        events,
        vec![json!({
            "destination_ip": "10.0.0.9",
            "event": "Blocked login",
            "message": "Login failed reason=bad password",
            "severity": "8",
            "source_ip": "10.0.0.4",
            "summary": "Blocked login: 10.0.0.4 -> 10.0.0.9",
            "timestamp": "2026-09-03T12:00:00Z",
            "user": "alice"
        })]
    );

    println!("{}", serde_json::to_string_pretty(&events)?);
    Ok(())
}
