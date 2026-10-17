//! PostgreSQL 驱动：基于 `tokio-postgres` 的 simple query（文本协议）实现 [`RelationalDriver`]。
//!
//! 设计取舍：
//! - 采用简单的文本协议（`simple_query`），一次查询即可拿到列名 + 文本值 + 影响行数，
//!   无需为每种类型做映射；所有值以文本返回（NULL 返回 None），展示层可接受。
//! - PostgreSQL 单连接只绑定一个库，因此把 UI 的「库」语义映射为 schema（search_path），
//!   `databases()` 返回可访问的 schema 列表。所有 PostgreSQL 专有逻辑都收敛在本文件。
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio_postgres::{Client, NoTls, SimpleQueryMessage, SimpleQueryRow};

use super::util::{
    build_paged_sql, csv_escape, export_cell, flush_insert, split_sql_statements,
    strip_trailing_semicolons,
};
use super::RelationalDriver;
use crate::models::{
    ColumnDef, ColumnInfo, DbConfig, ExportColumn, FieldInfo, ForeignKeyDef, IndexDef,
    QueryResponse, StatementResult, TableInfo, TableOptionDef, TableStructure,
};

pub struct PostgresDriver {
    client: Mutex<Client>,
    // 连接的后台任务句柄（驱动连接协议的 IO）；持有它避免任务被提前丢弃。
    _conn_task: tokio::task::JoinHandle<()>,
}

impl PostgresDriver {
    pub async fn connect(cfg: &DbConfig, host: &str, port: u16) -> Result<Self, String> {
        let mut config = tokio_postgres::Config::new();
        config.host(host);
        config.port(port);
        config.application_name("dbman");
        config.connect_timeout(Duration::from_secs(8));
        if let Some(user) = cfg.user.as_ref().filter(|u| !u.is_empty()) {
            config.user(user.as_str());
        }
        if let Some(pass) = cfg.password.as_ref() {
            config.password(pass.as_str());
        }
        if let Some(db) = cfg.database.as_ref().filter(|d| !d.is_empty()) {
            config.dbname(db.as_str());
        }
        let (client, connection) = config.connect(NoTls).await.map_err(|e| e.to_string())?;
        let handle = tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok(Self {
            client: Mutex::new(client),
            _conn_task: handle,
        })
    }
}

/// 一次 simple query 的汇总结果。
struct SimpleResult {
    fields: Vec<String>,
    rows: Vec<serde_json::Value>,
    affected: u64,
}

fn collect(messages: Vec<SimpleQueryMessage>) -> SimpleResult {
    let mut fields: Vec<String> = Vec::new();
    let mut rows: Vec<serde_json::Value> = Vec::new();
    let mut affected: u64 = 0;
    for msg in messages {
        match msg {
            SimpleQueryMessage::RowDescription(cols) => {
                fields = cols.iter().map(|c| c.name().to_string()).collect();
            }
            SimpleQueryMessage::Row(row) => rows.push(row_obj(&row)),
            SimpleQueryMessage::CommandComplete(n) => affected += n,
            _ => {}
        }
    }
    SimpleResult {
        fields,
        rows,
        affected,
    }
}

fn row_obj(row: &SimpleQueryRow) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (i, col) in row.columns().iter().enumerate() {
        let value = match row.get(i) {
            Some(s) => serde_json::Value::String(s.to_string()),
            None => serde_json::Value::Null,
        };
        map.insert(col.name().to_string(), value);
    }
    serde_json::Value::Object(map)
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

