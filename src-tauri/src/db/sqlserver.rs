//! SQL Server 驱动：基于 `tiberius`（TDS 协议）实现 [`RelationalDriver`]。
//!
//! 设计取舍：
//! - 读操作统一走 `simple_query` 文本协议，一次即可拿到列名 + 各列文本值，不必为每种类型建映射表；
//! - 写操作优先用预编译 `execute`（能拿到影响行数），遇到无法预编译的语句（USE / 视图 / 存储过程等）
//!   回退文本协议并排空结果；
//! - UI 的「库」对应 SQL Server 的 database，跨库访问统一用 `[db].[schema].[object]` 三段式限定；
//! - 分页使用 `OFFSET ... FETCH NEXT`（SQL Server 2012+）。本文件收敛全部 SQL Server 专有逻辑。
use std::io::Write;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::TryStreamExt;
use serde_json::{Map, Value};
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel, QueryItem};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::util::{
    csv_escape, export_cell, flush_insert, split_sql_statements, starts_with_select_or_with,
    strip_trailing_semicolons,
};
use super::RelationalDriver;
use crate::models::{
    ColumnDef, ColumnInfo, DbConfig, ExportColumn, FieldInfo, ForeignKeyDef, IndexDef,
    QueryResponse, StatementResult, TableInfo, TableOptionDef, TableStructure,
};

/// tiberius 客户端：TDS over 兼容包装的 TCP 流。
type SsClient = Client<Compat<TcpStream>>;

pub struct SqlServerDriver {
    client: Mutex<SsClient>,
}

impl SqlServerDriver {
    pub async fn connect(cfg: &DbConfig, host: &str, port: u16) -> Result<Self, String> {
        let mut config = Config::new();
        config.host(host);
        config.port(port);
        config.application_name("dbman");
        config.encryption(EncryptionLevel::Required);
        // 自签证书默认跳过校验，否则本地 / 内网实例一律连不上。
        config.trust_cert();
        let user = cfg
            .user
            .clone()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "sa".to_string());
        let password = cfg.password.clone().unwrap_or_default();
        config.authentication(AuthMethod::sql_server(user, password));
        if let Some(db) = cfg.database.as_ref().filter(|d| !d.is_empty()) {
            config.database(db.as_str());
        }
        let addr = config.get_addr();
        let tcp = tokio::time::timeout(Duration::from_secs(8), TcpStream::connect(addr))
            .await
            .map_err(|_| "连接 SQL Server 超时".to_string())?
            .map_err(|e| e.to_string())?;
        tcp.set_nodelay(true).map_err(|e| e.to_string())?;
        let client = Client::connect(config, tcp.compat_write())
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client: Mutex::new(client),
        })
    }
}

// ===== 底层辅助 =====

/// 方括号包裹标识符（内部 `]` 加倍）。
fn bracket(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// 拆出 (schema, table)，默认 schema 为 dbo。
fn split_schema(table: &str) -> (String, String) {
    match table.split_once('.') {
        Some((s, t)) if !s.is_empty() && !t.is_empty() => (s.to_string(), t.to_string()),
        _ => ("dbo".to_string(), table.to_string()),
    }
}

/// 两段式限定名：`[db].[schema].[table]`。
fn qualified(database: &str, table: &str) -> String {
    let (schema, name) = split_schema(table);
    format!("{}.{}.{}", bracket(database), bracket(&schema), bracket(&name))
}

/// Unicode 字符串字面量（内部单引号加倍）。
fn sql_literal(v: Option<&str>) -> String {
    match v {
        None => "NULL".to_string(),
        Some(s) => format!("N'{}'", s.replace('\'', "''")),
    }
}

fn hex_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02X}", b));
    }
    out
}

