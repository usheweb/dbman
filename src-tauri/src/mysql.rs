use std::collections::HashMap;
use std::io::Write;

use chrono::{Datelike, Local, TimeZone};
use mysql_async::consts::ColumnType;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Opts, OptsBuilder, Row, Value};
use tauri::State;

use crate::models::{
    ColumnDef, ColumnInfo, DbConfig, ExportColumn, FieldInfo, ForeignKeyDef, IndexDef,
    QueryResponse, TableInfo, TableOptionDef, TableStructure,
};
use crate::state::AppState;

/// 按 server.js 的默认端口规则补全端口。
pub fn default_port(db_type: &str) -> u16 {
    if db_type == "redis" {
        6379
    } else {
        3306
    }
}

pub async fn connect(
    cfg: &DbConfig,
    host: &str,
    port: u16,
) -> Result<Conn, String> {
    let mut builder = OptsBuilder::default()
        .ip_or_hostname(host)
        .tcp_port(port);
    if let Some(user) = cfg.user.as_ref().filter(|u| !u.is_empty()) {
        builder = builder.user(Some(user.clone()));
    }
    if let Some(pass) = cfg.password.as_ref() {
        builder = builder.pass(Some(pass.clone()));
    }
    if let Some(db) = cfg.database.as_ref().filter(|d| !d.is_empty()) {
        builder = builder.db_name(Some(db.clone()));
    }
    let opts: Opts = builder.into();
    Conn::new(opts).await.map_err(|e| e.to_string())
}

/// 剥离末尾分号（追加 LIMIT 时不会变成语法错误）。
fn strip_trailing_semicolons(sql: &str) -> String {
    sql.trim().trim_end_matches(';').trim_end().to_string()
}

