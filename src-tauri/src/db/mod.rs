//! 关系型数据库驱动抽象层。
//!
//! 目标：把「一个数据库类型 = 一整套写死的命令」改成「一套通用命令 + 可插拔驱动」，
//! 后续接入 PostgreSQL / SQLite / SQL Server / Oracle 时，只需实现 [`RelationalDriver`]
//! 并在 [`connect_driver`] 里注册，命令层与前端 API 不必改动。
//!
//! 已落地驱动：MySQL/MariaDB、PostgreSQL、SQLite、SQL Server、Oracle。
pub mod mysql;
pub mod oracle;
pub mod postgres;
pub mod sqlite;
pub mod sqlserver;
pub(crate) mod util;

use std::sync::Arc;

use async_trait::async_trait;
use tauri::State;

use crate::models::{
    ColumnInfo, DbConfig, ExportColumn, QueryResponse, TableInfo, TableStructure,
};
use crate::state::AppState;

/// 关系型数据库统一能力接口。实现方需自行持有连接句柄（内部加锁），
/// 并以 `&self` 暴露能力，方便用 `Arc<dyn RelationalDriver>` 统一存储与分发。
#[async_trait]
pub trait RelationalDriver: Send + Sync {
    /// 驱动标识（与前端连接类型一致，如 "mysql"）。
    fn kind(&self) -> &'static str;

    /// 列出可访问的库/模式名。
    async fn databases(&self) -> Result<Vec<String>, String>;

    /// 列出某库中的表（名 + 注释 + 排序维度元数据）；filter 非空时按表名/注释模糊匹配。
    async fn tables(&self, database: &str, filter: Option<&str>) -> Result<Vec<TableInfo>, String>;

    /// 同 [`Self::tables`]，但 `limit` 非空时只取前若干张表。
    /// 表多的库（几百张）前端先用它拿一小批把列表渲染出来，再调 `tables` 拉全量覆盖，避免干等。
    /// 默认实现忽略 limit（驱动可自行覆盖以获得更快的首屏）。
    async fn tables_page(
        &self,
        database: &str,
        filter: Option<&str>,
        _limit: Option<u32>,
    ) -> Result<Vec<TableInfo>, String> {
        self.tables(database, filter).await
    }

    /// 列出某库某表的列（名 + 注释 + 类型）。
    async fn columns(&self, database: &str, table: &str) -> Result<Vec<ColumnInfo>, String>;

    /// 执行查询；对可安全分页的 SELECT/WITH 语句按 limit/offset 追加分页。
    async fn query(
        &self,
        database: Option<&str>,
        sql: &str,
        limit: i64,
        offset: i64,
    ) -> Result<QueryResponse, String>;

    /// 把查询结果导出为 CSV 文件，返回写入行数。
    async fn export_csv(
        &self,
        database: Option<&str>,
        sql: &str,
        headers: &[String],
        columns: &[ExportColumn],
        path: &str,
    ) -> Result<u64, String>;

    /// 取表的建表语句（DDL）。
    async fn table_ddl(&self, database: &str, table: &str) -> Result<String, String>;

    /// 读取表的完整结构：字段 / 索引 / 外键 / 表选项。
    async fn table_structure(&self, database: &str, table: &str)
        -> Result<TableStructure, String>;

    /// 把表数据导出为 INSERT 语句文件，返回写入行数。
    async fn export_data_sql(
        &self,
        database: &str,
        table: &str,
        path: &str,
        with_drop: bool,
        with_create: bool,
        batch_size: i64,
    ) -> Result<u64, String>;

    /// 执行一段（可含多条语句的）脚本，返回执行成功的语句数。
    async fn exec_script(&self, database: &str, sql: &str) -> Result<u64, String>;

    /// 查询某表某字段的去重取值，供 AI 生成 SQL 时确认真实数据值。返回 (取值列表, 是否被截断)。
    async fn distinct_values(
        &self,
        database: &str,
        table: &str,
        column: &str,
        keyword: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<String>, bool), String>;

    /// 清空表（CSV 导入前置操作）。
    async fn truncate_table(&self, database: &str, table: &str) -> Result<(), String>;

    /// 批量插入若干行；rows 中每行的单元格与 columns 一一对应，None 表示 SQL NULL。
    async fn insert_rows(
        &self,
        database: &str,
        table: &str,
        columns: &[String],
        rows: &[Vec<Option<String>>],
    ) -> Result<(), String>;
}

