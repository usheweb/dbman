//! SQLite 驱动：基于 `rusqlite`，连接的是本地数据库文件（无主机/端口）。
//!
//! rusqlite 是同步 API，且 `Connection` 为 `Send` 但非 `Sync`，因此用 `std::sync::Mutex`
//! 包裹连接，并通过 `tokio::task::spawn_blocking` 把阻塞调用移出异步运行时。
//!
//! 设计取舍：SQLite 单文件即单库，UI 的「库」概念无意义，`databases()` 返回 `PRAGMA database_list`
//! （通常只有 `main`）；表清单取自 `sqlite_master`。SQLite 专有逻辑全部收敛在本文件。
use std::io::Write;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, Row};

use super::util::{
    build_paged_sql, csv_escape, export_cell, flush_insert, split_sql_statements,
    strip_trailing_semicolons,
};
use super::RelationalDriver;
use crate::models::{
    ColumnDef, ColumnInfo, DbConfig, ExportColumn, FieldInfo, ForeignKeyDef, IndexDef,
    QueryResponse, TableInfo, TableOptionDef, TableStructure,
};

pub struct SqliteDriver {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteDriver {
    pub fn connect(cfg: &DbConfig) -> Result<Self, String> {
        let path = cfg
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "SQLite 需要提供数据库文件路径".to_string())?;
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 在阻塞线程池里执行需要独占连接的逻辑。
    async fn with_conn<T, F>(&self, f: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, String> + Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let guard = conn.lock().map_err(|_| "SQLite 连接锁已中毒".to_string())?;
            f(&guard)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[async_trait]
impl RelationalDriver for SqliteDriver {
    fn kind(&self) -> &'static str {
        "sqlite"
    }

    async fn databases(&self) -> Result<Vec<String>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("PRAGMA database_list")
                .map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                // 列：seq, name, file
                if let Some(name) = cell_opt(row, 1) {
                    if !name.is_empty() {
                        out.push(name);
                    }
                }
            }
            Ok(out)
        })
        .await
    }

    async fn tables(&self, _database: &str, filter: Option<&str>) -> Result<Vec<TableInfo>, String> {
        let filter = filter.map(|s| s.to_string());
        self.with_conn(move |conn| {
            let mut sql = String::from(
                "SELECT name, type FROM sqlite_master \
                 WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'",
            );
            if let Some(f) = filter.filter(|f| !f.is_empty()) {
                sql.push_str(&format!(
                    " AND name LIKE {}",
                    sql_literal(Some(&format!("%{}%", f)))
                ));
            }
            sql.push_str(" ORDER BY name");

            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let table_type = cell_opt(row, 1).map(|t| {
                    if t.eq_ignore_ascii_case("view") {
                        "VIEW".to_string()
                    } else {
                        "BASE TABLE".to_string()
                    }
                });
                out.push(TableInfo {
                    table_name: cell_text(row, 0),
                    table_comment: None,
                    create_time: None,
                    update_time: None,
                    table_rows: None,
                    data_length: None,
                    index_length: None,
                    total_length: None,
                    engine: None,
                    charset: None,
                    table_type,
                    auto_increment: None,
                });
            }
            Ok(out)
        })
        .await
    }

    async fn columns(&self, _database: &str, table: &str) -> Result<Vec<ColumnInfo>, String> {
        let table = table.to_string();
        self.with_conn(move |conn| {
            let sql = format!("PRAGMA table_info({})", quote_ident(&table));
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                // 列：cid, name, type, notnull, dflt_value, pk
                let data_type = cell_opt(row, 2).filter(|s| !s.is_empty());
                out.push(ColumnInfo {
                    column_name: cell_text(row, 1),
                    column_comment: None,
                    data_type,
                });
            }
            Ok(out)
        })
        .await
    }

    async fn query(
        &self,
        _database: Option<&str>,
        sql: &str,
        limit: i64,
        offset: i64,
    ) -> Result<QueryResponse, String> {
        let (final_sql, paged) = build_paged_sql(sql, limit, offset);
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(&final_sql).map_err(|e| e.to_string())?;
            if stmt.column_count() == 0 {
                let affected = stmt.raw_execute().map_err(|e| e.to_string())? as u64;
                return Ok(QueryResponse {
                    write: true,
                    rows: Vec::new(),
                    fields: Vec::new(),
                    affected,
                    paged: false,
                });
            }
            let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
            let fields: Vec<FieldInfo> = names.iter().map(|n| FieldInfo { name: n.clone() }).collect();

            let mut rows_out = Vec::new();
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let mut map = serde_json::Map::new();
                for (i, name) in names.iter().enumerate() {
                    let value = match row.get_ref(i) {
                        Ok(v) => value_to_json(v),
                        Err(_) => serde_json::Value::Null,
                    };
                    map.insert(name.clone(), value);
                }
                rows_out.push(serde_json::Value::Object(map));
            }
            Ok(QueryResponse {
                write: false,
                rows: rows_out,
                fields,
                affected: 0,
                paged,
            })
        })
        .await
    }

    async fn export_csv(
        &self,
        _database: Option<&str>,
        sql: &str,
        headers: &[String],
        columns: &[ExportColumn],
        path: &str,
    ) -> Result<u64, String> {
        if columns.is_empty() {
            return Err("缺少导出列信息".to_string());
        }
        let base_sql = strip_trailing_semicolons(sql);
        let path = path.to_string();
        let headers: Vec<String> = headers.to_vec();
        let columns: Vec<ExportColumn> = columns.to_vec();

        self.with_conn(move |conn| {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            let mut writer = std::io::BufWriter::new(file);
            writer
                .write_all("\u{feff}".as_bytes())
                .map_err(|e| e.to_string())?;

            let header_line = columns
                .iter()
                .enumerate()
                .map(|(i, c)| csv_escape(headers.get(i).unwrap_or(&c.name)))
                .collect::<Vec<_>>()
                .join(",");
            writer
                .write_all(header_line.as_bytes())
                .map_err(|e| e.to_string())?;
            writer.write_all(b"\r\n").map_err(|e| e.to_string())?;

            let mut stmt = conn.prepare(&base_sql).map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let empty = serde_json::Value::Null;
            let mut count: u64 = 0;
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let mut obj = serde_json::Map::new();
                for (i, name) in columns.iter().enumerate() {
                    let value = match row.get_ref(i) {
                        Ok(v) => value_to_json(v),
                        Err(_) => serde_json::Value::Null,
                    };
                    obj.insert(name.name.clone(), value);
                }
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
            writer.flush().map_err(|e| e.to_string())?;
            Ok(count)
        })
        .await
    }

    async fn table_ddl(&self, _database: &str, table: &str) -> Result<String, String> {
        let table = table.to_string();
        self.with_conn(move |conn| fetch_ddl(conn, &table)).await
    }

    async fn table_structure(
        &self,
        _database: &str,
        table: &str,
    ) -> Result<TableStructure, String> {
        let table = table.to_string();
        self.with_conn(move |conn| {
            Ok(TableStructure {
                columns: fetch_column_defs(conn, &table)?,
                indexes: fetch_indexes(conn, &table)?,
                foreign_keys: fetch_foreign_keys(conn, &table)?,
                options: TableOptionDef {
                    engine: None,
                    charset: None,
                    collation: None,
                    comment: None,
                    auto_increment: None,
                },
            })
        })
        .await
    }

    async fn export_data_sql(
        &self,
        _database: &str,
        table: &str,
        path: &str,
        with_drop: bool,
        with_create: bool,
        batch_size: i64,
    ) -> Result<u64, String> {
        let table = table.to_string();
        let path = path.to_string();
        self.with_conn(move |conn| {
            let columns = fetch_column_defs(conn, &table)?;
            let cols: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();

            let file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            let mut writer = std::io::BufWriter::new(file);

            if with_drop {
                writer
                    .write_all(format!("DROP TABLE IF EXISTS {};\n", quote_ident(&table)).as_bytes())
                    .map_err(|e| e.to_string())?;
            }
            if with_create {
                let ddl = fetch_ddl(conn, &table)?;
                writer
                    .write_all(ddl.trim().trim_end_matches(';').as_bytes())
                    .map_err(|e| e.to_string())?;
                writer.write_all(b";\n").map_err(|e| e.to_string())?;
            }

            let select_cols = if cols.is_empty() {
                "*".to_string()
            } else {
                cols.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ")
            };
            let select_sql = format!("SELECT {} FROM {}", select_cols, quote_ident(&table));

            let mut stmt = conn.prepare(&select_sql).map_err(|e| e.to_string())?;
            let names: Vec<String> = if cols.is_empty() {
                stmt.column_names().iter().map(|s| s.to_string()).collect()
            } else {
                cols.clone()
            };
            let header = format!(
                "INSERT INTO {} ({}) VALUES\n",
                quote_ident(&table),
                names.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ")
            );

            let batch = if batch_size > 0 { batch_size as usize } else { 1000 };
            let byte_cap = 4 * 1024 * 1024usize;
            let mut pending: Vec<String> = Vec::new();
            let mut pending_bytes = 0usize;
            let mut count: u64 = 0;

            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let tuple = (0..names.len())
                    .map(|i| match row.get_ref(i) {
                        Ok(v) => literal_from_value(v),
                        Err(_) => "NULL".to_string(),
                    })
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
            if !pending.is_empty() {
                flush_insert(&mut writer, &header, &pending)?;
                count += pending.len() as u64;
            }
            writer.flush().map_err(|e| e.to_string())?;
            Ok(count)
        })
        .await
    }

    async fn exec_script(&self, _database: &str, sql: &str) -> Result<u64, String> {
        let sql = sql.to_string();
        self.with_conn(move |conn| {
            let statements = split_sql_statements(&sql);
            if statements.is_empty() {
                return Err("没有可执行的语句".to_string());
            }
            let mut count = 0u64;
            for (idx, stmt) in statements.iter().enumerate() {
                conn.execute_batch(stmt)
                    .map_err(|e| format!("第 {} 条语句执行失败：{}", idx + 1, e))?;
                count += 1;
            }
            Ok(count)
        })
        .await
    }

    async fn distinct_values(
        &self,
        _database: &str,
        table: &str,
        column: &str,
        keyword: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<String>, bool), String> {
        let table = table.to_string();
        let column = column.to_string();
        let keyword = keyword.map(|s| s.to_string());
        self.with_conn(move |conn| {
            let lim = limit.clamp(1, 100);
            let t = quote_ident(&table);
            let c = quote_ident(&column);
            let mut sql = format!(
                "SELECT DISTINCT {c} FROM {t} WHERE {c} IS NOT NULL AND CAST({c} AS TEXT) <> ''"
            );
            if let Some(k) = keyword.filter(|k| !k.is_empty()) {
                sql.push_str(&format!(
                    " AND CAST({c} AS TEXT) LIKE {}",
                    sql_literal(Some(&format!("%{}%", k)))
                ));
            }
            sql.push_str(&format!(" ORDER BY {c} LIMIT {}", lim + 1));

            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut out: Vec<String> = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                if let Some(s) = cell_opt(row, 0) {
                    out.push(s.chars().take(64).collect());
                }
            }
            let truncated = out.len() > lim;
            out.truncate(lim);
            Ok((out, truncated))
        })
        .await
    }

    async fn truncate_table(&self, _database: &str, table: &str) -> Result<(), String> {
        let table = table.to_string();
        self.with_conn(move |conn| {
            // SQLite 无 TRUNCATE；DELETE 清空数据。若为 AUTOINCREMENT 表则顺带重置序列。
            let _ = conn.execute("DELETE FROM sqlite_sequence WHERE name = ?1", [table.as_str()]);
            conn.execute(&format!("DELETE FROM {}", quote_ident(&table)), [])
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    }

    async fn insert_rows(
        &self,
        _database: &str,
        table: &str,
        columns: &[String],
        rows: &[Vec<Option<String>>],
    ) -> Result<(), String> {
        if rows.is_empty() {
            return Ok(());
        }
        let table = table.to_string();
        let columns = columns.to_vec();
        let rows = rows.to_vec();
        self.with_conn(move |conn| {
            let col_list = columns.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
            let prefix = format!("INSERT INTO {} ({}) VALUES\n", quote_ident(&table), col_list);
            for chunk in rows.chunks(1000) {
                let values = chunk
                    .iter()
                    .map(|row| {
                        let tuple = row
                            .iter()
                            .map(|c| sql_literal(c.as_deref()))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("({})", tuple)
                    })
                    .collect::<Vec<_>>()
                    .join(",\n");
                conn.execute_batch(&format!("{}{}", prefix, values))
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
        .await
    }
}

/// 双引号包裹标识符（内部双引号加倍）。
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// 单引号字符串字面量（内部单引号加倍）。
fn sql_literal(v: Option<&str>) -> String {
    match v {
        None => "NULL".to_string(),
        Some(s) => format!("'{}'", s.replace('\'', "''")),
    }
}

fn hex_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02X}", b));
    }
    out
}

/// 结果值 → JSON（Blob 以十六进制文本表示）。
fn value_to_json(v: ValueRef<'_>) -> serde_json::Value {
    match v {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(i) => serde_json::json!(i),
        ValueRef::Real(f) => serde_json::json!(f),
        ValueRef::Text(bytes) => {
            serde_json::Value::String(String::from_utf8_lossy(bytes).into_owned())
        }
        ValueRef::Blob(bytes) => serde_json::Value::String(hex_string(bytes)),
    }
}

/// 结果值 → 文本（NULL → ""）。
fn value_to_text(v: ValueRef<'_>) -> String {
    match v {
        ValueRef::Null => String::new(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        ValueRef::Blob(bytes) => hex_string(bytes),
    }
}

/// 结果值 → INSERT 用的 SQL 字面量。
fn literal_from_value(v: ValueRef<'_>) -> String {
    match v {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(bytes) => {
            format!("'{}'", String::from_utf8_lossy(bytes).replace('\'', "''"))
        }
        ValueRef::Blob(bytes) => format!("X'{}'", hex_string(bytes)),
    }
}

fn cell_opt(row: &Row, idx: usize) -> Option<String> {
    match row.get_ref(idx) {
        Ok(ValueRef::Null) | Err(_) => None,
        Ok(v) => Some(value_to_text(v)),
    }
}

fn cell_text(row: &Row, idx: usize) -> String {
    cell_opt(row, idx).unwrap_or_default()
}

fn cell_i64(row: &Row, idx: usize) -> Option<i64> {
    match row.get_ref(idx) {
        Ok(ValueRef::Integer(i)) => Some(i),
        Ok(ValueRef::Real(f)) => Some(f as i64),
        Ok(ValueRef::Text(b)) => std::str::from_utf8(b).ok()?.trim().parse().ok(),
        _ => None,
    }
}

/// 取建表/建视图语句（`sqlite_master.sql`）。
fn fetch_ddl(conn: &Connection, table: &str) -> Result<String, String> {
    let mut stmt = conn
        .prepare("SELECT sql FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?1")
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([table]).map_err(|e| e.to_string())?;
    if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let sql = cell_text(row, 0);
        if sql.is_empty() {
            Ok(String::new())
        } else {
            Ok(format!("{};", sql.trim().trim_end_matches(';')))
        }
    } else {
        Ok(String::new())
    }
}

fn fetch_column_defs(conn: &Connection, table: &str) -> Result<Vec<ColumnDef>, String> {
    let sql = format!("PRAGMA table_info({})", quote_ident(table));
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        // 列：cid, name, type, notnull, dflt_value, pk
        let column_type = cell_text(row, 2);
        let pk = cell_i64(row, 5).unwrap_or(0);
        out.push(ColumnDef {
            name: cell_text(row, 1),
            column_type: column_type.clone(),
            data_type: column_type,
            nullable: cell_i64(row, 3).unwrap_or(0) == 0,
            default: cell_opt(row, 4),
            column_key: if pk > 0 { Some("PRI".to_string()) } else { None },
            extra: String::new(),
            comment: None,
            charset: None,
            collation: None,
            generation_expression: None,
            ordinal: cell_i64(row, 0).unwrap_or(0),
        });
    }
    Ok(out)
}