fn starts_with_select_or_with(sql: &str) -> bool {
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

/// 返回 (最终 SQL, 是否分页)。
fn build_paged_sql(sql: &str, limit: i64, offset: i64) -> (String, bool) {
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

fn format_date(y: u16, mo: u8, d: u8, h: u8, mi: u8, s: u8, us: u32) -> String {
    let base = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, mi, s);
    if us != 0 {
        format!("{}.{:06}", base, us)
    } else {
        base
    }
}

fn format_time(neg: bool, days: u32, h: u8, m: u8, s: u8, us: u32) -> String {
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

fn value_to_json(v: &Value, is_json_column: bool) -> serde_json::Value {
    match v {
        Value::NULL => serde_json::Value::Null,
        Value::Bytes(bytes) => {
            let s = String::from_utf8_lossy(bytes).into_owned();
            if is_json_column {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&s) {
                    return parsed;
                }
            }
            serde_json::Value::String(s)
        }
        Value::Int(i) => serde_json::json!(i),
        Value::UInt(u) => serde_json::json!(u),
        Value::Float(f) => serde_json::json!(f),
        Value::Double(d) => serde_json::json!(d),
        Value::Date(y, mo, d, h, mi, s, us) => {
            serde_json::Value::String(format_date(*y, *mo, *d, *h, *mi, *s, *us))
        }
        Value::Time(neg, days, h, m, s, us) => {
            serde_json::Value::String(format_time(*neg, *days, *h, *m, *s, *us))
        }
    }
}

fn row_to_json(row: &Row) -> serde_json::Value {
    let columns = row.columns_ref();
    let mut map = serde_json::Map::new();
    for (i, col) in columns.iter().enumerate() {
        let name = col.name_str().into_owned();
        let value = row
            .as_ref(i)
            .map(|v| value_to_json(v, col.column_type() == ColumnType::MYSQL_TYPE_JSON))
            .unwrap_or(serde_json::Value::Null);
        map.insert(name, value);
    }
    serde_json::Value::Object(map)
}

/// 与 server.js csvCell 对齐：null → 空；对象/数组 → JSON 字符串；否则原样。
fn csv_cell(v: &serde_json::Value) -> String {
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

fn csv_escape(s: &str) -> String {
    if s.contains('"') || s.contains(',') || s.contains('\r') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// 对齐 JS `String(value)`（用于 exportValue 的取键/正则判断）。
fn json_to_string(v: &serde_json::Value) -> String {
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

fn export_cell(
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

pub(crate) async fn use_database(conn: &mut Conn, database: &str) -> Result<(), String> {
    let escaped = database.replace('`', "``");
    conn.query_drop(format!("USE `{}`", escaped))
        .await
        .map_err(|e| e.to_string())
}

/// 查询某库的表（名 + 注释）。filter 非空时按表名/注释模糊匹配。
/// 供 mysql_tables 命令与 llm.rs 的 search_tables 工具共用。
pub(crate) async fn fetch_tables(
    conn: &mut Conn,
    database: &str,
    filter: Option<&str>,
) -> Result<Vec<TableInfo>, String> {
    let mut sql = String::from(
        "SELECT TABLE_NAME as tableName, TABLE_COMMENT as tableComment \
         FROM information_schema.TABLES WHERE TABLE_SCHEMA = ?",
    );
    let mut params: Vec<Value> = vec![Value::Bytes(database.as_bytes().to_vec())];
    if let Some(f) = filter.filter(|f| !f.is_empty()) {
        sql.push_str(" AND (TABLE_NAME LIKE ? OR TABLE_COMMENT LIKE ?)");
        let like = format!("%{}%", f);
        params.push(Value::Bytes(like.clone().into_bytes()));
        params.push(Value::Bytes(like.into_bytes()));
    }

    let mut result = conn
        .exec_iter(sql, mysql_async::Params::Positional(params))
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        let table_name = row
            .as_ref(0)
            .map(|v| json_to_string(&value_to_json(v, false)))
            .unwrap_or_default();
        let table_comment = match row.as_ref(1) {
            Some(Value::NULL) | None => None,
            Some(v) => Some(json_to_string(&value_to_json(v, false))),
        };
        out.push(TableInfo {
            table_name,
            table_comment,
        });
    }
    result.drop_result().await.map_err(|e| e.to_string())?;
    Ok(out)
}

/// 查询某库某表的列（名 + 注释 + 类型）。
/// 供 mysql_columns 命令与 llm.rs 的 get_table_columns 工具共用。
pub(crate) async fn fetch_columns(
    conn: &mut Conn,
    database: &str,
    table: &str,
) -> Result<Vec<ColumnInfo>, String> {
    let sql = "SELECT COLUMN_NAME as columnName, COLUMN_COMMENT as columnComment, DATA_TYPE as dataType \
               FROM information_schema.COLUMNS \
               WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION";
    let params = mysql_async::Params::Positional(vec![
        Value::Bytes(database.as_bytes().to_vec()),
        Value::Bytes(table.as_bytes().to_vec()),
    ]);
    let mut result = conn.exec_iter(sql, params).await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        let column_name = row
            .as_ref(0)
            .map(|v| json_to_string(&value_to_json(v, false)))
            .unwrap_or_default();
        let column_comment = match row.as_ref(1) {
            Some(Value::NULL) | None => None,
            Some(v) => Some(json_to_string(&value_to_json(v, false))),
        };
        let data_type = match row.as_ref(2) {
            Some(Value::NULL) | None => None,
            Some(v) => Some(json_to_string(&value_to_json(v, false))),
        };
        out.push(ColumnInfo {
            column_name,
            column_comment,
            data_type,
        });
    }
    result.drop_result().await.map_err(|e| e.to_string())?;
    Ok(out)
}

#[tauri::command]
pub async fn mysql_databases(
    state: State<'_, AppState>,
    conn_id: String,
) -> Result<Vec<String>, String> {
    let conn = state.get_mysql(&conn_id).await?;
    let mut guard = conn.lock().await;
    let mut result = guard
        .query_iter("SHOW DATABASES")
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        let name = row
            .as_ref(0)
            .map(|v| match v {
                Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
                other => json_to_string(&value_to_json(other, false)),
            })
            .unwrap_or_default();
        out.push(name);
    }
    result.drop_result().await.map_err(|e| e.to_string())?;
    Ok(out)
}

#[tauri::command]
pub async fn mysql_tables(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    filter: Option<String>,
) -> Result<Vec<TableInfo>, String> {
    let conn = state.get_mysql(&conn_id).await?;
    let mut guard = conn.lock().await;
    use_database(&mut guard, &database).await?;
    fetch_tables(&mut guard, &database, filter.as_deref()).await
}

#[tauri::command]
pub async fn mysql_columns(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    table: String,
) -> Result<Vec<ColumnInfo>, String> {
    let conn = state.get_mysql(&conn_id).await?;
    let mut guard = conn.lock().await;
    fetch_columns(&mut guard, &database, &table).await
}

#[tauri::command]
pub async fn mysql_query(
    state: State<'_, AppState>,
    conn_id: String,
    sql: String,
    database: Option<String>,
    limit: i64,
    offset: i64,
) -> Result<QueryResponse, String> {
    let conn = state.get_mysql(&conn_id).await?;
    let mut guard = conn.lock().await;
    if let Some(db) = database.as_ref().filter(|d| !d.is_empty()) {
        use_database(&mut guard, db).await?;
    }
    let (final_sql, paged) = build_paged_sql(&sql, limit, offset);
    let mut result = guard
        .query_iter(final_sql)
        .await
        .map_err(|e| e.to_string())?;

    if result.is_empty() {
        let affected = result.affected_rows();
        result.drop_result().await.map_err(|e| e.to_string())?;
        return Ok(QueryResponse {
            write: true,
            rows: Vec::new(),
            fields: Vec::new(),
            affected,
            paged: false,
        });
    }

    let fields: Vec<FieldInfo> = result
        .columns_ref()
        .iter()
        .map(|c| FieldInfo {
            name: c.name_str().into_owned(),
        })
        .collect();

    let mut rows = Vec::new();
    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        rows.push(row_to_json(&row));
    }
    result.drop_result().await.map_err(|e| e.to_string())?;

    Ok(QueryResponse {
        write: false,
        rows,
        fields,
        affected: 0,
        paged,
    })
}

#[tauri::command]
pub async fn mysql_export(
    state: State<'_, AppState>,
    conn_id: String,
    sql: String,
    database: Option<String>,
    headers: Vec<String>,
    columns: Vec<ExportColumn>,
    path: String,
) -> Result<u64, String> {
    if columns.is_empty() {
        return Err("缺少导出列信息".to_string());
    }
    let conn = state.get_mysql(&conn_id).await?;
    let mut guard = conn.lock().await;
    if let Some(db) = database.as_ref().filter(|d| !d.is_empty()) {
        use_database(&mut guard, db).await?;
    }
    let base_sql = strip_trailing_semicolons(&sql);

    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::new(file);

    // UTF-8 BOM
    writer
        .write_all("\u{feff}".as_bytes())
        .map_err(|e| e.to_string())?;

    let header_line = columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let label = headers
                .get(i)
                .cloned()
                .unwrap_or_else(|| c.name.clone());
            csv_escape(&label)
        })
        .collect::<Vec<_>>()
        .join(",");
    writer
        .write_all(header_line.as_bytes())
        .map_err(|e| e.to_string())?;
    writer.write_all(b"\r\n").map_err(|e| e.to_string())?;

    let mut result = guard
        .query_iter(base_sql)
        .await
        .map_err(|e| e.to_string())?;
    let mut count: u64 = 0;
    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        let obj = row_to_json(&row);
        let empty = serde_json::Value::Null;
        let line = columns
            .iter()
            .map(|c| {
                let cell = obj.get(&c.name).unwrap_or(&empty);
                export_cell(cell, &c.mode, c.enum_map.as_ref())
            })
            .collect::<Vec<_>>()
            .join(",");
        writer
            .write_all(line.as_bytes())
            .map_err(|e| e.to_string())?;
        writer.write_all(b"\r\n").map_err(|e| e.to_string())?;
        count += 1;
    }
    result.drop_result().await.map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;

    Ok(count)
}

