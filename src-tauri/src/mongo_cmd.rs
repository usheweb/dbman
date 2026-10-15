//! MongoDB 命令层。
//!
//! 文档型数据库与关系型的语义完全不同：没有表结构 / DDL / 外键，查询语言是
//! find 过滤条件与 aggregate 管道（JSON）而不是 SQL，所以不套用 db::RelationalDriver，
//! 而是像 redis_cmd 那样单独一组命令：库 → 集合 → 文档。
//!
//! 前端的「查询描述」与关系型的 lastSql 同构：一段 JSON 文本，形如
//! `{"mode":"find","collection":"users","filter":{},"sort":{"_id":1}}` 或
//! `{"mode":"aggregate","collection":"users","pipeline":[...]}`。

use std::io::Write;

use mongodb::bson::{doc, Bson, Document};
use mongodb::{Client, Cursor, IndexModel};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use tauri::State;

use crate::db::util::{csv_escape, export_cell};
use crate::models::{
    DbConfig, ExportColumn, FieldInfo, MongoCollection, MongoIndex, QueryResponse,
};
use crate::state::AppState;

/// 连接串里的用户名/密码必须百分号编码，否则密码中的 @ : / ? 会破坏 URI 结构
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// 建立连接。config.database 对 MongoDB 表示「认证库」（authSource），留空按 admin。
pub async fn connect(cfg: &DbConfig, host: &str, port: u16) -> Result<Client, String> {
    let user = cfg.user.clone().unwrap_or_default();
    let password = cfg.password.clone().unwrap_or_default();
    let auth_db = cfg
        .database
        .clone()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| "admin".to_string());

    let mut uri = String::from("mongodb://");
    if !user.is_empty() {
        uri.push_str(&percent_encode(&user));
        if !password.is_empty() {
            uri.push(':');
            uri.push_str(&percent_encode(&password));
        }
        uri.push('@');
    }
    // IPv6 字面量必须写成 [::1]:27017
    if host.contains(':') && !host.starts_with('[') {
        uri.push_str(&format!("[{}]:{}", host, port));
    } else {
        uri.push_str(&format!("{}:{}", host, port));
    }
    uri.push_str(&format!(
        "/?authSource={}&serverSelectionTimeoutMS=8000&connectTimeoutMS=8000",
        percent_encode(&auth_db)
    ));

    let client = Client::with_uri_str(&uri).await.map_err(|e| e.to_string())?;
    // 主动 ping 一次：参数写错时驱动可能只是延迟报错，这里立刻给出结论
    client
        .database(&auth_db)
        .run_command(doc! { "ping": 1 })
        .await
        .map_err(|e| e.to_string())?;
    Ok(client)
}

/// 前端提交的查询描述
#[derive(Debug, Deserialize)]
struct MongoQuerySpec {
    /// find | aggregate
    #[serde(default)]
    mode: String,
    #[serde(default)]
    collection: String,
    #[serde(default)]
    filter: Option<String>,
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    projection: Option<String>,
    #[serde(default)]
    pipeline: Option<String>,
}

fn parse_spec(spec: &str) -> Result<MongoQuerySpec, String> {
    let parsed: MongoQuerySpec = serde_json::from_str(spec)
        .map_err(|e| format!("查询描述格式不正确：{}", e))?;
    if parsed.collection.trim().is_empty() {
        return Err("请先在左侧选择一个集合".to_string());
    }
    Ok(parsed)
}

fn non_empty(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|t| !t.is_empty())
}

/// 编辑框里的 JSON 文本转 BSON 文档；空文本按 {} 处理
fn json_to_doc(text: &str, what: &str) -> Result<Document, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(Document::new());
    }
    let value: JsonValue =
        serde_json::from_str(t).map_err(|e| format!("{}不是合法 JSON：{}", what, e))?;
    if !value.is_object() {
        return Err(format!("{}需要是 JSON 对象，例如 {{\"status\":\"active\"}}", what));
    }
    serde_json::from_value(value).map_err(|e| format!("{}格式不正确：{}", what, e))
}