fn opt_col(row: &serde_json::Value, name: &str) -> Option<String> {
    row.get(name).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn req_col(row: &serde_json::Value, name: &str) -> String {
    opt_col(row, name).unwrap_or_default()
}

fn num_col(row: &serde_json::Value, name: &str) -> Option<u64> {
    opt_col(row, name)
        .and_then(|s| s.trim().parse::<i64>().ok())
        .and_then(|n| u64::try_from(n).ok())
}

#[async_trait]
impl RelationalDriver for PostgresDriver {
    fn kind(&self) -> &'static str {
        "postgres"
    }

    async fn databases(&self) -> Result<Vec<String>, String> {
        let client = self.client.lock().await;
        let sql = "SELECT schema_name FROM information_schema.schemata \
                   WHERE schema_name NOT IN ('information_schema', 'pg_catalog', 'pg_toast') \
                   ORDER BY schema_name";
        let result = collect(client.simple_query(sql).await.map_err(|e| e.to_string())?);
        let mut out = Vec::new();
        for row in &result.rows {
            if let Some(s) = row.get("schema_name").and_then(|v| v.as_str()) {
                out.push(s.to_string());
            }
        }
        Ok(out)
    }

    async fn tables(&self, database: &str, filter: Option<&str>) -> Result<Vec<TableInfo>, String> {
        let client = self.client.lock().await;
        let mut sql = String::from(
            "SELECT c.relname AS table_name, \
                    COALESCE(obj_description(c.oid, 'pg_class'), '') AS table_comment, \
                    CASE c.relkind WHEN 'r' THEN 'BASE TABLE' WHEN 'p' THEN 'BASE TABLE' \
                         WHEN 'v' THEN 'VIEW' WHEN 'm' THEN 'MATERIALIZED VIEW' \
                         WHEN 'f' THEN 'FOREIGN TABLE' ELSE 'TABLE' END AS table_type, \
                    c.reltuples::bigint AS table_rows, \
                    pg_total_relation_size(c.oid)::bigint AS total_length \
             FROM pg_catalog.pg_class c \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relkind IN ('r','p','v','m','f') ",
        );
        sql.push_str(&format!("AND n.nspname = {} ", sql_literal(Some(database))));
        if let Some(f) = filter.filter(|f| !f.is_empty()) {
            let like = sql_literal(Some(&format!("%{}%", f)));
            sql.push_str(&format!(
                "AND (c.relname ILIKE {like} OR obj_description(c.oid, 'pg_class') ILIKE {like}) "
            ));
        }
        sql.push_str("ORDER BY c.relname");

        let result = collect(client.simple_query(&sql).await.map_err(|e| e.to_string())?);
        let mut out = Vec::new();
        for row in &result.rows {
            let total_length = num_col(row, "total_length");
            out.push(TableInfo {
                table_name: req_col(row, "table_name"),
                table_comment: opt_col(row, "table_comment").filter(|s| !s.is_empty()),
                create_time: None,
                update_time: None,
                table_rows: num_col(row, "table_rows"),
                data_length: None,
                index_length: None,
                total_length,
                engine: None,
                charset: None,
                table_type: opt_col(row, "table_type"),
                auto_increment: None,
            });
        }
        Ok(out)
    }

    async fn columns(&self, database: &str, table: &str) -> Result<Vec<ColumnInfo>, String> {
        let client = self.client.lock().await;
        let sql = format!(
            "SELECT a.attname AS column_name, \
                    pg_catalog.format_type(a.atttypid, a.atttypmod) AS data_type, \
                    COALESCE(pg_catalog.col_description(a.attrelid, a.attnum), '') AS column_comment \
             FROM pg_catalog.pg_attribute a \
             JOIN pg_catalog.pg_class c ON c.oid = a.attrelid \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = {} AND c.relname = {} AND a.attnum > 0 AND NOT a.attisdropped \
             ORDER BY a.attnum",
            sql_literal(Some(database)),
            sql_literal(Some(table)),
        );
        let result = collect(client.simple_query(&sql).await.map_err(|e| e.to_string())?);
        let mut out = Vec::new();
        for row in &result.rows {
            out.push(ColumnInfo {
                column_name: req_col(row, "column_name"),
                column_comment: opt_col(row, "column_comment").filter(|s| !s.is_empty()),
                data_type: opt_col(row, "data_type"),
            });
        }
        Ok(out)
    }

    async fn query(
        &self,
        database: Option<&str>,
        sql: &str,
        limit: i64,
        offset: i64,
    ) -> Result<QueryResponse, String> {
        let client = self.client.lock().await;
        if let Some(schema) = database.filter(|s| !s.is_empty()) {
            set_schema(&client, schema).await?;
        }
        let (final_sql, paged) = build_paged_sql(sql, limit, offset);
        let result = collect(
            client
                .simple_query(&final_sql)
                .await
                .map_err(|e| e.to_string())?,
        );
        let is_write = result.fields.is_empty() && result.rows.is_empty();
        if is_write {
            return Ok(QueryResponse {
                write: true,
                rows: Vec::new(),
                fields: Vec::new(),
                affected: result.affected,
                paged: false,
            });
        }
        Ok(QueryResponse {
            write: false,
            fields: result.fields.into_iter().map(|name| FieldInfo { name }).collect(),
            rows: result.rows,
            affected: 0,
            paged,
        })
    }

    async fn export_csv(
        &self,
        database: Option<&str>,
        sql: &str,
        headers: &[String],
        columns: &[ExportColumn],
        path: &str,
    ) -> Result<u64, String> {
        if columns.is_empty() {
            return Err("缺少导出列信息".to_string());
        }
        let client = self.client.lock().await;
        if let Some(schema) = database.filter(|s| !s.is_empty()) {
            set_schema(&client, schema).await?;
        }
        let base_sql = strip_trailing_semicolons(sql);

        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
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

        let result = collect(
            client
                .simple_query(&base_sql)
                .await
                .map_err(|e| e.to_string())?,
        );
        let empty = serde_json::Value::Null;
        let mut count: u64 = 0;
        for obj in &result.rows {
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
    }

    async fn table_ddl(&self, database: &str, table: &str) -> Result<String, String> {
        let client = self.client.lock().await;
        let relkind = fetch_relkind(&client, database, table).await?;
        if relkind.as_deref() == Some("v") || relkind.as_deref() == Some("m") {
            let sql = format!(
                "SELECT pg_get_viewdef({}::regclass, true)",
                sql_literal(Some(&format!(
                    "{}.{}",
                    quote_ident(database),
                    quote_ident(table)
                )))
            );
            let result = collect(client.simple_query(&sql).await.map_err(|e| e.to_string())?);
            let body = result
                .rows
                .first()
                .and_then(|r| r.get("pg_get_viewdef").and_then(|v| v.as_str()))
                .unwrap_or("")
                .trim_end_matches(';')
                .to_string();
            let keyword = if relkind.as_deref() == Some("m") {
                "MATERIALIZED VIEW"
            } else {
                "VIEW"
            };
            return Ok(format!(
                "CREATE OR REPLACE {} {}.{} AS\n{};",
                keyword,
                quote_ident(database),
                quote_ident(table),
                body
            ));
        }
        generate_table_ddl(&client, database, table).await
    }

    async fn table_structure(
        &self,
        database: &str,
        table: &str,
    ) -> Result<TableStructure, String> {
        let client = self.client.lock().await;
        fetch_table_structure(&client, database, table).await
    }

    async fn export_data_sql(
        &self,
        database: &str,
        table: &str,
        path: &str,
        with_drop: bool,
        with_create: bool,
        batch_size: i64,
    ) -> Result<u64, String> {
        let client = self.client.lock().await;
        let structure = fetch_table_structure(&client, database, table).await?;
        let cols: Vec<String> = structure.columns.iter().map(|c| c.name.clone()).collect();

        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        let mut writer = std::io::BufWriter::new(file);

        if with_drop {
            writer
                .write_all(
                    format!(
                        "DROP TABLE IF EXISTS {}.{};\n",
                        quote_ident(database),
                        quote_ident(table)
                    )
                    .as_bytes(),
                )
                .map_err(|e| e.to_string())?;
        }
        if with_create {
            let ddl = generate_table_ddl(&client, database, table).await?;
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
        let select_sql = format!(
            "SELECT {} FROM {}.{}",
            select_cols,
            quote_ident(database),
            quote_ident(table)
        );
        let result = collect(
            client
                .simple_query(&select_sql)
                .await
                .map_err(|e| e.to_string())?,
        );

        let names: Vec<String> = if cols.is_empty() {
            result.fields.clone()
        } else {
            cols.clone()
        };
        let header = format!(
            "INSERT INTO {}.{} ({}) VALUES\n",
            quote_ident(database),
            quote_ident(table),
            names
                .iter()
                .map(|c| quote_ident(c))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let batch = if batch_size > 0 { batch_size as usize } else { 1000 };
        let byte_cap = 4 * 1024 * 1024usize;
        let mut pending: Vec<String> = Vec::new();
        let mut pending_bytes = 0usize;
        let mut count: u64 = 0;
        for obj in &result.rows {
            let tuple = names
                .iter()
                .map(|n| sql_literal(obj.get(n).and_then(|v| v.as_str())))
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
    }

    async fn exec_script(&self, database: &str, sql: &str) -> Result<Vec<StatementResult>, String> {
        let client = self.client.lock().await;
        if !database.is_empty() {
            set_schema(&client, database).await?;
        }
        let statements = split_sql_statements(sql);
        if statements.is_empty() {
            return Err("没有可执行的语句".to_string());
        }
        // 按条执行并记录每条结果：失败即停，剩余语句标记 skipped
        let mut out: Vec<StatementResult> = Vec::with_capacity(statements.len());
        let mut stopped = false;
        for stmt in statements {
            if stopped {
                out.push(StatementResult::skipped(stmt));
                continue;
            }
            let started = std::time::Instant::now();
            let res = match client.simple_query(stmt.as_str()).await {
                Ok(msgs) => {
                    let r = collect(msgs);
                    Ok((r.rows.len() as u64, r.affected))
                }
                Err(e) => Err(e.to_string()),
            };
            match res {
                Ok((rows, affected)) => {
                    let ms = started.elapsed().as_millis() as u64;
                    out.push(StatementResult::success(stmt, rows, affected, ms));
                }
                Err(e) => {
                    let ms = started.elapsed().as_millis() as u64;
                    out.push(StatementResult::error(stmt, e, ms));
                    stopped = true;
                }
            }
        }
        Ok(out)
    }

    async fn distinct_values(
        &self,
        database: &str,
        table: &str,
        column: &str,
        keyword: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<String>, bool), String> {
        let client = self.client.lock().await;
        if !database.is_empty() {
            set_schema(&client, database).await?;
        }
        let lim = limit.clamp(1, 100);
        let t = quote_ident(table);
        let c = quote_ident(column);
        let mut sql = format!("SELECT DISTINCT {c} FROM {t} WHERE {c} IS NOT NULL AND {c} <> ''");
        if let Some(k) = keyword.filter(|k| !k.is_empty()) {
            sql.push_str(&format!(
                " AND {c}::text ILIKE {}",
                sql_literal(Some(&format!("%{}%", k)))
            ));
        }
        sql.push_str(&format!(" ORDER BY {c} LIMIT {}", lim + 1));

        let result = collect(client.simple_query(&sql).await.map_err(|e| e.to_string())?);
        let mut out: Vec<String> = Vec::new();
        for obj in &result.rows {
            if let Some(s) = obj.get(column).and_then(|v| v.as_str()) {
                out.push(s.chars().take(64).collect());
            }
        }
        let truncated = out.len() > lim;
        out.truncate(lim);
        Ok((out, truncated))
    }

    async fn truncate_table(&self, database: &str, table: &str) -> Result<(), String> {
        let client = self.client.lock().await;
        let sql = if database.is_empty() {
            format!("TRUNCATE TABLE {}", quote_ident(table))
        } else {
            format!(
                "TRUNCATE TABLE {}.{}",
                quote_ident(database),
                quote_ident(table)
            )
        };
        client
            .simple_query(&sql)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn insert_rows(
        &self,
        database: &str,
        table: &str,
        columns: &[String],
        rows: &[Vec<Option<String>>],
    ) -> Result<(), String> {
        if rows.is_empty() {
            return Ok(());
        }
        let client = self.client.lock().await;
        if !database.is_empty() {
            set_schema(&client, database).await?;
        }
        let col_list = columns
            .iter()
            .map(|c| quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        let prefix = format!("INSERT INTO {} ({}) VALUES\n", quote_ident(table), col_list);

        // 每 1000 行一批，避免单条 SQL 过大
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
            client
                .simple_query(&format!("{}{}", prefix, values))
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// 设置会话 search_path，使未限定的表名解析到指定 schema。
async fn set_schema(client: &Client, schema: &str) -> Result<(), String> {
    client
        .simple_query(&format!("SET search_path TO {}", quote_ident(schema)))
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

async fn fetch_relkind(client: &Client, schema: &str, table: &str) -> Result<Option<String>, String> {
    let sql = format!(
        "SELECT c.relkind::text AS relkind FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = {} AND c.relname = {}",
        sql_literal(Some(schema)),
        sql_literal(Some(table)),
    );
    let result = collect(client.simple_query(&sql).await.map_err(|e| e.to_string())?);
    Ok(result
        .rows
        .first()
        .and_then(|r| r.get("relkind").and_then(|v| v.as_str()).map(|s| s.to_string())))
}

/// 从 pg_catalog 元数据拼出 CREATE TABLE 语句。
async fn generate_table_ddl(client: &Client, schema: &str, table: &str) -> Result<String, String> {
    let col_sql = format!(
        "SELECT a.attname AS column_name, \
                pg_catalog.format_type(a.atttypid, a.atttypmod) AS data_type, \
                a.attnotnull AS not_null, \
                pg_get_expr(ad.adbin, ad.adrelid) AS column_default \
         FROM pg_attribute a \
         JOIN pg_class c ON c.oid = a.attrelid \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         LEFT JOIN pg_attrdef ad ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum \
         WHERE n.nspname = {} AND c.relname = {} AND a.attnum > 0 AND NOT a.attisdropped \
         ORDER BY a.attnum",
        sql_literal(Some(schema)),
        sql_literal(Some(table)),
    );
    let cols = collect(client.simple_query(&col_sql).await.map_err(|e| e.to_string())?);

    let con_sql = format!(
        "SELECT con.conname AS conname, pg_get_constraintdef(con.oid) AS condef \
         FROM pg_constraint con \
         JOIN pg_class c ON c.oid = con.conrelid \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = {} AND c.relname = {} \
         ORDER BY con.conname",
        sql_literal(Some(schema)),
        sql_literal(Some(table)),
    );
    let cons = collect(client.simple_query(&con_sql).await.map_err(|e| e.to_string())?);

    let mut parts: Vec<String> = Vec::new();
    for row in &cols.rows {
        let name = req_col(row, "column_name");
        let data_type = req_col(row, "data_type");
        let not_null = row
            .get("not_null")
            .map(|v| v == "true" || v == "t")
            .unwrap_or(false);
        let default = opt_col(row, "column_default");
        let mut line = format!("    {} {}", quote_ident(&name), data_type);
        if let Some(d) = default.filter(|d| !d.is_empty()) {
            line.push_str(&format!(" DEFAULT {}", d));
        }
        if not_null {
            line.push_str(" NOT NULL");
        }
        parts.push(line);
    }
    for row in &cons.rows {
        let name = req_col(row, "conname");
        let def = req_col(row, "condef");
        parts.push(format!("    CONSTRAINT {} {}", quote_ident(&name), def));
    }

    Ok(format!(
        "CREATE TABLE {}.{} (\n{}\n);",
        quote_ident(schema),
        quote_ident(table),
        parts.join(",\n")
    ))
}

async fn fetch_table_structure(
    client: &Client,
    schema: &str,
    table: &str,
) -> Result<TableStructure, String> {
    // 主键列集合（用于标注 column_key）
    let pk_sql = format!(
        "SELECT a.attname AS column_name \
         FROM pg_index ix \
         JOIN pg_class c ON c.oid = ix.indrelid \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(ix.indkey) \
         WHERE n.nspname = {} AND c.relname = {} AND ix.indisprimary",
        sql_literal(Some(schema)),
        sql_literal(Some(table)),
    );
    let pk = collect(client.simple_query(&pk_sql).await.map_err(|e| e.to_string())?);
    let pk_cols: Vec<String> = pk
        .rows
        .iter()
        .map(|r| req_col(r, "column_name"))
        .filter(|s| !s.is_empty())
        .collect();

    let col_sql = format!(
        "SELECT a.attname AS column_name, \
                pg_catalog.format_type(a.atttypid, a.atttypmod) AS column_type, \
                t.typname AS base_type, \
                a.attnotnull AS not_null, \
                pg_get_expr(ad.adbin, ad.adrelid) AS column_default, \
                COALESCE(pg_catalog.col_description(a.attrelid, a.attnum), '') AS column_comment, \
                a.attnum AS ordinal \
         FROM pg_attribute a \
         JOIN pg_class c ON c.oid = a.attrelid \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_type t ON t.oid = a.atttypid \
         LEFT JOIN pg_attrdef ad ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum \
         WHERE n.nspname = {} AND c.relname = {} AND a.attnum > 0 AND NOT a.attisdropped \
         ORDER BY a.attnum",
        sql_literal(Some(schema)),
        sql_literal(Some(table)),
    );
    let col_res = collect(client.simple_query(&col_sql).await.map_err(|e| e.to_string())?);
    let mut columns = Vec::new();
    for row in &col_res.rows {
        let name = req_col(row, "column_name");
        let column_type = req_col(row, "column_type");
        let base_type = req_col(row, "base_type");
        columns.push(ColumnDef {
            column_key: if pk_cols.contains(&name) {
                Some("PRI".to_string())
            } else {
                None
            },
            name,
            column_type,
            data_type: base_type,
            nullable: !row.get("not_null").map(|v| v == "true" || v == "t").unwrap_or(false),
            default: opt_col(row, "column_default").filter(|s| !s.is_empty()),
            extra: String::new(),
            comment: opt_col(row, "column_comment").filter(|s| !s.is_empty()),
            charset: None,
            collation: None,
            generation_expression: None,
            ordinal: row
                .get("ordinal")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0),
        });
    }

    // 索引
    let idx_sql = format!(
        "SELECT i.relname AS index_name, ix.indisunique AS is_unique, \
                ix.indisprimary AS is_primary, am.amname AS method, a.attname AS column_name \
         FROM pg_index ix \
         JOIN pg_class i ON i.oid = ix.indexrelid \
         JOIN pg_class c ON c.oid = ix.indrelid \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_am am ON am.oid = i.relam \
         JOIN unnest(ix.indkey) WITH ORDINALITY AS k(attnum, ord) ON true \
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = k.attnum \
         WHERE n.nspname = {} AND c.relname = {} \
         ORDER BY i.relname, k.ord",
        sql_literal(Some(schema)),
        sql_literal(Some(table)),
    );
    let idx_res = collect(client.simple_query(&idx_sql).await.map_err(|e| e.to_string())?);
    let mut indexes: Vec<IndexDef> = Vec::new();
    for row in &idx_res.rows {
        let name = req_col(row, "index_name");
        let unique = row.get("is_unique").map(|v| v == "true" || v == "t").unwrap_or(false)
            || row.get("is_primary").map(|v| v == "true" || v == "t").unwrap_or(false);
        let method = req_col(row, "method").to_ascii_uppercase();
        let column = opt_col(row, "column_name");
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
                index_type: method,
                columns: column.into_iter().collect(),
            }),
        }
    }

    // 外键
    let fk_sql = format!(
        "SELECT con.conname AS conname, a.attname AS column_name, \
                rt.relname AS ref_table, ra.attname AS ref_column, \
                con.confdeltype::text AS on_delete, con.confupdtype::text AS on_update \
         FROM pg_constraint con \
         JOIN pg_class c ON c.oid = con.conrelid \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_class rt ON rt.oid = con.confrelid \
         JOIN unnest(con.conkey) WITH ORDINALITY AS ck(attnum, ord) ON true \
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ck.attnum \
         JOIN unnest(con.confkey) WITH ORDINALITY AS fk(attnum, ord) ON fk.ord = ck.ord \
         JOIN pg_attribute ra ON ra.attrelid = rt.oid AND ra.attnum = fk.attnum \
         WHERE con.contype = 'f' AND n.nspname = {} AND c.relname = {} \
         ORDER BY con.conname, ck.ord",
        sql_literal(Some(schema)),
        sql_literal(Some(table)),
    );
    let fk_res = collect(client.simple_query(&fk_sql).await.map_err(|e| e.to_string())?);
    let mut foreign_keys: Vec<ForeignKeyDef> = Vec::new();
    for row in &fk_res.rows {
        let name = req_col(row, "conname");
        let column = req_col(row, "column_name");
        let ref_table = req_col(row, "ref_table");
        let ref_column = req_col(row, "ref_column");
        let on_delete = pg_action(&req_col(row, "on_delete"));
        let on_update = pg_action(&req_col(row, "on_update"));
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

    // 表选项（PostgreSQL 只有注释）
    let opt_sql = format!(
        "SELECT COALESCE(obj_description(c.oid, 'pg_class'), '') AS comment \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = {} AND c.relname = {}",
        sql_literal(Some(schema)),
        sql_literal(Some(table)),
    );
    let opt_res = collect(client.simple_query(&opt_sql).await.map_err(|e| e.to_string())?);
    let options = TableOptionDef {
        engine: None,
        charset: None,
        collation: None,
        comment: opt_res
            .rows
            .first()
            .and_then(|r| opt_col(r, "comment"))
            .filter(|s| !s.is_empty()),
        auto_increment: None,
    };

    Ok(TableStructure {
        columns,
        indexes,
        foreign_keys,
        options,
    })
}

/// 把 PostgreSQL 约束动作码（a/r/c/n/d）翻成可读文本。
fn pg_action(code: &str) -> String {
    match code {
        "c" => "CASCADE",
        "n" => "SET NULL",
        "d" => "SET DEFAULT",
        "r" => "RESTRICT",
        _ => "NO ACTION",
    }
    .to_string()
}

/// 让 `Arc` 在类型签名中可见（驱动以 `Arc<dyn RelationalDriver>` 存储）。
#[allow(dead_code)]
type DriverArc = Arc<dyn RelationalDriver>;
