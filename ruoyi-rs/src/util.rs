use rusqlite::types::ValueRef;
use rusqlite::Row;
use serde_json::{json, Map, Value};

/// snake_case column name -> camelCase JSON key.
/// Names that already contain an uppercase letter (SQL aliases like `deptName`) pass through.
pub fn col_to_key(name: &str) -> String {
    if name.chars().any(|c| c.is_ascii_uppercase()) {
        return name.to_string();
    }
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Convert one rusqlite row to a camelCased JSON object.
/// INTEGERs obey the RuoYi BigNumberSerializer rule: number when |v| <= 2^53-1,
/// quoted string otherwise (plus-ui parses ids with json-bigint).
pub fn row_to_json(row: &Row) -> Value {
    let mut map = Map::new();
    for (i, col) in row.as_ref().column_names().iter().enumerate() {
        let key = col_to_key(col);
        let v = row.get_ref(i as usize).unwrap_or(ValueRef::Null);
        let val = match v {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(n) => {
                if n.abs() <= 9007199254740991 {
                    json!(n)
                } else {
                    json!(n.to_string())
                }
            }
            ValueRef::Real(f) => json!(f),
            ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
            ValueRef::Blob(b) => Value::String(String::from_utf8_lossy(b).into_owned()),
        };
        map.insert(key, val);
    }
    Value::Object(map)
}

/// Run a query and collect all rows as camelCased JSON objects.
pub fn query_json<P: rusqlite::Params>(
    db: &rusqlite::Connection,
    sql: &str,
    params: P,
) -> Vec<Value> {
    let Ok(mut stmt) = db.prepare(sql) else { return Vec::new() };
    let rows = stmt.query_map(params, |r| Ok(row_to_json(r)));
    match rows {
        Ok(rs) => rs.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

/// Run a query returning a single JSON object or Null.
pub fn query_one<P: rusqlite::Params>(db: &rusqlite::Connection, sql: &str, params: P) -> Value {
    query_json(db, sql, params).into_iter().next().unwrap_or(Value::Null)
}

/// Current local time formatted like MySQL datetime: YYYY-MM-DD HH:MM:SS
pub fn now_str() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    fmt_ms(ms)
}

fn fmt_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let days = secs / 86400;
    let mut rem = secs % 86400;
    let (y, m, d) = civil_from_days(days);
    let hh = rem / 3600;
    rem %= 3600;
    let mm = rem / 60;
    let ss = rem % 60;
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, hh, mm, ss)
}

/// Howard Hinnant's civil_from_days (proleptic Gregorian calendar).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

/// LIKE escape helper: wrap user input for a LIKE '%..%' match.
pub fn like_wrap(s: &str) -> String {
    format!("%{}%", s.replace('%', "\\%").replace('_', "\\_"))
}