/// 编辑框里的聚合管道文本转 BSON 数组
fn json_to_pipeline(text: &str) -> Result<Vec<Document>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(Vec::new());
    }
    let value: JsonValue =
        serde_json::from_str(t).map_err(|e| format!("聚合管道不是合法 JSON：{}", e))?;
    let arr = value
        .as_array()
        .ok_or_else(|| "聚合管道需要是 JSON 数组，例如 [{\"$match\":{}}]".to_string())?;
    arr.iter()
        .map(|stage| {
            if !stage.is_object() {
                return Err("聚合管道的每一段都需要是 JSON 对象".to_string());
            }
            serde_json::from_value(stage.clone()).map_err(|e| format!("聚合管道格式不正确：{}", e))
        })
        .collect()
}

/// BSON → JSON。文档型数据没有统一列，嵌套对象/数组原样保留，
/// 让前端按 JSON 文本展示；驱动特有的值转成可读文本。
fn bson_to_json(v: &Bson) -> JsonValue {
    match v {
        Bson::Null | Bson::Undefined => JsonValue::Null,
        Bson::Boolean(b) => JsonValue::Bool(*b),
        Bson::Int32(i) => json!(i),
        Bson::Int64(i) => json!(i),
        Bson::Double(f) => json!(f),
        Bson::String(s) | Bson::Symbol(s) | Bson::JavaScriptCode(s) => {
            JsonValue::String(s.clone())
        }
        Bson::ObjectId(oid) => JsonValue::String(oid.to_hex()),
        Bson::DateTime(dt) => JsonValue::String(
            dt.try_to_rfc3339_string()
                .unwrap_or_else(|_| format!("{:?}", dt)),
        ),
        Bson::Decimal128(d) => JsonValue::String(d.to_string()),
        Bson::Binary(b) => JsonValue::String(format!("<binary {} bytes>", b.bytes.len())),
        Bson::RegularExpression(r) => JsonValue::String(format!("/{}/{}", r.pattern, r.options)),
        Bson::Timestamp(t) => json!(t.time),
        Bson::MaxKey => JsonValue::String("<maxKey>".to_string()),
        Bson::MinKey => JsonValue::String("<minKey>".to_string()),
        Bson::Array(arr) => JsonValue::Array(arr.iter().map(bson_to_json).collect()),
        Bson::Document(d) => {
            let mut map = serde_json::Map::with_capacity(d.len());
            for (k, val) in d.iter() {
                map.insert(k.clone(), bson_to_json(val));
            }
            JsonValue::Object(map)
        }
        other => JsonValue::String(format!("{:?}", other)),
    }
}

/// 从当前页文档取字段并集：文档型数据字段不固定，按首次出现顺序排列，
/// 与前端「结果表按 fields 渲染」的约定保持一致。
fn fields_from_rows(rows: &[JsonValue]) -> Vec<FieldInfo> {
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        if let JsonValue::Object(map) = row {
            for key in map.keys() {
                if seen.insert(key.clone()) {
                    names.push(key.clone());
                }
            }
        }
    }
    names.into_iter().map(|name| FieldInfo { name }).collect()
}

/// 按上限收一页文档；limit+1 由调用方传入，用来判断是否还有下一页
async fn collect_docs(cursor: &mut Cursor<Document>, max: i64) -> Result<Vec<JsonValue>, String> {
    let cap = max.max(0) as usize;
    let mut rows: Vec<JsonValue> = Vec::new();
    while rows.len() < cap {
        if !cursor.advance().await.map_err(|e| e.to_string())? {
            break;
        }
        let doc: Document = cursor.deserialize_current().map_err(|e| e.to_string())?;
        rows.push(bson_to_json(&Bson::Document(doc)));
    }
    Ok(rows)
}

/// 打开查询游标：find 与 aggregate 两种模式共用后续的取值/导出流程
enum QueryCursor {
    Find(Cursor<Document>),
    Aggregate(Cursor<Document>),
}

impl QueryCursor {
    fn inner(&mut self) -> &mut Cursor<Document> {
        match self {
            QueryCursor::Find(c) | QueryCursor::Aggregate(c) => c,
        }
    }
}

