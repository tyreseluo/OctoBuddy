//! Data for an OctoSense app: a CSV file or a JSON API the person gives,
//! from which the outer loop builds the app.
//!
//! A CSV becomes the app's own data: OctoBuddy converts it to JSON and keeps
//! it in `main.splash` as one `let NAME = "…".parse_json()` line between
//! markers it owns (a script app reads no file of its bundle, and this runs
//! in every host with no capability at all), and the full JSON in
//! `.octobuddy/data/` for the loops to read. An API stays live: OctoBuddy
//! fetches one answer to learn its shape, and the app fetches it itself
//! (`net`, its host declared). Either way the outer loop is told what the
//! data holds: its columns or fields, their kinds, examples.
use serde_json::{Map, Value};
use std::path::Path;
use std::process::Command;

/// The largest JSON kept in `main.splash`; more is cut to fewer rows.
const MAX_EMBED: usize = 1_500_000;

/// What the loops are told about a data source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    /// The name the app knows it by (`SALES`), or the API's host.
    pub name: String,
    pub summary: String,
}

/// `sales-2024.csv` → `SALES_2024`; a name with no ASCII letter → `DATA`.
pub fn ident(stem: &str) -> String {
    let s: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    let s = s.trim_matches('_').to_string();
    match s.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => s,
        Some(_) => format!("DATA_{s}"),
        None => "DATA".into(),
    }
}

/// Rows of a CSV (quotes, doubled quotes, commas and newlines inside quotes).
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.trim_start_matches('\u{feff}');
    let (mut rows, mut row, mut field) = (Vec::new(), Vec::new(), String::new());
    let (mut quoted, mut chars) = (false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.trim().is_empty()));
    rows
}

fn cell(text: &str) -> Value {
    let t = text.trim();
    if t.is_empty() {
        return Value::Null;
    }
    if let Ok(n) = t.parse::<i64>() {
        return Value::from(n);
    }
    match t.parse::<f64>() {
        Ok(f) if f.is_finite() => Value::from(f),
        _ => Value::String(t.to_string()),
    }
}

/// A CSV's rows as JSON objects keyed by its header.
pub fn csv_to_json(text: &str) -> Result<Value, String> {
    let rows = parse_csv(text);
    let (header, body) = rows.split_first().ok_or("the file is empty")?;
    let keys: Vec<String> = header.iter().enumerate().map(|(i, h)| {
        let h = h.trim();
        if h.is_empty() { format!("column{}", i + 1) } else { h.to_string() }
    }).collect();
    if body.is_empty() {
        return Err("the file has a header but no rows".into());
    }
    Ok(Value::Array(body.iter().map(|r| {
        let mut o = Map::new();
        for (i, k) in keys.iter().enumerate() {
            o.insert(k.clone(), cell(r.get(i).map(String::as_str).unwrap_or("")));
        }
        Value::Object(o)
    }).collect()))
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "empty",
        Value::Bool(_) => "true/false",
        Value::Number(_) => "number",
        Value::String(_) => "text",
        Value::Array(_) => "list",
        Value::Object(_) => "object",
    }
}

fn example(v: &Value) -> String {
    let s = match v {
        Value::String(s) => format!("{s:?}"),
        other => other.to_string(),
    };
    s.chars().take(40).collect()
}

/// The fields of a list of objects: each one's kind and a couple of examples.
fn fields(rows: &[Value]) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    for r in rows.iter().take(200) {
        if let Value::Object(o) = r {
            for k in o.keys() {
                if !order.contains(k) {
                    order.push(k.clone());
                }
            }
        }
    }
    order.iter().map(|k| {
        let values: Vec<&Value> = rows.iter().filter_map(|r| r.get(k)).filter(|v| !v.is_null()).collect();
        let kinds: Vec<&str> = {
            let mut ks: Vec<&str> = values.iter().take(200).map(|v| kind(v)).collect();
            ks.sort();
            ks.dedup();
            ks
        };
        let examples: Vec<String> = values.iter().take(2).map(|v| example(v)).collect();
        let empty = rows.len() - values.len();
        let empty = if empty > 0 { format!(", {empty} empty") } else { String::new() };
        format!("{k} ({}{empty}; e.g. {})", if kinds.is_empty() { "empty".into() } else { kinds.join("/") }, examples.join(", "))
    }).collect()
}