/// 是否为已支持的关系型数据库类型。新增驱动时在此登记。
/// MariaDB 与 MySQL 同协议，复用 mysql 驱动。
pub fn is_relational(db_type: &str) -> bool {
    matches!(
        db_type,
        "mysql" | "mariadb" | "postgres" | "postgresql" | "sqlite" | "sqlserver" | "mssql" | "oracle"
    )
}

/// 是否为「本地文件型」数据库（无主机/端口，凭文件路径连接）。
pub fn is_file_based(db_type: &str) -> bool {
    matches!(db_type, "sqlite")
}

/// 按数据库类型的默认端口补全端口（对齐现有约定）。
pub fn default_port(db_type: &str) -> u16 {
    match db_type {
        "redis" => 6379,
        "mongodb" | "mongo" => 27017,
        "postgres" | "postgresql" => 5432,
        "sqlserver" | "mssql" => 1433,
        "oracle" => 1521,
        _ => 3306,
    }
}

/// 建立关系型数据库驱动。新增驱动时在此登记分支。
pub async fn connect_driver(
    db_type: &str,
    cfg: &DbConfig,
    host: &str,
    port: u16,
) -> Result<Arc<dyn RelationalDriver>, String> {
    match db_type {
        // MariaDB 与 MySQL 同协议，共用同一驱动实现。
        "mysql" | "mariadb" => Ok(Arc::new(mysql::MysqlDriver::connect(cfg, host, port).await?)),
        "postgres" | "postgresql" => {
            Ok(Arc::new(postgres::PostgresDriver::connect(cfg, host, port).await?))
        }
        // SQLite 为本地文件，不使用主机/端口。
        "sqlite" => Ok(Arc::new(sqlite::SqliteDriver::connect(cfg)?)),
        "sqlserver" | "mssql" => {
            Ok(Arc::new(sqlserver::SqlServerDriver::connect(cfg, host, port).await?))
        }
        "oracle" => Ok(Arc::new(oracle::OracleDriver::connect(cfg, host, port)?)),
        other => Err(format!("暂不支持的数据库类型：{}", other)),
    }
}

// ===== 通用 Tauri 命令（与具体数据库无关，统一走驱动抽象层）=====

#[tauri::command]
pub async fn db_databases(
    state: State<'_, AppState>,
    conn_id: String,
) -> Result<Vec<String>, String> {
    state.get_driver(&conn_id).await?.databases().await
}

#[tauri::command]
pub async fn db_tables(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    filter: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<TableInfo>, String> {
    state
        .get_driver(&conn_id)
        .await?
        .tables_page(&database, filter.as_deref(), limit)
        .await
}

#[tauri::command]
pub async fn db_columns(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    table: String,
) -> Result<Vec<ColumnInfo>, String> {
    state
        .get_driver(&conn_id)
        .await?
        .columns(&database, &table)
        .await
}

#[tauri::command]
pub async fn db_query(
    state: State<'_, AppState>,
    conn_id: String,
    sql: String,
    database: Option<String>,
    limit: i64,
    offset: i64,
) -> Result<QueryResponse, String> {
    let database = database.filter(|d| !d.is_empty());
    state
        .get_driver(&conn_id)
        .await?
        .query(database.as_deref(), &sql, limit, offset)
        .await
}

#[tauri::command]
pub async fn db_export(
    state: State<'_, AppState>,
    conn_id: String,
    sql: String,
    database: Option<String>,
    headers: Vec<String>,
    columns: Vec<ExportColumn>,
    path: String,
) -> Result<u64, String> {
    let database = database.filter(|d| !d.is_empty());
    state
        .get_driver(&conn_id)
        .await?
        .export_csv(database.as_deref(), &sql, &headers, &columns, &path)
        .await
}

#[tauri::command]
pub async fn db_table_ddl(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    table: String,
) -> Result<String, String> {
    state
        .get_driver(&conn_id)
        .await?
        .table_ddl(&database, &table)
        .await
}

#[tauri::command]
pub async fn db_table_structure(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    table: String,
) -> Result<TableStructure, String> {
    state
        .get_driver(&conn_id)
        .await?
        .table_structure(&database, &table)
        .await
}

#[tauri::command]
pub async fn db_export_data_sql(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    table: String,
    path: String,
    with_drop: bool,
    with_create: bool,
    batch_size: i64,
) -> Result<u64, String> {
    state
        .get_driver(&conn_id)
        .await?
        .export_data_sql(&database, &table, &path, with_drop, with_create, batch_size)
        .await
}

#[tauri::command]
pub async fn db_exec_script(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    sql: String,
) -> Result<u64, String> {
    state
        .get_driver(&conn_id)
        .await?
        .exec_script(&database, &sql)
        .await
}