// ===== 表结构设计器 / 导出 SQL / DDL 执行 =====

/// 从结果行取字符串（NULL / 缺失 → None；空串保留为空串）。
fn opt_string(row: &Row, idx: usize) -> Option<String> {
    match row.as_ref(idx) {
        Some(Value::NULL) | None => None,
        Some(v) => Some(json_to_string(&value_to_json(v, false))),
    }
}

fn req_string(row: &Row, idx: usize) -> String {
    opt_string(row, idx).unwrap_or_default()
}

fn opt_i64(row: &Row, idx: usize) -> Option<i64> {
    match row.as_ref(idx) {
        Some(Value::Int(v)) => Some(*v),
        Some(Value::UInt(v)) => Some(*v as i64),
        Some(Value::Bytes(b)) => String::from_utf8_lossy(b).trim().parse().ok(),
        _ => None,
    }
}

/// SHOW CREATE TABLE，返回第 2 列（建表语句）；视图返回 CREATE VIEW 语句。
pub(crate) async fn fetch_table_ddl(
    conn: &mut Conn,
    database: &str,
    table: &str,
) -> Result<String, String> {
    let sql = format!(
        "SHOW CREATE TABLE `{}`.`{}`",
        database.replace('`', "``"),
        table.replace('`', "``")
    );
    let mut result = conn.query_iter(sql).await.map_err(|e| e.to_string())?;
    let mut ddl = String::new();
    if let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        ddl = opt_string(&row, 1).unwrap_or_default();
    }
    result.drop_result().await.map_err(|e| e.to_string())?;
    Ok(ddl)
}