/// 把 TDS 列值转成展示用文本（NULL → None），对齐其它驱动的「一切皆文本」策略。
fn cell_text(data: &ColumnData<'static>) -> Option<String> {
    use tiberius::FromSql;
    match data {
        ColumnData::U8(v) => v.as_ref().map(|x| x.to_string()),
        ColumnData::I16(v) => v.as_ref().map(|x| x.to_string()),
        ColumnData::I32(v) => v.as_ref().map(|x| x.to_string()),
        ColumnData::I64(v) => v.as_ref().map(|x| x.to_string()),
        ColumnData::F32(v) => v.as_ref().map(|x| x.to_string()),
        ColumnData::F64(v) => v.as_ref().map(|x| x.to_string()),
        ColumnData::Bit(v) => v
            .as_ref()
            .map(|x| if *x { "1".to_string() } else { "0".to_string() }),
        ColumnData::String(v) => v.as_ref().map(|x| x.to_string()),
        ColumnData::Guid(v) => v.as_ref().map(|x| x.to_string()),
        ColumnData::Binary(v) => v.as_ref().map(|x| hex_string(x.as_ref())),
        ColumnData::Numeric(v) => v.as_ref().map(|x| x.to_string()),
        ColumnData::Xml(v) => v.as_ref().map(|x| x.to_string()),
        ColumnData::SmallDateTime(_) | ColumnData::DateTime(_) | ColumnData::DateTime2(_) => {
            <chrono::NaiveDateTime as FromSql>::from_sql(data)
                .ok()
                .flatten()
                .map(|x| x.to_string())
        }
        ColumnData::Time(_) => <chrono::NaiveTime as FromSql>::from_sql(data)
            .ok()
            .flatten()
            .map(|x| x.to_string()),
        ColumnData::Date(_) => <chrono::NaiveDate as FromSql>::from_sql(data)
            .ok()
            .flatten()
            .map(|x| x.to_string()),
        ColumnData::DateTimeOffset(_) => {
            <chrono::DateTime<chrono::Utc> as FromSql>::from_sql(data)
                .ok()
                .flatten()
                .map(|x| x.to_string())
        }
    }
}

/// 执行一条只读语句，返回 (列名, 行)。
async fn run_query(client: &mut SsClient, sql: &str) -> Result<(Vec<String>, Vec<Value>), String> {
    let stream = client.simple_query(sql).await.map_err(|e| e.to_string())?;
    let mut stream = Box::pin(stream);
    let mut fields: Vec<String> = Vec::new();
    let mut rows: Vec<Value> = Vec::new();
    while let Some(item) = stream.try_next().await.map_err(|e| e.to_string())? {
        match item {
            QueryItem::Metadata(meta) => {
                if fields.is_empty() {
                    fields = meta.columns().iter().map(|c| c.name().to_string()).collect();
                }
            }
            QueryItem::Row(row) => {
                if fields.is_empty() {
                    fields = row.columns().iter().map(|c| c.name().to_string()).collect();
                }
                let mut map = Map::new();
                for (col, data) in row.cells() {
                    let value = match cell_text(data) {
                        Some(s) => Value::String(s),
                        None => Value::Null,
                    };
                    map.insert(col.name().to_string(), value);
                }
                rows.push(Value::Object(map));
            }
        }
    }
    Ok((fields, rows))
}

/// 执行一条写语句，返回影响行数；无法预编译的语句回退文本协议并排空结果。
async fn run_write(client: &mut SsClient, sql: &str) -> Result<u64, String> {
    if let Ok(res) = client.execute(sql, &[] as &[&dyn tiberius::ToSql]).await {
        return Ok(res.total());
    }
    let stream = client.simple_query(sql).await.map_err(|e| e.to_string())?;
    stream.into_results().await.map_err(|e| e.to_string())?;
    Ok(0)
}

