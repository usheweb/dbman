//! Oracle 驱动：基于 `oracle`（ODPI-C / OCI 绑定）实现 [`RelationalDriver`]。
//!
//! 设计取舍：
//! - `oracle::Connection` 虽为 `Send + Sync`，但其 API 全部是同步阻塞的，因此用
//!   `std::sync::Mutex` 包裹连接，并通过 `tokio::task::spawn_blocking` 把阻塞调用
//!   移出异步运行时（与 sqlite 驱动一致）。
//! - Oracle 无「库」概念，UI 的「库」映射为 schema（`ALTER SESSION SET CURRENT_SCHEMA`），
//!   `databases()` 返回 `all_users` 中的用户名（即 schema）。
//! - 分页使用 Oracle 12c+ 的 `OFFSET ... FETCH NEXT ... ROWS ONLY`；建表语句取自
//!   `DBMS_METADATA.GET_DDL`。
//! - 运行时依赖 Oracle Instant Client（OCI 动态库），非纯 Rust 实现。
use std::io::Write;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use oracle::{Connection, Row};

use super::util::{
    csv_escape, export_cell, flush_insert, split_sql_statements, starts_with_select_or_with,
    strip_trailing_semicolons,
};
use super::RelationalDriver;
use crate::models::{
    ColumnDef, ColumnInfo, DbConfig, ExportColumn, FieldInfo, ForeignKeyDef, IndexDef,
    QueryResponse, TableInfo, TableOptionDef, TableStructure,
};

pub struct OracleDriver {
    conn: Arc<Mutex<Connection>>,
}

impl OracleDriver {
    pub fn connect(cfg: &DbConfig, host: &str, port: u16) -> Result<Self, String> {
        let user = cfg.user.as_deref().unwrap_or("");
        let password = cfg.password.as_deref().unwrap_or("");
        // Easy Connect 串：host:port/service_name；未填 service 时退回 host:port。
        let service = cfg
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let connect_string = match service {
            Some(svc) => format!("{}:{}/{}", host, port, svc),
            None => format!("{}:{}", host, port),
        };
        let conn = Connection::connect(user, password, &connect_string)
            .map_err(|e| format!("Oracle 连接失败：{}", e))?;
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
            let guard = conn.lock().map_err(|_| "Oracle 连接锁已中毒".to_string())?;
            f(&guard)
        })
        .await
        .map_err(|e| e.to_string())?
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

/// 结果行 → 按列名索引的 JSON 对象（NULL → Value::Null，其余按文本）。
fn row_to_obj(row: &Row) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (info, value) in row.column_info().iter().zip(row.sql_values().iter()) {
        let val = if value.is_null().unwrap_or(false) {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(value.to_string())
        };
        map.insert(info.name().to_string(), val);
    }
    serde_json::Value::Object(map)
}

/// 执行查询并收集结果为按列名索引的 JSON 行。
fn query_objs(conn: &Connection, sql: &str) -> Result<Vec<serde_json::Value>, String> {
    let rows = conn.query(sql, &[]).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        let row = r.map_err(|e| e.to_string())?;
        out.push(row_to_obj(&row));
    }
    Ok(out)
}