/// 读取表的完整结构：字段 / 索引 / 外键 / 表选项。
pub(crate) async fn fetch_table_structure(
    conn: &mut Conn,
    database: &str,
    table: &str,
) -> Result<TableStructure, String> {
    let params = || {
        mysql_async::Params::Positional(vec![
            Value::Bytes(database.as_bytes().to_vec()),
            Value::Bytes(table.as_bytes().to_vec()),
        ])
    };

    // 字段（用 COLUMN_TYPE 才能带上 unsigned / 长度 / enum 值）
    const COL_SQL: &str = "SELECT COLUMN_NAME, COLUMN_TYPE, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT, \
                   COLUMN_KEY, EXTRA, COLUMN_COMMENT, CHARACTER_SET_NAME, COLLATION_NAME, \
                   ORDINAL_POSITION, GENERATION_EXPRESSION \
                   FROM information_schema.COLUMNS \
                   WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION";
    // MySQL 5.6 及更早没有 GENERATION_EXPRESSION 列，老版本也不存在生成列，故直接省略该列
    const COL_SQL_LEGACY: &str = "SELECT COLUMN_NAME, COLUMN_TYPE, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT, \
                   COLUMN_KEY, EXTRA, COLUMN_COMMENT, CHARACTER_SET_NAME, COLLATION_NAME, \
                   ORDINAL_POSITION \
                   FROM information_schema.COLUMNS \
                   WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION";

    let mut result = match conn.exec_iter(COL_SQL, params()).await {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("1054") || msg.contains("Unknown column") {
                conn.exec_iter(COL_SQL_LEGACY, params())
                    .await
                    .map_err(|e| e.to_string())?
            } else {
                return Err(msg);
            }
        }
    };
    let mut columns = Vec::new();
    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        let generation = opt_string(&row, 11).filter(|s| !s.is_empty());
        columns.push(ColumnDef {
            name: req_string(&row, 0),
            column_type: req_string(&row, 1),
            data_type: req_string(&row, 2),
            nullable: req_string(&row, 3).eq_ignore_ascii_case("YES"),
            default: opt_string(&row, 4),
            column_key: opt_string(&row, 5),
            extra: req_string(&row, 6),
            comment: opt_string(&row, 7),
            charset: opt_string(&row, 8),
            collation: opt_string(&row, 9),
            generation_expression: generation,
            ordinal: opt_i64(&row, 10).unwrap_or(0),
        });
    }
    result.drop_result().await.map_err(|e| e.to_string())?;

    // 索引（按名称分组，保持首次出现顺序）
    let idx_sql = "SELECT INDEX_NAME, NON_UNIQUE, INDEX_TYPE, COLUMN_NAME \
                   FROM information_schema.STATISTICS \
                   WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
                   ORDER BY INDEX_NAME, SEQ_IN_INDEX";
    let mut result = conn.exec_iter(idx_sql, params()).await.map_err(|e| e.to_string())?;
    let mut indexes: Vec<IndexDef> = Vec::new();
    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        let name = req_string(&row, 0);
        let unique = opt_i64(&row, 1).unwrap_or(1) == 0;
        let index_type = req_string(&row, 2);
        let column = opt_string(&row, 3);
        match indexes.iter_mut().find(|ix| ix.name == name) {
            Some(ix) => {
                if unique {
                    ix.unique = true;
                }
                if let Some(c) = column {
                    ix.columns.push(c);
                }
            }
            None => indexes.push(IndexDef {
                name,
                unique,
                index_type,
                columns: column.into_iter().collect(),
            }),
        }
    }
    result.drop_result().await.map_err(|e| e.to_string())?;

    // 外键（按约束名分组；KEY_COLUMN_USAGE 只给列，规则在 REFERENTIAL_CONSTRAINTS）
    let fk_sql = "SELECT k.CONSTRAINT_NAME, k.COLUMN_NAME, k.REFERENCED_TABLE_NAME, \
                  k.REFERENCED_COLUMN_NAME, r.DELETE_RULE, r.UPDATE_RULE \
                  FROM information_schema.KEY_COLUMN_USAGE k \
                  JOIN information_schema.REFERENTIAL_CONSTRAINTS r \
                    ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA \
                   AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME \
                   AND r.TABLE_NAME = k.TABLE_NAME \
                  WHERE k.TABLE_SCHEMA = ? AND k.TABLE_NAME = ? \
                    AND k.REFERENCED_TABLE_NAME IS NOT NULL \
                  ORDER BY k.CONSTRAINT_NAME, k.ORDINAL_POSITION";
    let mut result = conn.exec_iter(fk_sql, params()).await.map_err(|e| e.to_string())?;
    let mut foreign_keys: Vec<ForeignKeyDef> = Vec::new();
    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        let name = req_string(&row, 0);
        let column = req_string(&row, 1);
        let ref_table = req_string(&row, 2);
        let ref_column = req_string(&row, 3);
        let on_delete = req_string(&row, 4);
        let on_update = req_string(&row, 5);
        match foreign_keys.iter_mut().find(|fk| fk.name == name) {
            Some(fk) => {
                fk.columns.push(column);
                fk.ref_columns.push(ref_column);
            }
            None => foreign_keys.push(ForeignKeyDef {
                name,
                columns: vec![column],
                ref_table,
                ref_columns: vec![ref_column],
                on_delete,
                on_update,
            }),
        }
    }
    result.drop_result().await.map_err(|e| e.to_string())?;

    // 表选项（字符集由排序规则名截取第一个下划线之前得到）
    let opt_sql = "SELECT ENGINE, TABLE_COLLATION, TABLE_COMMENT, AUTO_INCREMENT \
                   FROM information_schema.TABLES \
                   WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?";
    let mut result = conn.exec_iter(opt_sql, params()).await.map_err(|e| e.to_string())?;
    let mut options = TableOptionDef {
        engine: None,
        charset: None,
        collation: None,
        comment: None,
        auto_increment: None,
    };
    if let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        let collation = opt_string(&row, 1);
        let charset = collation
            .as_deref()
            .and_then(|c| c.split('_').next())
            .map(|s| s.to_string());
        options = TableOptionDef {
            engine: opt_string(&row, 0),
            charset,
            collation,
            comment: opt_string(&row, 2),
            auto_increment: opt_i64(&row, 3),
        };
    }
    result.drop_result().await.map_err(|e| e.to_string())?;

    Ok(TableStructure {
        columns,
        indexes,
        foreign_keys,
        options,
    })
}

