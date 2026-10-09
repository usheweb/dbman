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

// ===== MySQL =====
export function mysqlDatabases(connId) {
  return invoke('mysql_databases', { connId });
}

export function mysqlTables(connId, database, filter) {
  return invoke('mysql_tables', { connId, database, filter: filter || null });
}

export function mysqlColumns(connId, database, table) {
  return invoke('mysql_columns', { connId, database, table });
}

export function mysqlQuery(connId, sql, database, limit, offset) {
  return invoke('mysql_query', { connId, sql, database: database || null, limit, offset });
}

export function mysqlExport(connId, sql, database, headers, columns, path) {
  return invoke('mysql_export', {
    connId,
    sql,
    database: database || null,
    headers,
    columns,
    path
  });
}

// 表结构 / DDL
export function mysqlTableDdl(connId, database, table) {
  return invoke('mysql_table_ddl', { connId, database, table });
}

export function mysqlTableStructure(connId, database, table) {
  return invoke('mysql_table_structure', { connId, database, table });
}

export function mysqlExportDataSql(connId, database, table, path, withDrop, withCreate, batchSize) {
  return invoke('mysql_export_data_sql', { connId, database, table, path, withDrop, withCreate, batchSize });
}

// 执行（可手改的）DDL 脚本，支持多条语句
export function mysqlExecDdl(connId, database, sql) {
  return invoke('mysql_exec_ddl', { connId, database, sql });
}

// ===== Redis =====
export function redisInfo(connId) {
  return invoke('redis_info', { connId });
}

export function redisKeys(connId, pattern) {
  return invoke('redis_keys', { connId, pattern: pattern || '*' });
}

export function redisGet(connId, key) {
  return invoke('redis_get', { connId, key });
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
export function mysqlImportCsv(connId, database, table, path, mapping, options) {
  return invoke('mysql_import_csv', { connId, database, table, path, mapping, options });
}

// 文件对话框（导出 CSV 用 save，选择 SQL/CSV 文件用 open）
export { save as saveDialog, open as openDialog } from '@tauri-apps/plugin-dialog';