async fn open_cursor(
    client: &Client,
    database: &str,
    spec: &MongoQuerySpec,
    limit: i64,
    skip: i64,
) -> Result<QueryCursor, String> {
    let coll = client
        .database(database)
        .collection::<Document>(&spec.collection);
    if spec.mode == "aggregate" {
        let pipeline = json_to_pipeline(spec.pipeline.as_deref().unwrap_or(""))?;
        let cursor = coll.aggregate(pipeline).await.map_err(|e| e.to_string())?;
        return Ok(QueryCursor::Aggregate(cursor));
    }
    let filter = json_to_doc(spec.filter.as_deref().unwrap_or(""), "过滤条件")?;
    let mut find = coll.find(filter);
    if let Some(sort) = non_empty(spec.sort.as_deref()) {
        find = find.sort(json_to_doc(sort, "排序")?);
    }
    if let Some(projection) = non_empty(spec.projection.as_deref()) {
        find = find.projection(json_to_doc(projection, "字段筛选")?);
    }
    let cursor = find
        .skip(skip.max(0) as u64)
        .limit(limit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(QueryCursor::Find(cursor))
}

#[tauri::command]
pub async fn mongo_databases(
    state: State<'_, AppState>,
    conn_id: String,
) -> Result<Vec<String>, String> {
    let client = state.get_mongo(&conn_id).await?;
    let mut names = client
        .list_database_names()
        .await
        .map_err(|e| e.to_string())?;
    names.sort();
    Ok(names)
}

#[tauri::command]
pub async fn mongo_collections(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
) -> Result<Vec<MongoCollection>, String> {
    let client = state.get_mongo(&conn_id).await?;
    let mut names = client
        .database(&database)
        .list_collection_names()
        .await
        .map_err(|e| e.to_string())?;
    names.sort();
    Ok(names.into_iter().map(|name| MongoCollection { name }).collect())
}

#[tauri::command]
pub async fn mongo_query(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    spec: String,
    limit: i64,
    skip: i64,
) -> Result<QueryResponse, String> {
    let client = state.get_mongo(&conn_id).await?;
    let spec = parse_spec(&spec)?;
    let mut cursor = open_cursor(&client, &database, &spec, limit, skip).await?;
    let rows = collect_docs(cursor.inner(), limit).await?;
    // 聚合管道没有 skip/offset 概念，翻页无从谈起，交给管道里的 $skip/$limit 自行控制
    let paged = spec.mode != "aggregate";
    Ok(QueryResponse {
        write: false,
        fields: fields_from_rows(&rows),
        rows,
        affected: 0,
        paged,
    })
}

#[tauri::command]
pub async fn mongo_indexes(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    collection: String,
) -> Result<Vec<MongoIndex>, String> {
    let client = state.get_mongo(&conn_id).await?;
    let coll = client
        .database(&database)
        .collection::<Document>(&collection);
    let mut cursor = coll.list_indexes().await.map_err(|e| e.to_string())?;
    let mut out: Vec<MongoIndex> = Vec::new();
    while cursor.advance().await.map_err(|e| e.to_string())? {
        let model: IndexModel = cursor.deserialize_current().map_err(|e| e.to_string())?;
        let opts = model.options.unwrap_or_default();
        let name = opts.name.clone().unwrap_or_default();
        let unique = opts.unique.unwrap_or(false);
        let keys = bson_to_json(&Bson::Document(model.keys.clone())).to_string();
        out.push(MongoIndex { name, keys, unique });
    }
    Ok(out)
}

/// 把查询结果整体导出为 CSV：与关系型一样由后端重新执行查询、流式写文件，
/// 不在前端内存里攒全量数据。
#[tauri::command]
pub async fn mongo_export(
    state: State<'_, AppState>,
    conn_id: String,
    database: String,
    spec: String,
    headers: Vec<String>,
    columns: Vec<ExportColumn>,
    path: String,
) -> Result<u64, String> {
    if columns.is_empty() {
        return Err("缺少导出列信息".to_string());
    }
    let client = state.get_mongo(&conn_id).await?;
    let spec = parse_spec(&spec)?;
    // 导出全量：不加 limit（0 表示不限）
    let mut cursor = open_cursor(&client, &database, &spec, 0, 0).await?;

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

    let mut count: u64 = 0;
    loop {
        if !cursor
            .inner()
            .advance()
            .await
            .map_err(|e| e.to_string())?
        {
            break;
        }
        let doc: Document = cursor
            .inner()
            .deserialize_current()
            .map_err(|e| e.to_string())?;
        let obj = bson_to_json(&Bson::Document(doc));
        let empty = JsonValue::Null;
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