/// SQL 字符串字面量转义（MySQL 反斜杠风格）。
fn escape_sql_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\0' => out.push_str("\\0"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{1a}' => out.push_str("\\Z"),
            c => out.push(c),
        }
    }
    out
}

/// 把一个结果值渲染为 INSERT 用的 SQL 字面量。
fn sql_literal(v: &Value) -> String {
    match v {
        Value::NULL => "NULL".to_string(),
        Value::Int(i) => i.to_string(),
        Value::UInt(u) => u.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Double(d) => d.to_string(),
        Value::Date(y, mo, d, h, mi, s, us) => {
            format!("'{}'", format_date(*y, *mo, *d, *h, *mi, *s, *us))
        }
        Value::Time(neg, days, h, m, s, us) => {
            format!("'{}'", format_time(*neg, *days, *h, *m, *s, *us))
        }
        Value::Bytes(bytes) => match std::str::from_utf8(bytes) {
            Ok(s) => format!("'{}'", escape_sql_string(s)),
            Err(_) => {
                let mut hex = String::with_capacity(bytes.len() * 2);
                for b in bytes {
                    hex.push_str(&format!("{:02X}", b));
                }
                format!("X'{}'", hex)
            }
        },
    }
}

fn flush_insert<W: Write>(writer: &mut W, header: &str, rows: &[String]) -> Result<(), String> {
    writer.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
    writer.write_all(rows.join(",\n").as_bytes()).map_err(|e| e.to_string())?;
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

#[tauri::command]
pub async fn mysql_table_ddl(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    table: String,
) -> Result<String, String> {
    let conn = state.get_mysql(&conn_id).await?;
    let mut guard = conn.lock().await;
    use_database(&mut guard, &database).await?;
    fetch_table_ddl(&mut guard, &database, &table).await
}

#[tauri::command]
pub async fn mysql_table_structure(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    table: String,
) -> Result<TableStructure, String> {
    let conn = state.get_mysql(&conn_id).await?;
    let mut guard = conn.lock().await;
    use_database(&mut guard, &database).await?;
    fetch_table_structure(&mut guard, &database, &table).await
}

#[tauri::command]
pub async fn mysql_export_data_sql(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    table: String,
    path: String,
    with_drop: bool,
    with_create: bool,
    batch_size: i64,
) -> Result<u64, String> {
    let conn = state.get_mysql(&conn_id).await?;
    let mut guard = conn.lock().await;
    use_database(&mut guard, &database).await?;

    let structure = fetch_table_structure(&mut guard, &database, &table).await?;
    // 生成列不能显式插入
    let cols: Vec<String> = structure
        .columns
        .iter()
        .filter(|c| {
            !c.extra.to_ascii_lowercase().contains("generated") && c.generation_expression.is_none()
        })
        .map(|c| c.name.clone())
        .collect();

    let db_q = database.replace('`', "``");
    let tbl_q = table.replace('`', "``");

    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::new(file);

    if with_drop {
        writer
            .write_all(format!("DROP TABLE IF EXISTS `{}`.`{}`;\n", db_q, tbl_q).as_bytes())
            .map_err(|e| e.to_string())?;
    }
    if with_create {
        let ddl = fetch_table_ddl(&mut guard, &database, &table).await?;
        writer
            .write_all(ddl.trim().trim_end_matches(';').as_bytes())
            .map_err(|e| e.to_string())?;
        writer.write_all(b";\n").map_err(|e| e.to_string())?;
    }

    let select_cols = if cols.is_empty() {
        "*".to_string()
    } else {
        cols.iter()
            .map(|c| format!("`{}`", c.replace('`', "``")))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let select_sql = format!("SELECT {} FROM `{}`.`{}`", select_cols, db_q, tbl_q);

    let batch = if batch_size > 0 { batch_size as usize } else { 1000 };
    let byte_cap = 4 * 1024 * 1024usize;

    let mut result = guard.query_iter(select_sql).await.map_err(|e| e.to_string())?;
    let names: Vec<String> = if cols.is_empty() {
        result
            .columns_ref()
            .iter()
            .map(|c| c.name_str().into_owned())
            .collect()
    } else {
        cols.clone()
    };
    let header = format!(
        "INSERT INTO `{}`.`{}` ({}) VALUES\n",
        db_q,
        tbl_q,
        names
            .iter()
            .map(|c| format!("`{}`", c.replace('`', "``")))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let ncols = names.len();
    let mut pending: Vec<String> = Vec::new();
    let mut pending_bytes = 0usize;
    let mut count: u64 = 0;

    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        let tuple = (0..ncols)
            .map(|i| row.as_ref(i).map(sql_literal).unwrap_or_else(|| "NULL".to_string()))
            .collect::<Vec<_>>()
            .join(", ");
        let line = format!("({})", tuple);
        pending_bytes += line.len() + 3;
        pending.push(line);
        if pending.len() >= batch || pending_bytes >= byte_cap {
            flush_insert(&mut writer, &header, &pending)?;
            count += pending.len() as u64;
            pending.clear();
            pending_bytes = 0;
        }
    }
    result.drop_result().await.map_err(|e| e.to_string())?;
    if !pending.is_empty() {
        flush_insert(&mut writer, &header, &pending)?;
        count += pending.len() as u64;
    }
    writer.flush().map_err(|e| e.to_string())?;

    Ok(count)
}

#[tauri::command]
pub async fn mysql_exec_ddl(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    sql: String,
) -> Result<u64, String> {
    let conn = state.get_mysql(&conn_id).await?;
    let mut guard = conn.lock().await;
    use_database(&mut guard, &database).await?;

    let statements = split_sql_statements(&sql);
    if statements.is_empty() {
        return Err("没有可执行的语句".to_string());
    }
    let mut count = 0u64;
    for (idx, stmt) in statements.iter().enumerate() {
        guard
            .query_drop(stmt.as_str())
            .await
            .map_err(|e| format!("第 {} 条语句执行失败：{}", idx + 1, e))?;
        count += 1;
    }
    Ok(count)
}
