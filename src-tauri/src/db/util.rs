//! 关系型数据库驱动之间可复用的纯逻辑：与具体驱动库无关。
//!
//! 只放 SQL 文本处理、CSV 渲染、时间格式化等纯函数，驱动实现按需调用；
//! 任何依赖具体协议/元数据表的能力（SHOW/USE/information_schema 等）都下沉到各驱动。
use std::collections::HashMap;
use std::io::Write;

use chrono::{Datelike, Local, TimeZone};

/// 剥离末尾分号（追加 LIMIT 时不会变成语法错误）。
pub(crate) fn strip_trailing_semicolons(sql: &str) -> String {
    sql.trim().trim_end_matches(';').trim_end().to_string()
}

/// 语句是否以 SELECT / WITH 开头（用于区分读/写，供各驱动复用）。
pub(crate) fn starts_with_select_or_with(sql: &str) -> bool {
    let t = sql.trim_start();
    for kw in ["select", "with"] {
        if t.len() >= kw.len() && t[..kw.len()].eq_ignore_ascii_case(kw) {
            match t[kw.len()..].chars().next() {
                None => return true,
                Some(c) => {
                    if !(c.is_ascii_alphanumeric() || c == '_') {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// 是否包含独立单词 limit（对齐 /\blimit\b/i）。
fn has_word_limit(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let needle = b"limit";
    let mut i = 0usize;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let before_ok = i == 0 || !is_word_byte(bytes[i - 1]);
            let after = i + needle.len();
            let after_ok = after >= bytes.len() || !is_word_byte(bytes[after]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// 给 SELECT/WITH 语句追加标准 LIMIT/OFFSET 实现分页，返回 (最终 SQL, 是否分页)。
/// 仅适用于使用 LIMIT/OFFSET 语法的数据库（MySQL/PostgreSQL/SQLite）；
/// SQL Server / Oracle 等方言由各自驱动覆盖实现。
pub(crate) fn build_paged_sql(sql: &str, limit: i64, offset: i64) -> (String, bool) {
    let base = strip_trailing_semicolons(sql);
    let lim = if limit > 0 { limit } else { 0 };
    let off = if offset > 0 { offset } else { 0 };
    let paged = lim > 0 && starts_with_select_or_with(&base) && !has_word_limit(&base);
    let final_sql = if paged {
        if off > 0 {
            format!("{} LIMIT {} OFFSET {}", base, lim, off)
        } else {
            format!("{} LIMIT {}", base, lim)
        }
    } else {
        base
    };
    (final_sql, paged)
}

pub(crate) fn format_date(y: u16, mo: u8, d: u8, h: u8, mi: u8, s: u8, us: u32) -> String {
    let base = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, mi, s);
    if us != 0 {
        format!("{}.{:06}", base, us)
    } else {
        base
    }
}

pub(crate) fn format_time(neg: bool, days: u32, h: u8, m: u8, s: u8, us: u32) -> String {
    let total_hours = days * 24 + h as u32;
    let base = format!(
        "{}{:02}:{:02}:{:02}",
        if neg { "-" } else { "" },
        total_hours,
        m,
        s
    );
    if us != 0 {
        format!("{}.{:06}", base, us)
    } else {
        base
    }
}

/// 与 server.js csvCell 对齐：null → 空；对象/数组 → JSON 字符串；否则原样。
pub(crate) fn csv_cell(v: &serde_json::Value) -> String {
    let s = match v {
        serde_json::Value::Null => return String::new(),
        serde_json::Value::String(s) => s.clone(),
        other @ (serde_json::Value::Object(_) | serde_json::Value::Array(_)) => {
            serde_json::to_string(other).unwrap_or_default()
        }
        other => other.to_string(),
    };
    csv_escape(&s)
}

pub(crate) fn csv_escape(s: &str) -> String {
    if s.contains('"') || s.contains(',') || s.contains('\r') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 对齐 JS `String(value)`（用于 exportValue 的取键/正则判断）。
pub(crate) fn json_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn is_unix_ts(s: &str) -> bool {
    (s.len() == 10 || s.len() == 13) && s.bytes().all(|b| b.is_ascii_digit())
}

/// 10/13 位秒/毫秒时间戳 → 本地时间 "YYYY-MM-DD HH:MM:SS"；年份不在 2000..2100 则返回 None。
fn format_ts_local(s: &str) -> Option<String> {
    let raw: i64 = s.parse().ok()?;
    let (secs, nanos) = if s.len() == 13 {
        (raw / 1000, ((raw % 1000) * 1_000_000) as u32)
    } else {
        (raw, 0)
    };
    let dt = Local.timestamp_opt(secs, nanos).single()?;
    let year = dt.year();
    if !(2000..=2100).contains(&year) {
        return None;
    }
    Some(dt.format("%Y-%m-%d %H:%M:%S").to_string())
}

pub(crate) fn export_cell(
    v: &serde_json::Value,
    mode: &str,
    map: Option<&HashMap<String, String>>,
) -> String {
    if v.is_null() {
        return String::new();
    }
    if mode == "timestamp" {
        let s = json_to_string(v);
        if is_unix_ts(&s) {
            if let Some(fmt) = format_ts_local(&s) {
                return csv_escape(&fmt);
            }
        }
    }
    if mode == "enum" {
        if let Some(map) = map {
            let key = json_to_string(v);
            if let Some(label) = map.get(&key) {
                return csv_escape(label);
            }
        }
    }
    csv_cell(v)
}

pub(crate) fn flush_insert<W: Write>(
    writer: &mut W,
    header: &str,
    rows: &[String],
) -> Result<(), String> {
    writer
        .write_all(header.as_bytes())
        .map_err(|e| e.to_string())?;
    writer
        .write_all(rows.join(",\n").as_bytes())
        .map_err(|e| e.to_string())?;
    writer.write_all(b";\n").map_err(|e| e.to_string())?;
    Ok(())
}

/// 按顶层分号切分 SQL 脚本：跳过字符串 / 反引号标识符 / 注释里的分号。
pub(crate) fn split_sql_statements(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut i = 0usize;

    while i < n {
        let c = chars[i];
        match c {
            '\'' | '"' | '`' => {
                let quote = c;
                cur.push(c);
                i += 1;
                while i < n {
                    let ch = chars[i];
                    cur.push(ch);
                    if ch == '\\' && quote != '`' {
                        // 引号内反斜杠转义：吞掉下一个字符
                        i += 1;
                        if i < n {
                            cur.push(chars[i]);
                        }
                        i += 1;
                        continue;
                    }
                    if ch == quote {
                        // 连续两个引号是转义写法，不算结束
                        if i + 1 < n && chars[i + 1] == quote {
                            cur.push(chars[i + 1]);
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '#' => {
                while i < n && chars[i] != '\n' {
                    cur.push(chars[i]);
                    i += 1;
                }
            }
            '-' => {
                let is_comment = i + 1 < n
                    && chars[i + 1] == '-'
                    && (i + 2 >= n || chars[i + 2].is_whitespace());
                if is_comment {
                    while i < n && chars[i] != '\n' {
                        cur.push(chars[i]);
                        i += 1;
                    }
                } else {
                    cur.push(c);
                    i += 1;
                }
            }
            '/' => {
                if i + 1 < n && chars[i + 1] == '*' {
                    cur.push('/');
                    cur.push('*');
                    i += 2;
                    while i < n && !(chars[i] == '*' && i + 1 < n && chars[i + 1] == '/') {
                        cur.push(chars[i]);
                        i += 1;
                    }
                    if i < n {
                        cur.push('*');
                        i += 1;
                    }
                    if i < n {
                        cur.push('/');
                        i += 1;
                    }
                } else {
                    cur.push(c);
                    i += 1;
                }
            }
            ';' => {
                let stmt = cur.trim().to_string();
                if !stmt.is_empty() {
                    out.push(stmt);
                }
                cur.clear();
                i += 1;
            }
            _ => {
                cur.push(c);
                i += 1;
            }
        }
    }
    let stmt = cur.trim().to_string();
    if !stmt.is_empty() {
        out.push(stmt);
    }
    out
}