/// What a JSON value holds, in a few lines.
pub fn describe(v: &Value) -> String {
    match v {
        Value::Array(rows) if rows.iter().all(Value::is_object) => {
            format!("a list of {} records with the fields: {}", rows.len(), fields(rows).join("; "))
        }
        Value::Array(rows) => format!("a list of {} {} values", rows.len(), rows.first().map(kind).unwrap_or("empty")),
        Value::Object(o) => {
            let parts: Vec<String> = o.iter().take(30).map(|(k, v)| match v {
                Value::Array(rows) if rows.iter().all(Value::is_object) && !rows.is_empty() => format!("{k}: {}", describe(v)),
                other => format!("{k} ({}; e.g. {})", kind(other), example(other)),
            }).collect();
            format!("an object with: {}", parts.join("; "))
        }
        other => format!("a single {} ({})", kind(other), example(other)),
    }
}

/// `main.splash` with `value` as `NAME`'s line between OctoBuddy's markers
/// (replacing an earlier one); the rows kept, when it had to be cut.
pub fn embed(main: &str, name: &str, value: &Value) -> (String, Option<usize>) {
    let mut value = value.clone();
    let mut kept = None;
    if let Value::Array(rows) = &mut value {
        while serde_json::to_string(rows).map(|s| s.len()).unwrap_or(0) > MAX_EMBED && rows.len() > 1 {
            let keep = rows.len() * 3 / 4;
            rows.truncate(keep.max(1));
            kept = Some(rows.len());
        }
    }
    let json = serde_json::to_string(&value).unwrap_or_else(|_| "null".into());
    let escaped = json.replace('\\', "\\\\").replace('"', "\\\"");
    let (begin, end) = (format!("// octobuddy:data {name} begin"), format!("// octobuddy:data {name} end"));
    let block = format!("{begin} (OctoBuddy keeps this block: do not edit it)\nlet {name} = \"{escaped}\".parse_json()\n{end}\n");
    let out = match (main.find(&begin), main.find(&end)) {
        (Some(a), Some(b)) if b > a => {
            let after = main[b..].find('\n').map(|i| b + i + 1).unwrap_or(main.len());
            format!("{}{block}{}", &main[..a], &main[after..])
        }
        _ => format!("{block}{main}"),
    };
    (out, kept)
}

/// Adds a CSV to an app project: its data in `main.splash` and
/// `.octobuddy/data/`, and what it holds.
pub fn add_csv(project: &str, file: &Path) -> Result<Found, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let value = csv_to_json(&text)?;
    let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = ident(&stem);
    let main_path = Path::new(project).join("bundle/main.splash");
    let main = std::fs::read_to_string(&main_path).map_err(|e| format!("main.splash: {e}"))?;
    let (next, kept) = embed(&main, &name, &value);
    std::fs::write(&main_path, next).map_err(|e| format!("main.splash: {e}"))?;
    let data = Path::new(project).join(".octobuddy/data");
    let _ = std::fs::create_dir_all(&data);
    let full = data.join(format!("{name}.json"));
    let _ = std::fs::write(&full, serde_json::to_string_pretty(&value).unwrap_or_default());
    let cut = kept.map(|n| format!(" (too big to embed whole: the app has its first {n} rows)")).unwrap_or_default();
    Ok(Found {
        name: name.clone(),
        summary: format!("CSV {} → `{name}` in bundle/main.splash (a line between OctoBuddy's markers: read it, never edit it){cut}; \
the full data is .octobuddy/data/{name}.json. It is {}.", file.display(), describe(&value)),
    })
}

