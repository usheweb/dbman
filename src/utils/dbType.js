// 数据库类型归类工具。
// MariaDB 与 MySQL 同协议，前端行为（数据库字段、AI 生成、表树等）与之保持一致，
// 后续同族类型（如 Percona Server）也在此登记，避免各处散落判断。

// 与 MySQL 行为一致的数据库类型
export const MYSQL_LIKE = ['mysql', 'mariadb'];

// 已接入「关系型驱动抽象层」的数据库类型（后端 db::is_relational 一一对应）
export const RELATIONAL = [
  'mysql',
  'mariadb',
  'postgres',
  'postgresql',
  'sqlite',
  'sqlserver',
  'mssql',
  'oracle'
];

// 本地文件型数据库：无主机/端口，凭文件路径连接
export const FILE_BASED = ['sqlite'];

// 文档型数据库（MongoDB）：库 → 集合 → 文档，查询语言是 JSON 而非 SQL，
// 后端走独立的 mongo_cmd 命令集，不套关系型驱动抽象层
export const MONGO_LIKE = ['mongodb', 'mongo'];

/** 该类型是否按 MySQL 语义处理 */
export function isMysqlLike(type) {
  return MYSQL_LIKE.includes(type);
}

/** 该类型是否走关系型驱动（库表结构 / SQL 工作区） */
export function isRelational(type) {
  return RELATIONAL.includes(type);
}

/** 该类型是否为本地文件型（无主机/端口） */
export function isFileBased(type) {
  return FILE_BASED.includes(type);
}

/** 该类型是否为文档型（MongoDB：库-集合树 + JSON 查询工作区） */
export function isMongoLike(type) {
  return MONGO_LIKE.includes(type);
}
