use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// /api/connect 请求体
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectRequest {
    #[serde(rename = "type")]
    pub r#type: String,
    pub config: DbConfig,
    /// 前端总是传对象，但旧数据可能传 null，这里容错处理
    #[serde(default)]
    pub ssh: Option<SshConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DbConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub db: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SshConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default, rename = "privateKey")]
    pub private_key: Option<String>,
}

/// 表列表用：表名/注释 + information_schema.TABLES 的排序维度字段。
/// 各字段都可能为 NULL（视图、无引擎、无自增等），前端按「空值排最后」处理。
#[derive(Debug, Serialize)]
pub struct TableInfo {
    #[serde(rename = "tableName")]
    pub table_name: String,
    #[serde(rename = "tableComment")]
    pub table_comment: Option<String>,
    #[serde(rename = "createTime")]
    pub create_time: Option<String>,
    #[serde(rename = "updateTime")]
    pub update_time: Option<String>,
    #[serde(rename = "tableRows")]
    pub table_rows: Option<u64>,
    #[serde(rename = "dataLength")]
    pub data_length: Option<u64>,
    #[serde(rename = "indexLength")]
    pub index_length: Option<u64>,
    /// DATA_LENGTH + INDEX_LENGTH，两者都为空时为 None
    #[serde(rename = "totalLength")]
    pub total_length: Option<u64>,
    pub engine: Option<String>,
    pub charset: Option<String>,
    #[serde(rename = "tableType")]
    pub table_type: Option<String>,
    #[serde(rename = "autoIncrement")]
    pub auto_increment: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ColumnInfo {
    #[serde(rename = "columnName")]
    pub column_name: String,
    #[serde(rename = "columnComment")]
    pub column_comment: Option<String>,
    #[serde(rename = "dataType")]
    pub data_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FieldInfo {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct QueryResponse {
    pub write: bool,
    pub rows: Vec<serde_json::Value>,
    pub fields: Vec<FieldInfo>,
    pub affected: u64,
    pub paged: bool,
}

/// 多语句脚本中单条语句的执行结果（结果区逐条反馈列表的数据源）。
#[derive(Debug, Serialize)]
pub struct StatementResult {
    /// 该条语句原文：前端做单行摘要展示 + 悬浮提示全文
    pub sql: String,
    /// "success" | "error" | "skipped"（失败即停后剩余条目标记 skipped）
    pub status: String,
    /// SELECT 返回的行数；写语句为 0
    pub rows: u64,
    /// 影响行数（DDL 的 OK 包恒为 0，前端照实展示）
    pub affected: u64,
    /// 该条耗时（毫秒）；skipped 为 0
    pub elapsed_ms: u64,
    /// status = "error" 时的错误文案；其余为空串
    pub error: String,
}

impl StatementResult {
    pub fn success(sql: String, rows: u64, affected: u64, elapsed_ms: u64) -> Self {
        Self {
            sql,
            status: "success".to_string(),
            rows,
            affected,
            elapsed_ms,
            error: String::new(),
        }
    }

    pub fn error(sql: String, error: String, elapsed_ms: u64) -> Self {
        Self {
            sql,
            status: "error".to_string(),
            rows: 0,
            affected: 0,
            elapsed_ms,
            error,
        }
    }

    pub fn skipped(sql: String) -> Self {
        Self {
            sql,
            status: "skipped".to_string(),
            rows: 0,
            affected: 0,
            elapsed_ms: 0,
            error: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportColumn {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default, rename = "enumMap")]
    pub enum_map: Option<HashMap<String, String>>,
}

/// 表结构设计器用的列定义（information_schema.COLUMNS）
#[derive(Debug, Clone, Serialize)]
pub struct ColumnDef {
    pub name: String,
    /// COLUMN_TYPE：带 unsigned / 长度 / enum 值，如 varchar(255)、int unsigned
    #[serde(rename = "columnType")]
    pub column_type: String,
    #[serde(rename = "dataType")]
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    #[serde(rename = "columnKey")]
    pub column_key: Option<String>,
    /// EXTRA：auto_increment / on update CURRENT_TIMESTAMP / VIRTUAL GENERATED 等
    pub extra: String,
    pub comment: Option<String>,
    pub charset: Option<String>,
    pub collation: Option<String>,
    /// 生成列表达式，普通列为 None
    #[serde(rename = "generationExpression")]
    pub generation_expression: Option<String>,
    pub ordinal: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexDef {
    pub name: String,
    pub unique: bool,
    #[serde(rename = "indexType")]
    pub index_type: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignKeyDef {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(rename = "refTable")]
    pub ref_table: String,
    #[serde(rename = "refColumns")]
    pub ref_columns: Vec<String>,
    #[serde(rename = "onDelete")]
    pub on_delete: String,
    #[serde(rename = "onUpdate")]
    pub on_update: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableOptionDef {
    pub engine: Option<String>,
    pub charset: Option<String>,
    pub collation: Option<String>,
    pub comment: Option<String>,
    #[serde(rename = "autoIncrement")]
    pub auto_increment: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableStructure {
    pub columns: Vec<ColumnDef>,
    pub indexes: Vec<IndexDef>,
    #[serde(rename = "foreignKeys")]
    pub foreign_keys: Vec<ForeignKeyDef>,
    pub options: TableOptionDef,
}

/// CSV 预览结果：headers 与 rows 均已按列数对齐，前端据此渲染映射表
#[derive(Debug, Serialize)]
pub struct CsvPreview {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    #[serde(rename = "totalRows")]
    pub total_rows: usize,
    /// 实际用于解码的编码名（auto 模式会回填检测结果）
    pub encoding: String,
    /// 实际使用的分隔符（如 "," / "\t"）
    pub delimiter: String,
    /// 列数（取所有行中的最大值）
    pub columns: usize,
}

/// CSV 导入选项
#[derive(Debug, Deserialize)]
pub struct CsvImportOptions {
    #[serde(rename = "hasHeader")]
    pub has_header: bool,
    /// "auto" / "utf-8" / "gbk" / "gb18030"；缺省按 auto 处理
    pub encoding: Option<String>,
    /// 分隔符：auto / "," / ";" / "|" / "tab"；缺省自动探测
    pub delimiter: Option<String>,
    /// 视为 NULL 的文本（如 "NULL"、"\\N"），空串表示不启用
    #[serde(rename = "nullValue")]
    pub null_value: Option<String>,
    /// 空单元格是否按 NULL 写入
    #[serde(rename = "emptyAsNull")]
    pub empty_as_null: Option<bool>,
    /// 导入前先 TRUNCATE 目标表
    #[serde(rename = "truncateFirst")]
    pub truncate_first: Option<bool>,
    /// 出错时立即中止；false 则跳过坏行继续
    #[serde(rename = "stopOnError")]
    pub stop_on_error: Option<bool>,
    #[serde(rename = "batchSize")]
    pub batch_size: Option<usize>,
}

/// CSV 导入结果
#[derive(Debug, Serialize)]
pub struct CsvImportResult {
    pub inserted: u64,
    pub failed: u64,
    /// 失败明细（最多保留前 20 条）
    pub errors: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct RedisInfo {
    pub info: String,
    pub dbsize: u64,
}

#[derive(Debug, Serialize)]
pub struct RedisKey {
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: String,
}

// 一页 key：SCAN 游标分页的返回体
#[derive(Debug, Serialize)]
pub struct RedisKeyPage {
    /// 下一页的游标，0 表示已经扫到末尾
    pub cursor: u64,
    /// 当前库的 key 总数（DBSIZE，与 pattern 无关，用于界面展示规模）
    pub total: u64,
    pub keys: Vec<RedisKey>,
}

#[derive(Debug, Serialize)]
pub struct RedisValue {
    #[serde(rename = "type")]
    pub r#type: String,
    pub value: serde_json::Value,
    pub ttl: i64,
}

/// MongoDB 集合（文档型数据库没有列定义，集合即关系型里的「表」）
#[derive(Debug, Serialize)]
pub struct MongoCollection {
    pub name: String,
}

/// MongoDB 索引定义
#[derive(Debug, Serialize)]
pub struct MongoIndex {
    pub name: String,
    /// 键定义，原始 JSON 文本（如 {"_id":1}）
    pub keys: String,
    pub unique: bool,
}
