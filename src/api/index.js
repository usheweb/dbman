// Tauri 命令封装：一一对应 Rust 后端的 #[tauri::command]
// 说明：Rust 侧命令返回 Result<T, String>，出错时 invoke 会 reject 一个字符串；
// 凡是「连接句柄失效」都会返回精确文案 "Invalid connection"，由 connection store 触发静默重连。
import { invoke } from '@tauri-apps/api/core';

// ===== 连接 =====
export function connect(payload) {
  return invoke('connect', { payload });
}

export function disconnect(connId) {
  return invoke('disconnect', { connId });
}

// ===== 关系型数据库（MySQL / 后续 PostgreSQL、SQLite 等通用命令）=====
export function dbDatabases(connId) {
  return invoke('db_databases', { connId });
}

// limit 非空时后端只返回前 N 张表（首屏快速渲染用），为 null 时返回全量
export function dbTables(connId, database, filter, limit) {
  return invoke('db_tables', { connId, database, filter: filter || null, limit: limit || null });
}

export function dbColumns(connId, database, table) {
  return invoke('db_columns', { connId, database, table });
}

export function dbQuery(connId, sql, database, limit, offset) {
  return invoke('db_query', { connId, sql, database: database || null, limit, offset });
}

export function dbExport(connId, sql, database, headers, columns, path) {
  return invoke('db_export', {
    connId,
    sql,
    database: database || null,
    headers,
    columns,
    path
  });
}

// 表结构 / DDL
export function dbTableDdl(connId, database, table) {
  return invoke('db_table_ddl', { connId, database, table });
}

export function dbTableStructure(connId, database, table) {
  return invoke('db_table_structure', { connId, database, table });
}

export function dbExportDataSql(connId, database, table, path, withDrop, withCreate, batchSize) {
  return invoke('db_export_data_sql', { connId, database, table, path, withDrop, withCreate, batchSize });
}

// 执行（可手改的）DDL / SQL 脚本，支持多条语句
export function dbExecScript(connId, database, sql) {
  return invoke('db_exec_script', { connId, database, sql });
}

// ===== Redis =====
export function redisInfo(connId) {
  return invoke('redis_info', { connId });
}

// cursor 为 SCAN 游标，0 表示从第一页开始；返回 { cursor, total, keys }
export function redisKeys(connId, pattern, cursor = 0) {
  return invoke('redis_keys', { connId, pattern: pattern || '*', cursor });
}

export function redisGet(connId, key) {
  return invoke('redis_get', { connId, key });
}

export function redisSelectDb(connId, db) {
  return invoke('redis_select_db', { connId, db });
}

export function redisDatabases(connId) {
  return invoke('redis_databases', { connId });
}

export function redisSetTtl(connId, key, ttl) {
  return invoke('redis_set_ttl', { connId, key, ttl });
}

export function redisSetValue(connId, key, value) {
  return invoke('redis_set_value', { connId, key, value });
}

// ===== MongoDB（文档型：库 → 集合 → 文档）=====
// spec 是一段 JSON 查询描述（与关系型的 SQL 文本位置同构），形如
// {"mode":"find","collection":"users","filter":{},"sort":{"_id":1}} 或
// {"mode":"aggregate","collection":"users","pipeline":[...]}
export function mongoDatabases(connId) {
  return invoke('mongo_databases', { connId });
}

export function mongoCollections(connId, database) {
  return invoke('mongo_collections', { connId, database });
}

export function mongoQuery(connId, database, spec, limit, skip) {
  return invoke('mongo_query', { connId, database, spec, limit, skip });
}

export function mongoIndexes(connId, database, collection) {
  return invoke('mongo_indexes', { connId, database, collection });
}

export function mongoExport(connId, database, spec, headers, columns, path) {
  return invoke('mongo_export', { connId, database, spec, headers, columns, path });
}

// ===== AI：大模型生成 SQL =====
// config: { baseUrl, apiKey, model, timeoutSecs?, maxRounds? }
// continueSession: true 时在上一次的会话上下文基础上继续（追问式微调）
export function aiGenerateSql(connId, database, prompt, config, continueSession) {
  return invoke('ai_generate_sql', { connId, database, prompt, config, continueSession });
}

export function aiTestConfig(config) {
  return invoke('ai_test_config', { config });
}

// ===== 本地 JSON 存储 =====
export function storeLoad() {
  return invoke('store_load');
}

export function storeSet(key, value) {
  return invoke('store_set', { key, value });
}

// ===== 系统钥匙串（密码等秘密不进 connections.json）=====
// account 约定：连接用 `conn-<连接id>`，AI 配置用 `ai-config`
export function secretSet(account, value) {
  return invoke('secret_set', { account, value });
}

// 没存过返回 null；钥匙串不可用会 reject，调用方据此降级为明文保存
export function secretGet(account) {
  return invoke('secret_get', { account });
}

export function secretDelete(account) {
  return invoke('secret_delete', { account });
}

// 直接写文本文件（当前页导出用：数据已在前端内存里，不必再查一次库）
export function writeTextFile(path, content) {
  return invoke('write_text_file', { path, content });
}

// 读取文本文件（外部 SQL 文件导入编辑器用），后端已处理 UTF-8 / GB18030
export function readTextFile(path) {
  return invoke('read_text_file', { path });
}

// ===== CSV 导入 =====
// encoding: auto | utf-8 | gbk | gb18030；delimiter: auto | , | ; | | | tab
export function csvPreview(path, encoding, delimiter, hasHeader, limit) {
  return invoke('csv_preview', {
    path,
    encoding: encoding || null,
    delimiter: delimiter || null,
    hasHeader,
    limit
  });
}

// mapping：数组下标对应 CSV 列序，值为目标字段名（null 表示该列不导入）
export function dbImportCsv(connId, database, table, path, mapping, options) {
  return invoke('db_import_csv', { connId, database, table, path, mapping, options });
}

// 文件对话框（导出 CSV 用 save，选择 SQL/CSV 文件用 open）+ 原生确认框（危险操作二次确认）
export { save as saveDialog, open as openDialog, confirm as confirmDialog } from '@tauri-apps/plugin-dialog';