/// 执行查询，同时返回列名与行（列名在结果为空时仍可用）。
fn query_fields_rows(
    conn: &Connection,
    sql: &str,
) -> Result<(Vec<String>, Vec<serde_json::Value>), String> {
    let mut rows = conn.query(sql, &[]).map_err(|e| e.to_string())?;
    let fields: Vec<String> = rows
        .column_info()
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    let mut out = Vec::new();
    while let Some(r) = rows.next() {
        let row = r.map_err(|e| e.to_string())?;
        out.push(row_to_obj(&row));
    }
    Ok((fields, out))
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

/// 设置当前 schema（UI 的「库」语义）。
fn set_schema(conn: &Connection, owner: &str) -> Result<(), String> {
    if owner.is_empty() {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER SESSION SET CURRENT_SCHEMA = {}", quote_ident(owner)),
        &[],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Oracle 12c+ 分页：`FETCH FIRST n ROWS ONLY` / `OFFSET n ROWS FETCH NEXT m ROWS ONLY`。
fn build_paged_sql_oracle(sql: &str, limit: i64, offset: i64) -> (String, bool) {
    let base = strip_trailing_semicolons(sql);
    let lim = if limit > 0 { limit } else { 0 };
    let off = if offset > 0 { offset } else { 0 };
    if lim == 0 || !starts_with_select_or_with(&base) {
        return (base, false);
    }
    let lower = base.to_ascii_lowercase();
    if lower.contains(" fetch ") || lower.contains(" rownum") || lower.contains(" offset ") {
        return (base, false);
    }
    let final_sql = if off > 0 {
        format!("{} OFFSET {} ROWS FETCH NEXT {} ROWS ONLY", base, off, lim)
    } else {
        format!("{} FETCH FIRST {} ROWS ONLY", base, lim)
    };
    (final_sql, true)
}

/// 该对象是否为视图（用于 DDL / 跳过分页判定）。
fn is_view_meta(conn: &Connection, owner: &str, table: &str) -> bool {
    let sql = format!(
        "SELECT 1 AS one FROM all_views WHERE owner = {} AND view_name = {}",
        sql_literal(Some(owner)),
        sql_literal(Some(table))
    );
    query_objs(conn, &sql).map(|r| !r.is_empty()).unwrap_or(false)
}

/// 取建表/建视图语句（`DBMS_METADATA.GET_DDL`）。
fn fetch_ddl(conn: &Connection, owner: &str, table: &str) -> Result<String, String> {
    let object_type = if is_view_meta(conn, owner, table) {
        "VIEW"
    } else {
        "TABLE"
    };
    let sql = format!(
        "SELECT DBMS_METADATA.GET_DDL({}, {}, {}) AS ddl FROM DUAL",
        sql_literal(Some(object_type)),
        sql_literal(Some(table)),
        sql_literal(Some(owner))
    );
    let rows = query_objs(conn, &sql)?;
    let ddl = rows
        .first()
        .and_then(|r| opt_col(r, "DDL"))
        .unwrap_or_default();
    if ddl.trim().is_empty() {
        return Err(format!("未能获取 {}.{} 的建表语句", owner, table));
    }
    Ok(ddl)
}

/// 主键列（按 position 排序）。
fn fetch_pk_cols(conn: &Connection, owner: &str, table: &str) -> Vec<String> {
    let sql = format!(
        "SELECT cc.column_name AS column_name \
         FROM all_constraints c \
         JOIN all_cons_columns cc \
           ON c.owner = cc.owner AND c.constraint_name = cc.constraint_name \
         WHERE c.owner = {} AND c.table_name = {} AND c.constraint_type = 'P' \
         ORDER BY cc.position",
        sql_literal(Some(owner)),
        sql_literal(Some(table))
    );
    query_objs(conn, &sql)
        .unwrap_or_default()
        .iter()
        .map(|r| req_col(r, "COLUMN_NAME"))
        .collect()
}

/// 由数据字典拼出完整列类型字符串。
fn oracle_column_type(
    data_type: &str,
    data_length: Option<u64>,
    char_length: Option<u64>,
    precision: Option<u64>,
    scale: Option<i64>,
) -> String {
    let dt = data_type.to_ascii_uppercase();
    match dt.as_str() {
        "NUMBER" | "DECIMAL" | "NUMERIC" => match (precision, scale) {
            (Some(p), Some(s)) => format!("NUMBER({},{})", p, s),
            (Some(p), None) => format!("NUMBER({})", p),
            _ => "NUMBER".to_string(),
        },
        "VARCHAR2" | "NVARCHAR2" | "CHAR" | "NCHAR" | "RAW" => {
            let len = char_length.filter(|n| *n > 0).or(data_length);
            match len {
                Some(n) => format!("{}({})", dt, n),
                None => dt,
            }
        }
        _ => dt,
    }
}

/// 读取列定义（含类型、可空、默认值、注释）。
fn fetch_column_defs(
    conn: &Connection,
    owner: &str,
    table: &str,
) -> Result<Vec<ColumnDef>, String> {
    let sql = format!(
        "SELECT c.column_name AS column_name, c.data_type AS data_type, \
                c.data_length AS data_length, c.char_length AS char_length, \
                c.data_precision AS data_precision, c.data_scale AS data_scale, \
                c.nullable AS nullable, c.data_default AS data_default, \
                c.column_id AS column_id, cc.comments AS comments \
         FROM all_tab_columns c \
         LEFT JOIN all_col_comments cc \
           ON cc.owner = c.owner AND cc.table_name = c.table_name AND cc.column_name = c.column_name \
         WHERE c.owner = {} AND c.table_name = {} \
         ORDER BY c.column_id",
        sql_literal(Some(owner)),
        sql_literal(Some(table))
    );
    let pk_cols = fetch_pk_cols(conn, owner, table);
    let mut out = Vec::new();
    for r in query_objs(conn, &sql)? {
        let name = req_col(&r, "COLUMN_NAME");
        let data_type = req_col(&r, "DATA_TYPE");
        let nullable = req_col(&r, "NULLABLE").eq_ignore_ascii_case("Y");
        let default = opt_col(&r, "DATA_DEFAULT");
        let comment = opt_col(&r, "COMMENTS");
        let ordinal = num_col(&r, "COLUMN_ID").map(|n| n as i64).unwrap_or(0);
        let column_type = oracle_column_type(
            &data_type,
            num_col(&r, "DATA_LENGTH"),
            num_col(&r, "CHAR_LENGTH"),
            num_col(&r, "DATA_PRECISION"),
            opt_col(&r, "DATA_SCALE").and_then(|s| s.trim().parse::<i64>().ok()),
        );
        let is_pk = pk_cols.iter().any(|c| c.eq_ignore_ascii_case(&name));
        out.push(ColumnDef {
            name,
            column_type,
            data_type,
            nullable,
            default: default.map(|s| s.trim().to_string()),
            column_key: if is_pk { Some("PRI".to_string()) } else { None },
            extra: String::new(),
            comment,
            charset: None,
            collation: None,
            generation_expression: None,
            ordinal,
        });
    }
    Ok(out)
}

/// 读取索引定义（按索引名聚合列）。
fn fetch_indexes(conn: &Connection, owner: &str, table: &str) -> Vec<IndexDef> {
    let sql = format!(
        "SELECT i.index_name AS index_name, i.uniqueness AS uniqueness, \
                i.index_type AS index_type, ic.column_name AS column_name \
         FROM all_indexes i \
         JOIN all_ind_columns ic \
           ON ic.index_owner = i.owner AND ic.index_name = i.index_name \
         WHERE i.table_owner = {} AND i.table_name = {} \
         ORDER BY i.index_name, ic.column_position",
        sql_literal(Some(owner)),
        sql_literal(Some(table))
    );
    let rows = query_objs(conn, &sql).unwrap_or_default();
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::HashMap<String, IndexDef> = std::collections::HashMap::new();
    for r in rows {
        let name = req_col(&r, "INDEX_NAME");
        let entry = map.entry(name.clone()).or_insert_with(|| {
            order.push(name.clone());
            IndexDef {
                name: name.clone(),
                unique: req_col(&r, "UNIQUENESS").eq_ignore_ascii_case("UNIQUE"),
                index_type: req_col(&r, "INDEX_TYPE"),
                columns: Vec::new(),
            }
        });
        entry.columns.push(req_col(&r, "COLUMN_NAME"));
    }
    order
        .into_iter()
        .filter_map(|n| map.remove(&n))
        .collect()
}

/// 读取外键定义（按约束名聚合列）。
fn fetch_foreign_keys(conn: &Connection, owner: &str, table: &str) -> Vec<ForeignKeyDef> {
    let sql = format!(
        "SELECT c.constraint_name AS constraint_name, c.delete_rule AS delete_rule, \
                cc.column_name AS column_name, rc.table_name AS ref_table, \
                rcc.column_name AS ref_column \
         FROM all_constraints c \
         JOIN all_cons_columns cc \
           ON c.owner = cc.owner AND c.constraint_name = cc.constraint_name \
         JOIN all_constraints rc \
           ON c.r_owner = rc.owner AND c.r_constraint_name = rc.constraint_name \
         JOIN all_cons_columns rcc \
           ON rc.owner = rcc.owner AND rc.constraint_name = rcc.constraint_name \
          AND cc.position = rcc.position \
         WHERE c.owner = {} AND c.table_name = {} AND c.constraint_type = 'R' \
         ORDER BY c.constraint_name, cc.position",
        sql_literal(Some(owner)),
        sql_literal(Some(table))
    );
    let rows = query_objs(conn, &sql).unwrap_or_default();
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::HashMap<String, ForeignKeyDef> = std::collections::HashMap::new();
    for r in rows {
        let name = req_col(&r, "CONSTRAINT_NAME");
        let entry = map.entry(name.clone()).or_insert_with(|| {
            order.push(name.clone());
            ForeignKeyDef {
                name: name.clone(),
                columns: Vec::new(),
                ref_table: req_col(&r, "REF_TABLE"),
                ref_columns: Vec::new(),
                on_delete: req_col(&r, "DELETE_RULE"),
                on_update: String::new(),
            }
        });
        entry.columns.push(req_col(&r, "COLUMN_NAME"));
        entry.ref_columns.push(req_col(&r, "REF_COLUMN"));
    }
    order
        .into_iter()
        .filter_map(|n| map.remove(&n))
        .collect()
}

/// 表注释。
fn fetch_table_comment(conn: &Connection, owner: &str, table: &str) -> Option<String> {
    let sql = format!(
        "SELECT comments AS comments FROM all_tab_comments \
         WHERE owner = {} AND table_name = {}",
        sql_literal(Some(owner)),
        sql_literal(Some(table))
    );
    query_objs(conn, &sql)
        .ok()
        .and_then(|r| r.into_iter().next())
        .and_then(|r| opt_col(&r, "COMMENTS"))
}

/// 组装完整表结构。
fn fetch_table_structure(
    conn: &Connection,
    owner: &str,
    table: &str,
) -> Result<TableStructure, String> {
    let columns = fetch_column_defs(conn, owner, table)?;
    let indexes = fetch_indexes(conn, owner, table);
    let foreign_keys = fetch_foreign_keys(conn, owner, table);
    let options = TableOptionDef {
        engine: None,
        charset: None,
        collation: None,
        comment: fetch_table_comment(conn, owner, table),
        auto_increment: None,
    };
    Ok(TableStructure {
        columns,
        indexes,
        foreign_keys,
        options,
    })
}

/// 把 UI 的「库」解析为 Oracle schema；为空时取当前会话 schema。
fn resolve_owner(conn: &Connection, database: &str) -> Result<String, String> {
    let owner = database.trim();
    if !owner.is_empty() {
        return Ok(owner.to_string());
    }
    let rows = query_objs(
        conn,
        "SELECT sys_context('USERENV','CURRENT_SCHEMA') AS current_schema FROM dual",
    )?;
    Ok(rows
        .first()
        .and_then(|r| opt_col(r, "CURRENT_SCHEMA"))
        .unwrap_or_default()
        .trim()
        .to_string())
}

#[async_trait]
impl RelationalDriver for OracleDriver {
    fn kind(&self) -> &'static str {
        "oracle"
    }

    async fn databases(&self) -> Result<Vec<String>, String> {
        self.with_conn(|conn| {
            let sql = "SELECT username AS username FROM all_users ORDER BY username";
            Ok(query_objs(conn, sql)?
                .iter()
                .map(|r| req_col(r, "USERNAME"))
                .collect())
        })
        .await
    }

    async fn tables(&self, database: &str, filter: Option<&str>) -> Result<Vec<TableInfo>, String> {
        let database = database.to_string();
        let filter = filter.map(|s| s.to_string());
        self.with_conn(move |conn| {
            let owner = resolve_owner(conn, &database)?;
            let owner_lit = sql_literal(Some(&owner));
            let inner = format!(
                "SELECT t.table_name AS table_name, c.comments AS comments, \
                        t.num_rows AS num_rows, 'BASE TABLE' AS table_type \
                 FROM all_tables t \
                 LEFT JOIN all_tab_comments c \
                   ON c.owner = t.owner AND c.table_name = t.table_name \
                 WHERE t.owner = {owner_lit} \
                 UNION ALL \
                 SELECT v.view_name AS table_name, c.comments AS comments, \
                        NULL AS num_rows, 'VIEW' AS table_type \
                 FROM all_views v \
                 LEFT JOIN all_tab_comments c \
                   ON c.owner = v.owner AND c.table_name = v.view_name \
                 WHERE v.owner = {owner_lit}"
            );
            let mut sql = format!("SELECT * FROM ({}) WHERE 1 = 1", inner);
            if let Some(f) = filter.filter(|f| !f.is_empty()) {
                let like = sql_literal(Some(&format!("%{}%", f.to_uppercase())));
                sql.push_str(&format!(
                    " AND (UPPER(table_name) LIKE {like} OR UPPER(comments) LIKE {like})"
                ));
            }
            sql.push_str(" ORDER BY table_name");
            let mut out = Vec::new();
            for r in query_objs(conn, &sql)? {
                out.push(TableInfo {
                    table_name: req_col(&r, "TABLE_NAME"),
                    table_comment: opt_col(&r, "COMMENTS"),
                    create_time: None,
                    update_time: None,
                    table_rows: num_col(&r, "NUM_ROWS"),
                    data_length: None,
                    index_length: None,
                    total_length: None,
                    engine: None,
                    charset: None,
                    table_type: opt_col(&r, "TABLE_TYPE"),
                    auto_increment: None,
                });
            }
            Ok(out)
        })
        .await
    }

    async fn columns(&self, database: &str, table: &str) -> Result<Vec<ColumnInfo>, String> {
        let database = database.to_string();
        let table = table.to_string();
        self.with_conn(move |conn| {
            let owner = resolve_owner(conn, &database)?;
            let sql = format!(
                "SELECT c.column_name AS column_name, cc.comments AS comments, \
                        c.data_type AS data_type \
                 FROM all_tab_columns c \
                 LEFT JOIN all_col_comments cc \
                   ON cc.owner = c.owner AND cc.table_name = c.table_name \
                  AND cc.column_name = c.column_name \
                 WHERE c.owner = {} AND c.table_name = {} \
                 ORDER BY c.column_id",
                sql_literal(Some(&owner)),
                sql_literal(Some(&table))
            );
            Ok(query_objs(conn, &sql)?
                .iter()
                .map(|r| ColumnInfo {
                    column_name: req_col(r, "COLUMN_NAME"),
                    column_comment: opt_col(r, "COMMENTS"),
                    data_type: opt_col(r, "DATA_TYPE"),
                })
                .collect())
        })
        .await
    }

    async fn query(
        &self,
        database: Option<&str>,
        sql: &str,
        limit: i64,
        offset: i64,
    ) -> Result<QueryResponse, String> {
        let database = database.map(|s| s.to_string());
        let sql = sql.to_string();
        self.with_conn(move |conn| {
            if let Some(db) = database.as_deref() {
                set_schema(conn, db)?;
            }
            let base = strip_trailing_semicolons(&sql);
            if starts_with_select_or_with(&base) {
                let (final_sql, paged) = build_paged_sql_oracle(&base, limit, offset);
                let (names, rows) = query_fields_rows(conn, &final_sql)?;
                let fields = names.into_iter().map(|name| FieldInfo { name }).collect();
                Ok(QueryResponse {
                    write: false,
                    rows,
                    fields,
                    affected: 0,
                    paged,
                })
            } else {
                let stmt = conn.execute(&base, &[]).map_err(|e| e.to_string())?;
                let affected = stmt.row_count().unwrap_or(0);
                conn.commit().map_err(|e| e.to_string())?;
                Ok(QueryResponse {
                    write: true,
                    rows: Vec::new(),
                    fields: Vec::new(),
                    affected,
                    paged: false,
                })
            }
        })
        .await
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
        let database = database.map(|s| s.to_string());
        let base = strip_trailing_semicolons(sql);
        let path = path.to_string();
        let headers = headers.to_vec();
        let columns = columns.to_vec();
        self.with_conn(move |conn| {
            if let Some(db) = database.as_deref() {
                set_schema(conn, db)?;
            }
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

            let rows = conn.query(&base, &[]).map_err(|e| e.to_string())?;
            let empty = serde_json::Value::Null;
            let mut count: u64 = 0;
            for r in rows {
                let row = r.map_err(|e| e.to_string())?;
                let obj = row_to_obj(&row);
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

    async fn table_ddl(&self, database: &str, table: &str) -> Result<String, String> {
        let database = database.to_string();
        let table = table.to_string();
        self.with_conn(move |conn| {
            let owner = resolve_owner(conn, &database)?;
            fetch_ddl(conn, &owner, &table)
        })
        .await
    }

    async fn table_structure(
        &self,
        database: &str,
        table: &str,
    ) -> Result<TableStructure, String> {
        let database = database.to_string();
        let table = table.to_string();
        self.with_conn(move |conn| {
            let owner = resolve_owner(conn, &database)?;
            fetch_table_structure(conn, &owner, &table)
        })
        .await
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
        let database = database.to_string();
        let table = table.to_string();
        let path = path.to_string();
        self.with_conn(move |conn| {
            let owner = resolve_owner(conn, &database)?;
            let qualified = format!("{}.{}", quote_ident(&owner), quote_ident(&table));
            let columns = fetch_column_defs(conn, &owner, &table)?;
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
                    .write_all(format!("DROP TABLE {};\n", qualified).as_bytes())
                    .map_err(|e| e.to_string())?;
            }
            if with_create {
                let ddl = fetch_ddl(conn, &owner, &table)?;
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
            let select_sql = format!("SELECT {} FROM {}", select_cols, qualified);

            let mut rows = conn.query(&select_sql, &[]).map_err(|e| e.to_string())?;
            let names: Vec<String> = if cols.is_empty() {
                rows.column_info().iter().map(|c| c.name().to_string()).collect()
            } else {
                cols.clone()
            };
            let header = format!(
                "INSERT INTO {} ({}) VALUES\n",
                qualified,
                names.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ")
            );

            let batch = if batch_size > 0 { batch_size as usize } else { 1000 };
            let byte_cap = 4 * 1024 * 1024usize;
            let mut pending: Vec<String> = Vec::new();
            let mut pending_bytes = 0usize;
            let mut count: u64 = 0;

            while let Some(r) = rows.next() {
                let row = r.map_err(|e| e.to_string())?;
                let obj = row_to_obj(&row);
                let tuple = names
                    .iter()
                    .map(|n| json_literal(obj.get(n).unwrap_or(&serde_json::Value::Null)))
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

    async fn exec_script(&self, database: &str, sql: &str) -> Result<u64, String> {
        let database = database.to_string();
        let sql = sql.to_string();
        self.with_conn(move |conn| {
            let owner = resolve_owner(conn, &database)?;
            set_schema(conn, &owner)?;
            let statements = split_sql_statements(&sql);
            if statements.is_empty() {
                return Err("没有可执行的语句".to_string());
            }
            let mut count = 0u64;
            for (idx, stmt) in statements.iter().enumerate() {
                conn.execute(stmt, &[])
                    .map_err(|e| format!("第 {} 条语句执行失败：{}", idx + 1, e))?;
                count += 1;
            }
            conn.commit().map_err(|e| e.to_string())?;
            Ok(count)
        })
        .await
    }

    async fn distinct_values(
        &self,
        database: &str,
        table: &str,
        column: &str,
        keyword: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<String>, bool), String> {
        let database = database.to_string();
        let table = table.to_string();
        let column = column.to_string();
        let keyword = keyword.map(|s| s.to_string());
        self.with_conn(move |conn| {
            let owner = resolve_owner(conn, &database)?;
            let lim = limit.clamp(1, 100);
            let qualified = format!("{}.{}", quote_ident(&owner), quote_ident(&table));
            let c = quote_ident(&column);
            let mut sql = format!(
                "SELECT DISTINCT {c} AS v FROM {qualified} \
                 WHERE {c} IS NOT NULL AND TO_CHAR({c}) <> ''"
            );
            if let Some(k) = keyword.filter(|k| !k.is_empty()) {
                sql.push_str(&format!(
                    " AND TO_CHAR({c}) LIKE {}",
                    sql_literal(Some(&format!("%{}%", k)))
                ));
            }
            sql.push_str(&format!(" ORDER BY v FETCH FIRST {} ROWS ONLY", lim + 1));

            let mut out: Vec<String> = Vec::new();
            for r in query_objs(conn, &sql)? {
                if let Some(s) = opt_col(&r, "V") {
                    out.push(s.chars().take(64).collect());
                }
            }
            let truncated = out.len() > lim;
            out.truncate(lim);
            Ok((out, truncated))
        })
        .await
    }

    async fn truncate_table(&self, database: &str, table: &str) -> Result<(), String> {
        let database = database.to_string();
        let table = table.to_string();
        self.with_conn(move |conn| {
            let owner = resolve_owner(conn, &database)?;
            let qualified = format!("{}.{}", quote_ident(&owner), quote_ident(&table));
            conn.execute(&format!("TRUNCATE TABLE {}", qualified), &[])
                .map(|_| ())
                .map_err(|e| e.to_string())?;
            conn.commit().map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
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
        let database = database.to_string();
        let table = table.to_string();
        let columns = columns.to_vec();
        let rows = rows.to_vec();
        self.with_conn(move |conn| {
            let owner = resolve_owner(conn, &database)?;
            let qualified = format!("{}.{}", quote_ident(&owner), quote_ident(&table));
            let col_list = columns
                .iter()
                .map(|c| quote_ident(c))
                .collect::<Vec<_>>()
                .join(", ");
            // Oracle 不支持多行 VALUES，改用 INSERT ALL ... SELECT 1 FROM DUAL。
            for chunk in rows.chunks(500) {
                let mut sb = String::from("INSERT ALL\n");
                for row in chunk {
                    let tuple = row
                        .iter()
                        .map(|c| sql_literal(c.as_deref()))
                        .collect::<Vec<_>>()
                        .join(", ");
                    sb.push_str(&format!("INTO {} ({}) VALUES ({})\n", qualified, col_list, tuple));
                }
                sb.push_str("SELECT 1 FROM DUAL");
                conn.execute(&sb, &[]).map_err(|e| e.to_string())?;
            }
            conn.commit().map_err(|e| e.to_string())?;
            Ok(())
        })
        .await
    }
}

/// JSON 单元格 → INSERT 用的 SQL 字面量。
fn json_literal(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::String(s) => sql_literal(Some(s)),
        other => other.to_string(),
    }
}