/// An API's host, from its URL.
pub fn host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?.split(':').next()?.to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Learns an API's shape from one answer (kept in `.octobuddy/data/`).
pub fn add_api(project: &str, given: &str) -> Result<Found, String> {
    let url = given;
    let first_host = host(url).ok_or("give an https:// address (the app may only reach https hosts)")?;
    // Where it really answers from: an app follows no redirect to a host it
    // did not declare, so the app must ask there itself.
    let out = Command::new("curl").args(["-sSL", "-m", "20", "--max-filesize", "5000000", "-H", "Accept: application/json", "-w", "\n%{url_effective}", url])
        .output().map_err(|e| format!("could not run curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("could not fetch it: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let body = String::from_utf8_lossy(&out.stdout).into_owned();
    let (json, effective) = body.rsplit_once('\n').unwrap_or((body.as_str(), url));
    let (url, host) = match host(effective.trim()) {
        Some(h) if effective.trim() != url => (effective.trim().to_string(), h),
        _ => (url.to_string(), first_host),
    };
    let moved = if url != given { format!(" It redirects from {given}: the app must fetch {url} itself (redirects to another host are blocked).") } else { String::new() };
    let value: Value = serde_json::from_str(json).map_err(|e| format!("it did not answer JSON ({e})"))?;
    let data = Path::new(project).join(".octobuddy/data");
    let _ = std::fs::create_dir_all(&data);
    let sample = format!("{}.json", host.replace('.', "_"));
    let _ = std::fs::write(data.join(&sample), serde_json::to_string_pretty(&value).unwrap_or_default());
    Ok(Found {
        name: host.clone(),
        summary: format!("API {url}: the app fetches it live with net.http_request (manifest: capability \"net\", \
network.hosts [\"{host}\"]); one answer is in .octobuddy/data/{sample}.{moved} It answers {}.", describe(&value)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_csv_becomes_records() {
        let v = csv_to_json("\u{feff}city,amount,note\n北京,12.5,\n\"上海, \"\"HQ\"\"\",7,\"two\nlines\"\n").unwrap();
        assert_eq!(v[0]["city"], "北京");
        assert_eq!(v[0]["amount"], 12.5);
        assert!(v[0]["note"].is_null());
        assert_eq!(v[1]["city"], "上海, \"HQ\"");
        assert_eq!(v[1]["amount"], 7);
        assert_eq!(v[1]["note"], "two\nlines");
        let d = describe(&v);
        assert!(d.contains("2 records") && d.contains("amount (number") && d.contains("note (text, 1 empty"), "{d}");
    }

    #[test]
    fn a_csv_goes_into_the_app() {
        let dir = std::env::temp_dir().join(format!("octobuddy-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bundle")).unwrap();
        std::fs::write(dir.join("bundle/main.splash"), "View{}\n").unwrap();
        let csv = dir.join("sales.csv");
        std::fs::write(&csv, "month,amount\n2026-01,10\n2026-02,12.5\n").unwrap();
        let project = dir.to_string_lossy().into_owned();
        let found = add_csv(&project, &csv).unwrap();
        assert_eq!(found.name, "SALES");
        assert!(found.summary.contains("2 records") && found.summary.contains("amount (number"), "{}", found.summary);
        let main = std::fs::read_to_string(dir.join("bundle/main.splash")).unwrap();
        assert!(main.contains("let SALES = \"[{") && main.ends_with("View{}\n"));
        assert!(dir.join(".octobuddy/data/SALES.json").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_data_line_is_replaced_not_repeated() {
        let v: Value = serde_json::json!([{"a": "x \"y\""}]);
        let (once, _) = embed("View{}\n", "SALES", &v);
        assert!(once.starts_with("// octobuddy:data SALES begin"));
        assert!(once.contains(r#"let SALES = "[{\"a\":\"x \\\"y\\\"\"}]".parse_json()"#), "{once}");
        let (twice, _) = embed(&once, "SALES", &serde_json::json!([1]));
        assert_eq!(twice.matches("octobuddy:data SALES begin").count(), 1);
        assert!(twice.contains(r#"let SALES = "[1]".parse_json()"#) && twice.ends_with("View{}\n"));
        assert_eq!(ident("sales-2024"), "SALES_2024");
        assert_eq!(ident("销售"), "DATA");
        assert_eq!(host("https://api.example.com/v1?q=1").as_deref(), Some("api.example.com"));
        assert_eq!(host("http://x.com"), None);
    }
}