/// 切换当前连接的默认库（`USE [db]`），排空结果不影响后续操作。
async fn use_database(client: &mut SsClient, database: &str) -> Result<(), String> {
    let sql = format!("USE {}", bracket(database));
    let stream = client.simple_query(sql).await.map_err(|e| e.to_string())?;
    stream.into_results().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// 给 SELECT/WITH 追加 `OFFSET ... FETCH NEXT` 分页；返回 (最终 SQL, 是否分页)。
fn build_paged_sql(sql: &str, limit: i64, offset: i64) -> (String, bool) {
    let base = strip_trailing_semicolons(sql);
    let lim = if limit > 0 { limit } else { 0 };
    let off = if offset > 0 { offset } else { 0 };
    if lim == 0 || !starts_with_select_or_with(&base) {
        return (base, false);
    }
    let lower = base.to_ascii_lowercase();
    // 已自带分页语义（TOP / OFFSET / FETCH）时不重复追加。
    if lower.contains(" offset ") || lower.contains(" fetch ") || lower.contains(" top ") {
        return (base, false);
    }
    let sql = if lower.contains(" order by ") {
        format!("{} OFFSET {} ROWS FETCH NEXT {} ROWS ONLY", base, off, lim)
    } else {
        format!(
            "{} ORDER BY (SELECT NULL) OFFSET {} ROWS FETCH NEXT {} ROWS ONLY",
            base, off, lim
        )
    };
    (sql, true)
}

fn str_of(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn opt_str(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn opt_u64(row: &Value, key: &str) -> Option<u64> {
    row.get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

// ===== RelationalDriver 实现 =====

#[async_trait]
impl RelationalDriver for SqlServerDriver {
    fn kind(&self) -> &'static str {
        "sqlserver"
    }

    async fn databases(&self) -> Result<Vec<String>, String> {
        let mut client = self.client.lock().await;
        let (_, rows) = run_query(&mut client, "SELECT name FROM sys.databases ORDER BY name").await?;
        Ok(rows.iter().map(|r| str_of(r, "name")).collect())
    }

    async fn tables(&self, database: &str, filter: Option<&str>) -> Result<Vec<TableInfo>, String> {
        let mut client = self.client.lock().await;
        let db = bracket(database);
        let filter_sql = match filter.map(str::trim).filter(|f| !f.is_empty()) {
            Some(f) => {
                let kw = f.replace('\'', "''");
                format!(
                    " AND (o.name LIKE N'%{}%' OR CAST(ep.value AS nvarchar(max)) LIKE N'%{}%')",
                    kw, kw
                )
            }
            None => String::new(),
        };
        let sql = format!(
            "SELECT o.name AS table_name, o.type AS table_type, \
                    CAST(ep.value AS nvarchar(max)) AS table_comment, \
                    CONVERT(varchar(19), o.create_date, 120) AS create_time, \
                    CONVERT(varchar(19), o.modify_date, 120) AS update_time, \
                    (SELECT SUM(p.rows) FROM {db}.sys.partitions p \
                     WHERE p.object_id = o.object_id AND p.index_id IN (0, 1)) AS table_rows \
             FROM {db}.sys.objects o \
             LEFT JOIN {db}.sys.extended_properties ep \
                    ON ep.major_id = o.object_id AND ep.minor_id = 0 AND ep.name = 'MS_Description' \
             WHERE o.type IN ('U', 'V') AND o.is_ms_shipped = 0{filter_sql} \
             ORDER BY o.type, o.name",
            db = db,
            filter_sql = filter_sql
        );
        let (_, rows) = run_query(&mut client, &sql).await?;
        Ok(rows
            .iter()
            .map(|r| TableInfo {
                table_name: str_of(r, "table_name"),
                table_comment: opt_str(r, "table_comment"),
                create_time: opt_str(r, "create_time"),
                update_time: opt_str(r, "update_time"),
                table_rows: opt_u64(r, "table_rows"),
                data_length: None,
                index_length: None,
                total_length: None,
                engine: None,
                charset: None,
                table_type: opt_str(r, "table_type").map(|t| {
                    if t == "V" {
                        "VIEW".to_string()
                    } else {
                        "BASE TABLE".to_string()
                    }
                }),
                auto_increment: None,
            })
            .collect())
    }

    async fn columns(&self, database: &str, table: &str) -> Result<Vec<ColumnInfo>, String> {
        let mut client = self.client.lock().await;
        let db = bracket(database);
        let sql = format!(
            "SELECT c.name AS column_name, ty.name AS data_type, \
                    CAST(ep.value AS nvarchar(max)) AS column_comment \
             FROM {db}.sys.columns c \
             JOIN {db}.sys.types ty ON ty.user_type_id = c.user_type_id \
             LEFT JOIN {db}.sys.extended_properties ep \
                    ON ep.major_id = c.object_id AND ep.minor_id = c.column_id \
                   AND ep.name = 'MS_Description' \
             WHERE c.object_id = OBJECT_ID(N'{obj}') \
             ORDER BY c.column_id",
            db = db,
            obj = qualified(database, table)
        );
        let (_, rows) = run_query(&mut client, &sql).await?;
        Ok(rows
            .iter()
            .map(|r| ColumnInfo {
                column_name: str_of(r, "column_name"),
                column_comment: opt_str(r, "column_comment"),
                data_type: opt_str(r, "data_type"),
            })
            .collect())
    }

    async fn query(
        &self,
        database: Option<&str>,
        sql: &str,
        limit: i64,
        offset: i64,
    ) -> Result<QueryResponse, String> {
        let mut client = self.client.lock().await;
        if let Some(db) = database.filter(|d| !d.is_empty()) {
            use_database(&mut client, db).await?;
        }
        let (final_sql, paged) = build_paged_sql(sql, limit, offset);
        // 读 / 写按语句文本判断：写语句若先跑一遍 simple_query 会被重复执行一次。
        let is_read = starts_with_select_or_with(&final_sql);
        let mut fields: Vec<String> = Vec::new();
        let mut rows: Vec<Value> = Vec::new();
        let mut affected: u64 = 0;
        if is_read {
            let result = run_query(&mut client, &final_sql).await?;
            fields = result.0;
            rows = result.1;
        } else {
            affected = run_write(&mut client, &final_sql).await?;
        }
        Ok(QueryResponse {
            write: !is_read || (fields.is_empty() && rows.is_empty()),
            rows,
            fields: fields.into_iter().map(|name| FieldInfo { name }).collect(),
            affected,
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
        let mut client = self.client.lock().await;
        if let Some(db) = database.filter(|d| !d.is_empty()) {
            use_database(&mut client, db).await?;
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

        let (_, rows) = run_query(&mut client, &base_sql).await?;
        let empty = Value::Null;
        let mut count: u64 = 0;
        for obj in &rows {
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
        let mut client = self.client.lock().await;
        if let Some(def) = fetch_view_definition(&mut client, database, table).await? {
            return Ok(def);
        }
        generate_table_ddl(&mut client, database, table).await
    }

    async fn table_structure(
        &self,
        database: &str,
        table: &str,
    ) -> Result<TableStructure, String> {
        let mut client = self.client.lock().await;
        fetch_table_structure(&mut client, database, table).await
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
        let mut client = self.client.lock().await;
        let structure = fetch_table_structure(&mut client, database, table).await?;
        let cols: Vec<String> = structure.columns.iter().map(|c| c.name.clone()).collect();

        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        let mut writer = std::io::BufWriter::new(file);

        let target = qualified(database, table);
        if with_drop {
            writer
                .write_all(
                    format!(
                        "IF OBJECT_ID(N'{}', N'U') IS NOT NULL DROP TABLE {};\n",
                        target.replace('\'', "''"),
                        target
                    )
                    .as_bytes(),
                )
                .map_err(|e| e.to_string())?;
        }
        if with_create {
            let ddl = generate_table_ddl(&mut client, database, table).await?;
            writer
                .write_all(ddl.trim().trim_end_matches(';').as_bytes())
                .map_err(|e| e.to_string())?;
            writer.write_all(b";\n").map_err(|e| e.to_string())?;
        }

        let select_cols = if cols.is_empty() {
            "*".to_string()
        } else {
            cols.iter().map(|c| bracket(c)).collect::<Vec<_>>().join(", ")
        };
        let select_sql = format!("SELECT {} FROM {}", select_cols, target);
        let (fields, rows) = run_query(&mut client, &select_sql).await?;

        let names: Vec<String> = if cols.is_empty() { fields } else { cols };
        let header = format!(
            "INSERT INTO {} ({}) VALUES\n",
            target,
            names
                .iter()
                .map(|c| bracket(c))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let batch = if batch_size > 0 { batch_size as usize } else { 1000 };
        let byte_cap = 4 * 1024 * 1024usize;
        let mut pending: Vec<String> = Vec::new();
        let mut pending_bytes = 0usize;
        let mut count: u64 = 0;
        for obj in &rows {
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
        let mut client = self.client.lock().await;
        if !database.is_empty() {
            use_database(&mut client, database).await?;
        }
        let statements = split_sql_statements(sql);
        if statements.is_empty() {
            return Err("没有可执行的语句".to_string());
        }
        // 按条执行：脚本语句类型混杂（DDL / DML / USE），统一走文本协议并排空结果；
        // 文本协议下拿不到影响行数，只区分成功与失败。失败即停，剩余语句标记 skipped。
        let mut out: Vec<StatementResult> = Vec::with_capacity(statements.len());
        let mut stopped = false;
        for stmt in statements {
            if stopped {
                out.push(StatementResult::skipped(stmt));
                continue;
            }
            let started = std::time::Instant::now();
            let res = match client.simple_query(stmt.as_str()).await {
                Ok(stream) => stream
                    .into_results()
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            match res {
                Ok(()) => {
                    let ms = started.elapsed().as_millis() as u64;
                    out.push(StatementResult::success(stmt, 0, 0, ms));
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
        let mut client = self.client.lock().await;
        let lim = limit.clamp(1, 100);
        let target = qualified(database, table);
        let c = bracket(column);
        let mut sql = format!(
            "SELECT DISTINCT TOP ({}) {} FROM {} WHERE {} IS NOT NULL AND {} <> ''",
            lim + 1,
            c,
            target,
            c,
            c
        );
        if let Some(k) = keyword.filter(|k| !k.is_empty()) {
            sql.push_str(&format!(
                " AND {} LIKE {}",
                c,
                sql_literal(Some(&format!("%{}%", k)))
            ));
        }
        sql.push_str(&format!(" ORDER BY {}", c));

        let (_, rows) = run_query(&mut client, &sql).await?;
        let mut out: Vec<String> = Vec::new();
        for obj in &rows {
            if let Some(s) = obj.get(column).and_then(|v| v.as_str()) {
                out.push(s.chars().take(64).collect());
            }
        }
        let truncated = out.len() > lim;
        out.truncate(lim);
        Ok((out, truncated))
    }

    async fn truncate_table(&self, database: &str, table: &str) -> Result<(), String> {
        let mut client = self.client.lock().await;
        let target = if database.is_empty() {
            let (schema, name) = split_schema(table);
            format!("{}.{}", bracket(&schema), bracket(&name))
        } else {
            qualified(database, table)
        };
        let stream = client
            .simple_query(format!("TRUNCATE TABLE {}", target))
            .await
            .map_err(|e| e.to_string())?;
        stream.into_results().await.map_err(|e| e.to_string())?;
        Ok(())
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
        let mut client = self.client.lock().await;
        let target = qualified(database, table);
        let col_list = columns
            .iter()
            .map(|c| bracket(c))
            .collect::<Vec<_>>()
            .join(", ");
        let prefix = format!("INSERT INTO {} ({}) VALUES\n", target, col_list);

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
            let stream = client
                .simple_query(format!("{}{}", prefix, values))
                .await
                .map_err(|e| e.to_string())?;
            stream.into_results().await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// 一列的元数据（含类型渲染、默认值、注释等）。
struct SsColumn {
    name: String,
    type_sql: String,
    nullable: bool,
    identity: bool,
    computed: bool,
    default_def: Option<String>,
    comment: Option<String>,
    ordinal: i64,
}

/// 按 sys.types / sys.columns 的元数据还原出带长度的类型文本。
fn render_type(data_type: &str, max_length: i64, precision: i64, scale: i64) -> String {
    let dt = data_type.to_ascii_lowercase();
    match dt.as_str() {
        // nchar / nvarchar 的 max_length 单位为字节，需除以 2；-1 表示 max。
        "nvarchar" | "nchar" => {
            if max_length < 0 || max_length >= 8000 {
                format!("{}(max)", dt)
            } else {
                format!("{}({})", dt, max_length / 2)
            }
        }
        "varchar" | "char" | "varbinary" | "binary" => {
            if max_length < 0 {
                format!("{}(max)", dt)
            } else {
                format!("{}({})", dt, max_length)
            }
        }
        "decimal" | "numeric" => format!("{}({},{})", dt, precision, scale),
        "datetime2" | "time" | "datetimeoffset" => {
            if scale > 0 {
                format!("{}({})", dt, scale)
            } else {
                dt
            }
        }
        "float" => {
            if precision > 0 && precision < 53 {
                format!("float({})", precision)
            } else {
                dt
            }
        }
        _ => dt,
    }
}

/// 读取布尔位（run_query 已把 bit 渲染为 "1" / "0"）。
fn bit_of(row: &Value, key: &str) -> bool {
    matches!(
        row.get(key).and_then(|v| v.as_str()),
        Some("1") | Some("true") | Some("t")
    )
}

fn i64_of(row: &Value, key: &str) -> i64 {
    row.get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(0)
}

/// 读取列元数据（类型 / 可空 / 自增 / 生成列 / 默认值 / 注释）。
async fn fetch_columns(
    client: &mut SsClient,
    database: &str,
    table: &str,
) -> Result<Vec<SsColumn>, String> {
    let db = bracket(database);
    let sql = format!(
        "SELECT c.name AS column_name, ty.name AS data_type, \
                c.max_length AS max_length, c.precision AS precision, c.scale AS scale, \
                c.is_nullable AS is_nullable, c.is_identity AS is_identity, \
                c.is_computed AS is_computed, dc.definition AS default_def, \
                cc.definition AS computed_def, \
                CAST(ep.value AS nvarchar(max)) AS column_comment, c.column_id AS ordinal \
         FROM {db}.sys.columns c \
         JOIN {db}.sys.types ty ON ty.user_type_id = c.user_type_id \
         LEFT JOIN {db}.sys.default_constraints dc \
                ON dc.parent_object_id = c.object_id AND dc.parent_column_id = c.column_id \
         LEFT JOIN {db}.sys.computed_columns cc \
                ON cc.object_id = c.object_id AND cc.column_id = c.column_id \
         LEFT JOIN {db}.sys.extended_properties ep \
                ON ep.major_id = c.object_id AND ep.minor_id = c.column_id \
               AND ep.name = 'MS_Description' \
         WHERE c.object_id = OBJECT_ID(N'{obj}') \
         ORDER BY c.column_id",
        db = db,
        obj = qualified(database, table)
    );
    let (_, rows) = run_query(client, &sql).await?;
    Ok(rows
        .iter()
        .map(|r| {
            let data_type = str_of(r, "data_type");
            let computed = bit_of(r, "is_computed");
            let type_sql = render_type(
                &data_type,
                i64_of(r, "max_length"),
                i64_of(r, "precision"),
                i64_of(r, "scale"),
            );
            SsColumn {
                name: str_of(r, "column_name"),
                type_sql,
                nullable: bit_of(r, "is_nullable"),
                identity: bit_of(r, "is_identity"),
                computed,
                default_def: opt_str(r, "computed_def")
                    .or_else(|| opt_str(r, "default_def"))
                    .filter(|s| !s.is_empty()),
                comment: opt_str(r, "column_comment").filter(|s| !s.is_empty()),
                ordinal: i64_of(r, "ordinal"),
            }
        })
        .collect())
}

/// 主键列名（按 key_ordinal 排序）。
async fn fetch_primary_key(
    client: &mut SsClient,
    database: &str,
    table: &str,
) -> Result<Vec<String>, String> {
    let db = bracket(database);
    let sql = format!(
        "SELECT col.name AS column_name \
         FROM {db}.sys.indexes i \
         JOIN {db}.sys.index_columns ic \
              ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
         JOIN {db}.sys.columns col \
              ON col.object_id = ic.object_id AND col.column_id = ic.column_id \
         WHERE i.object_id = OBJECT_ID(N'{obj}') AND i.is_primary_key = 1 \
         ORDER BY ic.key_ordinal",
        db = db,
        obj = qualified(database, table)
    );
    let (_, rows) = run_query(client, &sql).await?;
    Ok(rows.iter().map(|r| str_of(r, "column_name")).collect())
}

/// 若是视图，返回 `CREATE VIEW` 定义（否则 None）。
async fn fetch_view_definition(
    client: &mut SsClient,
    database: &str,
    table: &str,
) -> Result<Option<String>, String> {
    let db = bracket(database);
    let sql = format!(
        "SELECT o.type AS obj_type, OBJECT_DEFINITION(o.object_id) AS obj_def \
         FROM {db}.sys.objects o WHERE o.object_id = OBJECT_ID(N'{obj}')",
        db = db,
        obj = qualified(database, table)
    );
    let (_, rows) = run_query(client, &sql).await?;
    let row = match rows.first() {
        Some(r) => r,
        None => return Ok(None),
    };
    if str_of(row, "obj_type") != "V" {
        return Ok(None);
    }
    let body = match opt_str(row, "obj_def") {
        Some(b) => b,
        None => return Ok(None),
    };
    Ok(Some(format!(
        "CREATE VIEW {} AS\n{};",
        qualified(database, table),
        strip_trailing_semicolons(&body)
    )))
}

/// 索引（按索引名归并多列，排除包含列与非聚集内部行）。
async fn fetch_indexes(
    client: &mut SsClient,
    database: &str,
    table: &str,
) -> Result<Vec<IndexDef>, String> {
    let db = bracket(database);
    let sql = format!(
        "SELECT i.name AS index_name, i.is_unique AS is_unique, \
                i.type_desc AS index_type, col.name AS column_name \
         FROM {db}.sys.indexes i \
         JOIN {db}.sys.index_columns ic \
              ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
         JOIN {db}.sys.columns col \
              ON col.object_id = ic.object_id AND col.column_id = ic.column_id \
         WHERE i.object_id = OBJECT_ID(N'{obj}') AND i.type > 0 \
               AND i.is_primary_key = 0 AND ic.is_included_column = 0 \
         ORDER BY i.name, ic.key_ordinal",
        db = db,
        obj = qualified(database, table)
    );
    let (_, rows) = run_query(client, &sql).await?;
    let mut indexes: Vec<IndexDef> = Vec::new();
    for row in &rows {
        let name = str_of(row, "index_name");
        let unique = bit_of(row, "is_unique");
        let index_type = str_of(row, "index_type");
        let column = str_of(row, "column_name");
        match indexes.iter_mut().find(|ix| ix.name == name) {
            Some(ix) => {
                if unique {
                    ix.unique = true;
                }
                if !column.is_empty() {
                    ix.columns.push(column);
                }
            }
            None => indexes.push(IndexDef {
                name,
                unique,
                index_type,
                columns: if column.is_empty() {
                    Vec::new()
                } else {
                    vec![column]
                },
            }),
        }
    }
    Ok(indexes)
}

/// 外键（按约束名归并多列）。
async fn fetch_foreign_keys(
    client: &mut SsClient,
    database: &str,
    table: &str,
) -> Result<Vec<ForeignKeyDef>, String> {
    let db = bracket(database);
    let sql = format!(
        "SELECT fk.name AS fk_name, pc.name AS column_name, rs.name AS ref_schema, \
                rt.name AS ref_table, rc.name AS ref_column, \
                fk.delete_referential_action_desc AS on_delete, \
                fk.update_referential_action_desc AS on_update \
         FROM {db}.sys.foreign_keys fk \
         JOIN {db}.sys.foreign_key_columns fkc \
              ON fkc.constraint_object_id = fk.object_id \
         JOIN {db}.sys.columns pc \
              ON pc.object_id = fkc.parent_object_id AND pc.column_id = fkc.parent_column_id \
         JOIN {db}.sys.tables rt ON rt.object_id = fk.referenced_object_id \
         JOIN {db}.sys.schemas rs ON rs.schema_id = rt.schema_id \
         JOIN {db}.sys.columns rc \
              ON rc.object_id = fkc.referenced_object_id AND rc.column_id = fkc.referenced_column_id \
         WHERE fk.parent_object_id = OBJECT_ID(N'{obj}') \
         ORDER BY fk.name, fkc.constraint_column_id",
        db = db,
        obj = qualified(database, table)
    );
    let (_, rows) = run_query(client, &sql).await?;
    let mut foreign_keys: Vec<ForeignKeyDef> = Vec::new();
    for row in &rows {
        let name = str_of(row, "fk_name");
        let column = str_of(row, "column_name");
        let ref_schema = str_of(row, "ref_schema");
        let ref_name = str_of(row, "ref_table");
        let ref_table = if ref_schema.is_empty() || ref_schema == "dbo" {
            ref_name
        } else {
            format!("{}.{}", ref_schema, ref_name)
        };
        let ref_column = str_of(row, "ref_column");
        let on_delete = str_of(row, "on_delete").replace('_', " ");
        let on_update = str_of(row, "on_update").replace('_', " ");
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
    Ok(foreign_keys)
}

/// 表注释（MS_Description 扩展属性）。
async fn fetch_table_comment(
    client: &mut SsClient,
    database: &str,
    table: &str,
) -> Result<Option<String>, String> {
    let db = bracket(database);
    let sql = format!(
        "SELECT CAST(ep.value AS nvarchar(max)) AS table_comment \
         FROM {db}.sys.objects o \
         LEFT JOIN {db}.sys.extended_properties ep \
                ON ep.major_id = o.object_id AND ep.minor_id = 0 \
               AND ep.name = 'MS_Description' \
         WHERE o.object_id = OBJECT_ID(N'{obj}')",
        db = db,
        obj = qualified(database, table)
    );
    let (_, rows) = run_query(client, &sql).await?;
    Ok(rows
        .first()
        .and_then(|r| opt_str(r, "table_comment"))
        .filter(|s| !s.is_empty()))
}

async fn fetch_table_structure(
    client: &mut SsClient,
    database: &str,
    table: &str,
) -> Result<TableStructure, String> {
    let cols = fetch_columns(client, database, table).await?;
    let pk = fetch_primary_key(client, database, table).await?;
    let indexes = fetch_indexes(client, database, table).await?;
    let foreign_keys = fetch_foreign_keys(client, database, table).await?;
    let comment = fetch_table_comment(client, database, table).await?;

    let columns = cols
        .into_iter()
        .map(|c| {
            let (default, generation_expression) = if c.computed {
                (None, c.default_def.clone())
            } else {
                (c.default_def.clone(), None)
            };
            let extra = if c.identity {
                "identity".to_string()
            } else if c.computed {
                "GENERATED".to_string()
            } else {
                String::new()
            };
            ColumnDef {
                column_key: if pk.contains(&c.name) {
                    Some("PRI".to_string())
                } else {
                    None
                },
                name: c.name,
                column_type: c.type_sql.clone(),
                data_type: c.type_sql,
                nullable: c.nullable,
                default,
                extra,
                comment: c.comment,
                charset: None,
                collation: None,
                generation_expression,
                ordinal: c.ordinal,
            }
        })
        .collect();

    Ok(TableStructure {
        columns,
        indexes,
        foreign_keys,
        options: TableOptionDef {
            engine: None,
            charset: None,
            collation: None,
            comment,
            auto_increment: None,
        },
    })
}

/// 由元数据拼出 `CREATE TABLE` 语句（含主键约束）。
async fn generate_table_ddl(
    client: &mut SsClient,
    database: &str,
    table: &str,
) -> Result<String, String> {
    let cols = fetch_columns(client, database, table).await?;
    let pk = fetch_primary_key(client, database, table).await?;

    let mut parts: Vec<String> = Vec::new();
    for c in &cols {
        let mut line = format!("  {} ", bracket(&c.name));
        if c.computed {
            line.push_str(&format!("AS {}", c.default_def.clone().unwrap_or_default()));
        } else {
            line.push_str(&c.type_sql);
            if c.identity {
                line.push_str(" IDENTITY(1,1)");
            }
            line.push_str(if c.nullable { " NULL" } else { " NOT NULL" });
            if let Some(d) = c.default_def.as_ref().filter(|d| !d.is_empty()) {
                line.push_str(&format!(" DEFAULT {}", d));
            }
        }
        parts.push(line);
    }
    if !pk.is_empty() {
        let (_, name) = split_schema(table);
        let pk_cols = pk.iter().map(|c| bracket(c)).collect::<Vec<_>>().join(", ");
        parts.push(format!(
            "  CONSTRAINT {} PRIMARY KEY ({})",
            bracket(&format!("PK_{}", name)),
            pk_cols
        ));
    }

    Ok(format!(
        "CREATE TABLE {} (\n{}\n);",
        qualified(database, table),
        parts.join(",\n")
    ))
}