fn fetch_indexes(conn: &Connection, table: &str) -> Result<Vec<IndexDef>, String> {
    let sql = format!("PRAGMA index_list({})", quote_ident(table));
    let mut names: Vec<(String, bool)> = Vec::new();
    {
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            // 列：seq, name, unique, origin, partial
            names.push((cell_text(row, 1), cell_i64(row, 2).unwrap_or(0) != 0));
        }
    }

    let mut out = Vec::new();
    for (name, unique) in names {
        let info_sql = format!("PRAGMA index_info({})", quote_ident(&name));
        let mut columns = Vec::new();
        {
            let mut stmt = conn.prepare(&info_sql).map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                // 列：seqno, cid, name
                if let Some(c) = cell_opt(row, 2) {
                    columns.push(c);
                }
            }
        }
        out.push(IndexDef {
            name,
            unique,
            index_type: "BTREE".to_string(),
            columns,
        });
    }
    Ok(out)
}

fn fetch_foreign_keys(conn: &Connection, table: &str) -> Result<Vec<ForeignKeyDef>, String> {
    let sql = format!("PRAGMA foreign_key_list({})", quote_ident(table));
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    let mut out: Vec<ForeignKeyDef> = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        // 列：id, seq, table, from, to, on_update, on_delete, match
        let id = cell_i64(row, 0).unwrap_or(0);
        let name = format!("fk_{}_{}", table, id);
        let column = cell_text(row, 3);
        let ref_table = cell_text(row, 2);
        let ref_column = cell_text(row, 4);
        let on_update = cell_text(row, 5);
        let on_delete = cell_text(row, 6);
        match out.iter_mut().find(|fk| fk.name == name) {
            Some(fk) => {
                fk.columns.push(column);
                fk.ref_columns.push(ref_column);
            }
            None => out.push(ForeignKeyDef {
                name,
                columns: vec![column],
                ref_table,
                ref_columns: vec![ref_column],
                on_delete,
                on_update,
            }),
        }
    }
    Ok(out)
}
