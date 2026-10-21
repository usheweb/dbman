//! MySQL 驱动：以现有能力为基准实现 [`RelationalDriver`]。
//!
//! 所有 MySQL 专有的东西都收敛在这里：`USE`、`SHOW DATABASES`/`SHOW CREATE TABLE`、
//! `information_schema` 元数据查询、反引号标识符、`TRUNCATE TABLE`、MySQL 字面量转义等。
use std::io::Write;
use std::sync::Arc;

use async_trait::async_trait;
use mysql_async::consts::ColumnType;
use mysql_async::prelude::Queryable;
use mysql_async::{Conn, Opts, OptsBuilder, Params, Row, Value};
use tokio::sync::Mutex;

use super::util::{
    build_paged_sql, csv_escape, export_cell, flush_insert, format_date, format_time,
    json_to_string, split_sql_statements, strip_trailing_semicolons,
};
use super::RelationalDriver;
use crate::models::{
    ColumnDef, ColumnInfo, DbConfig, ExportColumn, FieldInfo, ForeignKeyDef, IndexDef,
    QueryResponse, StatementResult, TableInfo, TableOptionDef, TableStructure,
};

pub struct MysqlDriver {
    conn: Arc<Mutex<Conn>>,
}

impl MysqlDriver {
    pub async fn connect(cfg: &DbConfig, host: &str, port: u16) -> Result<Self, String> {
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
        let conn = Conn::new(opts).await.map_err(|e| e.to_string())?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

#[async_trait]
impl RelationalDriver for MysqlDriver {
    fn kind(&self) -> &'static str {
        "mysql"
    }

    async fn databases(&self) -> Result<Vec<String>, String> {
        let mut conn = self.conn.lock().await;
        let mut result = conn
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

    async fn tables(&self, database: &str, filter: Option<&str>) -> Result<Vec<TableInfo>, String> {
        let mut conn = self.conn.lock().await;
        use_database(&mut conn, database).await?;
        fetch_tables(&mut conn, database, filter).await
    }

    async fn columns(&self, database: &str, table: &str) -> Result<Vec<ColumnInfo>, String> {
        let mut conn = self.conn.lock().await;
        fetch_columns(&mut conn, database, table).await
    }

    async fn query(
        &self,
        database: Option<&str>,
        sql: &str,
        limit: i64,
        offset: i64,
    ) -> Result<QueryResponse, String> {
        let mut conn = self.conn.lock().await;
        if let Some(db) = database {
            use_database(&mut conn, db).await?;
        }
        let (final_sql, paged) = build_paged_sql(sql, limit, offset);
        let mut result = conn
            .query_iter(final_sql)
            .await
            .map_err(|e| e.to_string())?;

        // 一次提交可能含多条语句（客户端默认开了 CLIENT_MULTI_STATEMENTS），服务器会返回多个结果集。
        // 所以不能再用 is_empty() 判断读写：OK 包（写语句的结果）本来就是无列的，
        // 而多条语句时 is_empty() 又是 false（因为还有后续结果集），
        // 于是「3 条 ALTER 一起执行」会被误判成查询，既拿不到影响行数、又会把夹在
        // 结果集序列里的错误静默丢掉。这里改为逐个结果集消费：
        //   有列的段 → 按行读取（第一段作为查询结果返回，其余读掉）
        //   无列的段 → OK 包，累加 affected_rows()
        // affected_rows() 取的是「最近读完的那个 OK 包」，所以必须在消费该段之前取。
        let mut first_rows: Option<(Vec<FieldInfo>, Vec<serde_json::Value>)> = None;
        let mut affected: u64 = 0;
        // columns() 返回 None 表示已经没有待处理结果集（columns_ref() 同样为空但无法区分
        // 「没有结果集」和「结果是 OK 包」，所以这里用 columns()）
        while let Some(cols) = result.columns() {
            if cols.is_empty() {
                affected += result.affected_rows();
                // 空列段调一次 next() 即消费完当前段并前进到下一段
                let _ = result.next().await.map_err(|e| e.to_string())?;
                continue;
            }
            let fields: Vec<FieldInfo> = cols
                .iter()
                .map(|c| FieldInfo {
                    name: c.name_str().into_owned(),
                })
                .collect();
            let mut rows = Vec::new();
            while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
                rows.push(row_to_json(&row));
            }
            if first_rows.is_none() {
                first_rows = Some((fields, rows));
            }
        }
        // 收尾：多结果集里夹带的错误会被驱动挂在 pending 上（此时 columns() 已是 None），
        // 必须再 next() 一次把它抛出来，否则会「报错了却显示执行成功」
        let _ = result.next().await.map_err(|e| e.to_string())?;

        if let Some((fields, rows)) = first_rows {
            return Ok(QueryResponse {
                write: false,
                rows,
                fields,
                affected: 0,
                paged,
            });
        }

        Ok(QueryResponse {
            write: true,
            rows: Vec::new(),
            fields: Vec::new(),
            affected,
            paged: false,
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
        let mut conn = self.conn.lock().await;
        if let Some(db) = database {
            use_database(&mut conn, db).await?;
        }
        let base_sql = strip_trailing_semicolons(sql);

        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
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
                let label = headers.get(i).cloned().unwrap_or_else(|| c.name.clone());
                csv_escape(&label)
            })
            .collect::<Vec<_>>()
            .join(",");
        writer
            .write_all(header_line.as_bytes())
            .map_err(|e| e.to_string())?;
        writer.write_all(b"\r\n").map_err(|e| e.to_string())?;

        let mut result = conn
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

    async fn table_ddl(&self, database: &str, table: &str) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        use_database(&mut conn, database).await?;
        fetch_table_ddl(&mut conn, database, table).await
    }

    async fn table_structure(
        &self,
        database: &str,
        table: &str,
    ) -> Result<TableStructure, String> {
        let mut conn = self.conn.lock().await;
        use_database(&mut conn, database).await?;
        fetch_table_structure(&mut conn, database, table).await
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
        let mut conn = self.conn.lock().await;
        use_database(&mut conn, database).await?;

        let structure = fetch_table_structure(&mut conn, database, table).await?;
        // 生成列不能显式插入
        let cols: Vec<String> = structure
            .columns
            .iter()
            .filter(|c| {
                !c.extra.to_ascii_lowercase().contains("generated")
                    && c.generation_expression.is_none()
            })
            .map(|c| c.name.clone())
            .collect();

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
            let ddl = fetch_table_ddl(&mut conn, database, table).await?;
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

        let batch = if batch_size > 0 {
            batch_size as usize
        } else {
            1000
        };
        let byte_cap = 4 * 1024 * 1024usize;

        let mut result = conn
            .query_iter(select_sql)
            .await
            .map_err(|e| e.to_string())?;
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
            "INSERT INTO {}.{} ({}) VALUES\n",
            quote_ident(database),
            quote_ident(table),
            names
                .iter()
                .map(|c| quote_ident(c))
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

    async fn exec_script(&self, database: &str, sql: &str) -> Result<Vec<StatementResult>, String> {
        let mut conn = self.conn.lock().await;
        // 未选择库时不能发 USE ``，否则整个脚本会在第一条就失败、一条语句都跑不到；
        // 这种情况交给脚本自己的 USE 决定目标库
        if !database.is_empty() {
            use_database(&mut conn, database).await?;
        }

        let statements = split_sql_statements(sql);
        if statements.is_empty() {
            return Err("没有可执行的语句".to_string());
        }
        // 按条执行并记录每条的结果：失败即停，剩余语句标记 skipped，
        // 让用户能清楚看到「哪条成功 / 哪条失败 / 哪条没跑」。
        let mut out: Vec<StatementResult> = Vec::with_capacity(statements.len());
        let mut stopped = false;
        for stmt in statements {
            if stopped {
                out.push(StatementResult::skipped(stmt));
                continue;
            }
            let started = std::time::Instant::now();
            match consume_statement(&mut conn, &stmt).await {
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
        let mut conn = self.conn.lock().await;
        fetch_distinct_values(&mut conn, database, table, column, keyword, limit).await
    }

    async fn truncate_table(&self, database: &str, table: &str) -> Result<(), String> {
        let mut conn = self.conn.lock().await;
        use_database(&mut conn, database).await?;
        conn.exec_drop(format!("TRUNCATE TABLE {}", quote_ident(table)), ())
            .await
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
        let mut conn = self.conn.lock().await;
        use_database(&mut conn, database).await?;

        let col_list = columns.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
        let placeholder = format!("({})", vec!["?"; columns.len()].join(", "));
        let values = vec![placeholder; rows.len()].join(", ");
        let sql = format!(
            "INSERT INTO {} ({}) VALUES {}",
            quote_ident(table),
            col_list,
            values
        );

        let mut params = Vec::with_capacity(rows.len() * columns.len());
        for row in rows {
            for cell in row {
                params.push(match cell {
                    None => Value::NULL,
                    Some(s) => Value::Bytes(s.clone().into_bytes()),
                });
            }
        }
        conn.exec_drop(sql, Params::Positional(params))
            .await
            .map_err(|e| e.to_string())
    }
}

/// 反引号包裹标识符（内部反引号加倍）。
fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// 切换到指定库（`USE`）。
async fn use_database(conn: &mut Conn, database: &str) -> Result<(), String> {
    let escaped = database.replace('`', "``");
    conn.query_drop(format!("USE `{}`", escaped))
        .await
        .map_err(|e| e.to_string())
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

/// 执行单条语句并消费它可能产生的多个结果集，返回 (返回行数, 影响行数)。
/// 与 `query()` 的消费写法一致：OK 包（无列段）累加 `affected_rows()`，
/// 有列段按行读取计数；收尾再 `next()` 一次把多结果集里夹带的错误抛出来。
async fn consume_statement(conn: &mut Conn, sql: &str) -> Result<(u64, u64), String> {
    let mut result = conn.query_iter(sql).await.map_err(|e| e.to_string())?;
    let mut rows_total: u64 = 0;
    let mut affected: u64 = 0;
    while let Some(cols) = result.columns() {
        if cols.is_empty() {
            affected += result.affected_rows();
            let _ = result.next().await.map_err(|e| e.to_string())?;
            continue;
        }
        while let Some(_row) = result.next().await.map_err(|e| e.to_string())? {
            rows_total += 1;
        }
    }
    let _ = result.next().await.map_err(|e| e.to_string())?;
    Ok((rows_total, affected))
}

/// 查询某库的表（名 + 注释 + 排序维度元数据）。filter 非空时按表名/注释模糊匹配。
/// 全部字段来自 information_schema.TABLES 的**一条** SQL（MySQL 5.6 及更早版本同样具备这些列），
/// 行数/大小直接取服务端统计值，不再逐表补 SELECT COUNT(*)——几百张表的库也只发一次查询。
async fn fetch_tables(
    conn: &mut Conn,
    database: &str,
    filter: Option<&str>,
) -> Result<Vec<TableInfo>, String> {
    let mut sql = String::from(
        "SELECT TABLE_NAME as tableName, TABLE_COMMENT as tableComment, \
         CREATE_TIME, UPDATE_TIME, TABLE_ROWS, DATA_LENGTH, INDEX_LENGTH, \
         ENGINE, SUBSTRING_INDEX(TABLE_COLLATION, '_', 1) as tableCharset, \
         TABLE_TYPE, AUTO_INCREMENT \
         FROM information_schema.TABLES WHERE TABLE_SCHEMA = ?",
    );
    let mut params: Vec<Value> = vec![Value::Bytes(database.as_bytes().to_vec())];
    if let Some(f) = filter.filter(|f| !f.is_empty()) {
        sql.push_str(" AND (TABLE_NAME LIKE ? OR TABLE_COMMENT LIKE ?)");
        let like = format!("%{}%", f);
        params.push(Value::Bytes(like.clone().into_bytes()));
        params.push(Value::Bytes(like.into_bytes()));
    }
    // 固定按表名排序，前端本地再按用户选的维度排，多次刷新顺序一致、列表不闪动
    sql.push_str(" ORDER BY TABLE_NAME");

    let mut result = conn
        .exec_iter(sql, Params::Positional(params))
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        // information_schema.TABLES 的尺寸单位是字节
        let data_length = opt_u64(&row, 5);
        let index_length = opt_u64(&row, 6);
        let total_length = match (data_length, index_length) {
            (None, None) => None,
            (d, i) => Some(d.unwrap_or(0) + i.unwrap_or(0)),
        };
        out.push(TableInfo {
            table_name: opt_string(&row, 0).unwrap_or_default(),
            table_comment: opt_string(&row, 1),
            create_time: opt_string(&row, 2),
            update_time: opt_string(&row, 3),
            table_rows: opt_u64(&row, 4),
            data_length,
            index_length,
            total_length,
            engine: opt_string(&row, 7),
            charset: opt_string(&row, 8),
            table_type: opt_string(&row, 9),
            auto_increment: opt_u64(&row, 10),
        });
    }
    result.drop_result().await.map_err(|e| e.to_string())?;

    // 行数直接用 information_schema.TABLES 的 TABLE_ROWS（InnoDB 是采样估算值，只看量级够用）。
    // 以前会对小表逐表补 SELECT COUNT(*)，几百张小表串行跑一轮就要好几秒，已整体去掉；
    // 前端展示时统一写成「约 N 行」表明是估算值。
    Ok(out)
}

/// 查询某库某表的列（名 + 注释 + 类型）。
async fn fetch_columns(
    conn: &mut Conn,
    database: &str,
    table: &str,
) -> Result<Vec<ColumnInfo>, String> {
    let sql = "SELECT COLUMN_NAME as columnName, COLUMN_COMMENT as columnComment, DATA_TYPE as dataType \
               FROM information_schema.COLUMNS \
               WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION";
    let params = Params::Positional(vec![
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

/// 查询某库某表某个字段的去重取值，供大模型确认「需求里的词」对应的真实数据值。
/// 返回 (取值列表, 是否被截断)。
async fn fetch_distinct_values(
    conn: &mut Conn,
    database: &str,
    table: &str,
    column: &str,
    keyword: Option<&str>,
    limit: usize,
) -> Result<(Vec<String>, bool), String> {
    use_database(conn, database).await?;
    // 标识符沿用项目惯例：反引号加倍后再包裹
    let t = table.replace('`', "``");
    let c = column.replace('`', "``");
    let lim = limit.clamp(1, 100);
    let mut sql = format!("SELECT DISTINCT `{c}` FROM `{t}` WHERE `{c}` IS NOT NULL AND `{c}` <> ''");
    let mut params: Vec<Value> = Vec::new();
    if let Some(k) = keyword.filter(|k| !k.is_empty()) {
        sql.push_str(&format!(" AND `{c}` LIKE ?"));
        params.push(Value::Bytes(format!("%{}%", k).into_bytes()));
    }
    // LIMIT 用受控整数内联（已 clamp，无注入风险），避免驱动对 LIMIT ? 的兼容差异；
    // 多取 1 条用于判断是否被截断。
    sql.push_str(&format!(" ORDER BY `{c}` LIMIT {}", lim + 1));

    let mut result = conn
        .exec_iter(sql, Params::Positional(params))
        .await
        .map_err(|e| e.to_string())?;
    let mut out: Vec<String> = Vec::new();
    while let Some(row) = result.next().await.map_err(|e| e.to_string())? {
        if let Some(v) = row.as_ref(0) {
            // 单值截断到 64 字符，避免超长文本灌进模型上下文
            let s = json_to_string(&value_to_json(v, false));
            out.push(s.chars().take(64).collect());
        }
    }
    result.drop_result().await.map_err(|e| e.to_string())?;
    let truncated = out.len() > lim;
    out.truncate(lim);
    Ok((out, truncated))
}

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

/// 取一列并转成 Option<u64>（NULL / 负数 / 非数字 → None）；用于 information_schema 的尺寸与行数。
fn opt_u64(row: &Row, idx: usize) -> Option<u64> {
    match row.as_ref(idx) {
        Some(Value::UInt(v)) => Some(*v),
        Some(Value::Int(v)) => u64::try_from(*v).ok(),
        Some(Value::Bytes(b)) => String::from_utf8_lossy(b).trim().parse().ok(),
        _ => None,
    }
}

/// SHOW CREATE TABLE，返回第 2 列（建表语句）；视图返回 CREATE VIEW 语句。
async fn fetch_table_ddl(conn: &mut Conn, database: &str, table: &str) -> Result<String, String> {
    let sql = format!(
        "SHOW CREATE TABLE {}.{}",
        quote_ident(database),
        quote_ident(table)
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
async fn fetch_table_structure(
    conn: &mut Conn,
    database: &str,
    table: &str,
) -> Result<TableStructure, String> {
    let params = || {
        Params::Positional(vec![
            Value::Bytes(database.as_bytes().to_vec()),
            Value::Bytes(table.as_bytes().to_vec()),
        ])
    };

    // 字段（用 COLUMN_TYPE 才能带上 unsigned / 长度 / enum 值）
    const COL_SQL: &str =
        "SELECT COLUMN_NAME, COLUMN_TYPE, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT, \
                   COLUMN_KEY, EXTRA, COLUMN_COMMENT, CHARACTER_SET_NAME, COLLATION_NAME, \
                   ORDINAL_POSITION, GENERATION_EXPRESSION \
                   FROM information_schema.COLUMNS \
                   WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION";
    // MySQL 5.6 及更早没有 GENERATION_EXPRESSION 列，老版本也不存在生成列，故直接省略该列
    const COL_SQL_LEGACY: &str =
        "SELECT COLUMN_NAME, COLUMN_TYPE, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT, \
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
    let mut result = conn
        .exec_iter(idx_sql, params())
        .await
        .map_err(|e| e.to_string())?;
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
    let mut result = conn
        .exec_iter(fk_sql, params())
        .await
        .map_err(|e| e.to_string())?;
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
    let mut result = conn
        .exec_iter(opt_sql, params())
        .await
        .map_err(|e| e.to_string())?;
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
