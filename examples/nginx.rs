use chrono::{DateTime, SecondsFormat};
use jstream::{JsonGet, JsonStream, Value, json};
use url::Url;

fn main() -> jstream::Result<()> {
    // This is Nginx's combined format with `$request_time` appended.
    let lines = [
        r#"192.0.2.10 - alice [03/Sep/2026:12:00:00 +0000] "GET /health HTTP/1.1" 200 18 "-" "monitor/1.0" 0.004"#,
        r#"192.0.2.11 - - [03/Sep/2026:12:00:01 +0000] "POST /api/orders?token=secret&limit=10 HTTP/1.1" 503 321 "https://shop.example/" "client/2.0" 1.245"#,
        r#"192.0.2.12 - bob [03/Sep/2026:12:00:02 +0000] "GET /reports?password=hunter2 HTTP/2.0" 200 4096 "-" "browser/3.0" 2.500"#,
    ];

    let events = lines
        .into_iter()
        .map(|line| {
            json!({ "raw": line })
                .parse_text(
                    "raw",
                    concat!(
                        "{remote_addr} - {remote_user} [{timestamp}] ",
                        "\"{request}\" {status} {body_bytes} ",
                        "\"{referer}\" \"{user_agent}\" {request_time}"
                    ),
                )
                .parse_text("request", "{method} {target} {protocol}")
                .replace_eval(|value| {
                    json!({
                        "timestamp": iso_timestamp(value),
                        "target": sanitized_target(value.get_str("target", "/")),
                        "status": value.get_str("status", "0").parse::<i64>().unwrap_or_default(),
                        "body_bytes": value.get_str("body_bytes", "0").parse::<u64>().unwrap_or_default(),
                        "request_time": value.get_str("request_time", "0").parse::<f64>().unwrap_or_default()
                    })
                })?
                .merge_eval(|value| {
                    let status = value.get_i64("status", 0);
                    let request_time = value.get_f64("request_time", 0.0);
                    json!({
                        "status_class": format!("{}xx", status / 100),
                        "server_error": status >= 500,
                        "slow": request_time >= 1.0
                    })
                })
                .filter(|value| {
                    value.get_bool("server_error", false) || value.get_bool("slow", false)
                })
                .select([
                    "timestamp",
                    "remote_addr as client_ip",
                    "remote_user as user",
                    "method",
                    "target",
                    "status",
                    "status_class",
                    "body_bytes",
                    "request_time",
                    "server_error",
                    "slow",
                ])
        })
        .collect::<jstream::Result<Vec<_>>>()?
        .into_iter()
        .filter(|event| !event.is_null())
        .collect::<Vec<_>>();

    assert_eq!(
        events,
        vec![
            json!({
                "body_bytes": 321,
                "client_ip": "192.0.2.11",
                "method": "POST",
                "request_time": 1.245,
                "server_error": true,
                "slow": true,
                "status": 503,
                "status_class": "5xx",
                "target": "/api/orders?token=REDACTED&limit=10",
                "timestamp": "2026-09-03T12:00:01Z",
                "user": "-"
            }),
            json!({
                "body_bytes": 4096,
                "client_ip": "192.0.2.12",
                "method": "GET",
                "request_time": 2.5,
                "server_error": false,
                "slow": true,
                "status": 200,
                "status_class": "2xx",
                "target": "/reports?password=REDACTED",
                "timestamp": "2026-09-03T12:00:02Z",
                "user": "bob"
            })
        ]
    );

    println!("{}", serde_json::to_string_pretty(&events)?);
    Ok(())
}

fn iso_timestamp(value: &Value) -> String {
    DateTime::parse_from_str(value.get_str("timestamp", ""), "%d/%b/%Y:%H:%M:%S %z")
        .expect("the example contains valid Nginx timestamps")
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn sanitized_target(target: &str) -> String {
    let mut url = Url::parse(&format!("https://log.invalid{target}"))
        .expect("the example contains valid request targets");
    let pairs = url
        .query_pairs()
        .map(|(key, value)| {
            let value = if matches!(key.as_ref(), "token" | "password" | "api_key") {
                "REDACTED".to_owned()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect::<Vec<_>>();

    if pairs.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }

    match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    }
}
