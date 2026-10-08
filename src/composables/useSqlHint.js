// SQL 补全引擎：从原 dbman/public/index.html 内联脚本迁移而来，逻辑保持一致
// 触发时实时读取连接 store 的表 / 列信息，避免用编辑器创建时的快照
import CodeMirror from 'codemirror';
import 'codemirror/mode/sql/sql.js';
import 'codemirror/addon/hint/show-hint.js';
import 'codemirror/addon/hint/sql-hint.js';
import { fuzzyScore, sqlKeywords, quoteIdent } from '@/utils/sql';

// 前置关键字 + 语句结构共同决定该位置能补全什么：
//   table   该位置要写表名（FROM / JOIN 之后，或 FROM 子句里的逗号之后）
//   column  该位置写列名 / 关键字，不再混入表名（表名已经写完了）
//   none    该位置是数字或字面量（LIMIT / OFFSET / AS ...），不给补全
//   default 语句开头等判断不了的位置，表名与关键字都给
const TABLE_KEYWORDS = ['FROM', 'JOIN', 'INTO', 'UPDATE', 'TABLE', 'TRUNCATE'];
const COLUMN_KEYWORDS = ['SELECT', 'WHERE', 'ON', 'AND', 'OR', 'HAVING', 'SET', 'ORDER', 'GROUP', 'BY', 'USING'];
const NONE_KEYWORDS = ['LIMIT', 'OFFSET', 'AS', 'VALUES'];
// DESC / DESCRIBE / EXPLAIN 只有在语句开头才是「查表结构」，否则是 ORDER BY 里的降序
const HEAD_TABLE_COMMANDS = ['DESC', 'DESCRIBE', 'EXPLAIN'];
// 回溯到最近一个子句关键字，判断逗号后面还在不在「列表名」的子句里
const CLAUSE_KEYWORDS = ['FROM', 'JOIN', 'SELECT', 'SET', 'INTO', 'VALUES', 'WHERE', 'GROUP', 'ORDER', 'HAVING', 'ON', 'AND', 'OR'];

// FROM / JOIN 后面这些词不是别名，别被当成别名吃掉
const NOT_ALIAS_WORDS = ['LEFT', 'RIGHT', 'INNER', 'OUTER', 'FULL', 'CROSS', 'JOIN', 'STRAIGHT_JOIN',
  'WHERE', 'ON', 'GROUP', 'ORDER', 'LIMIT', 'OFFSET', 'HAVING', 'UNION', 'SET', 'VALUES',
  'USING', 'AND', 'OR', 'NOT', 'IN', 'IS', 'LIKE', 'BETWEEN', 'AS', 'FORCE', 'USE', 'IGNORE'];

// 构建 CodeMirror sql-hint 需要的表结构：{ 表名: { columns: [列名], displayText: "表名  注释" } }
// 注意：addon 只认对象形式，传字符串数组会在 wrapTable 里抛
// "Cannot create property 'text' on string"，导致补全弹窗完全不出现
export function buildSqlHintTables(connStore) {
  const tables = {};
  connStore.allTables.forEach(t => {
    const entry = { columns: (connStore.tableColumns[t.tableName] || []).map(c => c.columnName) };
    if (t.tableComment) entry.displayText = `${t.tableName}  ${t.tableComment}`;
    tables[t.tableName] = entry;
  });
  // 已加载过列、但不在当前表列表里的表（例如手写 SQL 涉及的表）也补进去
  Object.keys(connStore.tableColumns).forEach(name => {
    if (!tables[name]) {
      tables[name] = { columns: connStore.tableColumns[name].map(c => c.columnName) };
    }
  });
  return tables;
}

// 触发补全时实时取数据，避免用编辑器创建时的快照（那时列注释往往还没加载）
export function showSqlHint(cm, connStore) {
  const token = cm.getTokenAt(cm.getCursor());
  if (token.type === 'comment' || token.type === 'string') return;
  // LIMIT / OFFSET 这类位置没有可补全的，顺便把已经弹出的下拉关掉
  if (getSqlContext(cm, cm.getCursor(), '') === 'none') {
    if (typeof cm.closeHint === 'function') cm.closeHint();
    return;
  }
  cm.showHint({
    completeSingle: false,
    tables: buildSqlHintTables(connStore),
    hint: (cm2, options) => sqlHintFuzzy(cm2, options, connStore)
  });
}

// 取文本末端的那个词（标识符 / 数字 / 反引号），跳过结尾空白与分号
export function lastWordOf(text) {
  const t = String(text).replace(/[\s;]+$/, '');
  const m = /([A-Za-z_][A-Za-z0-9_]*|[\w.`'"]+)$/.exec(t);
  return m ? m[1].toUpperCase() : '';
}

// 逗号后继续列表名只发生在 FROM / JOIN 子句内部
export function inFromClause(text) {
  const words = String(text).toUpperCase().match(/[A-Z_][A-Z0-9_]*/g) || [];
  for (let i = words.length - 1; i >= 0; i--) {
    if (CLAUSE_KEYWORDS.indexOf(words[i]) !== -1) {
      return words[i] === 'FROM' || words[i] === 'JOIN';
    }
  }
  return false;
}

// 判断光标处的上下文；search 是当前正在补全的那段文本（含反引号）
export function getSqlContext(cm, pos, search) {
  // 「表名.」形式：一定是在写列名
  if (String(search || '').indexOf('.') !== -1) return 'column';
  const before = cm.getRange({ line: 0, ch: 0 }, pos);
  // 先去掉正在输入的那一段（反引号 / 点 / 字母数字下划线），免得把半截词当成关键字
  const cleaned = before.replace(/[\s]*[`\w.]*$/, '');
  if (/,\s*$/.test(cleaned)) return inFromClause(cleaned) ? 'table' : 'column';
  // 左括号后面要么是子查询（要关键字），要么是字段清单，都不该出表名
  if (/\(\s*$/.test(cleaned)) return 'column';
  const word = lastWordOf(cleaned);
  if (word) {
    if (NONE_KEYWORDS.indexOf(word) !== -1) return 'none';
    if (COLUMN_KEYWORDS.indexOf(word) !== -1) return 'column';
    if (TABLE_KEYWORDS.indexOf(word) !== -1) return 'table';
    if (HEAD_TABLE_COMMANDS.indexOf(word) !== -1) {
      const seg = cleaned.slice(cleaned.lastIndexOf(';') + 1);
      const first = (seg.match(/^\s*\(*\s*([A-Za-z_][A-Za-z0-9_]*)/) || [])[1];
      return first && first.toUpperCase() === word ? 'table' : 'column';
    }
  }
  // 末端是标识符 / 数字 / 运算符（如 ON u.id= ）都说明表名已经写完，不该再出表名；
  // 只有空白/空语句才落到 default
  return /\S/.test(cleaned) ? 'column' : 'default';
}

// 解析语句里 FROM / JOIN 用到的表与别名，返回 { 小写名字: 真实表名 }
export function parseTableRefs(text) {
  const refs = {};
  const re = /(?:\bfrom\b|\bjoin\b)\s+`?([A-Za-z_][\w$]*)`?(?:\s+as\s+`?([A-Za-z_][\w$]*)`?|\s+`?([A-Za-z_][\w$]*)`?)?/gi;
  let m;
  while ((m = re.exec(text))) {
    const table = m[1];
    let alias = m[2] || m[3] || '';
    if (alias && NOT_ALIAS_WORDS.indexOf(alias.toUpperCase()) !== -1) alias = '';
    if (!Object.prototype.hasOwnProperty.call(refs, table.toLowerCase())) refs[table.toLowerCase()] = table;
    if (alias) refs[alias.toLowerCase()] = table;
  }
  return refs;
}

// 把用户写的名字（可能是别名）换成 tableColumns 里真实存在的表名
export function resolveTableName(cm, name, connStore) {
  const lower = String(name || '').toLowerCase();
  if (!lower) return '';
  const keys = Object.keys(connStore.tableColumns);
  const direct = keys.find(k => k.toLowerCase() === lower);
  if (direct) return direct;
  const real = parseTableRefs(cm.getRange({ line: 0, ch: 0 }, cm.getCursor()))[lower];
  if (!real) return name;
  return keys.find(k => k.toLowerCase() === real.toLowerCase()) || real;
}

// 语句里用到的表如果还没缓存列信息，后台拉一次，位置没变就重新弹下拉。
// 只对「确实存在于当前库表列表里」的表拉，避免边打字（FROM ca）就发无效请求；
// 拉过的记下来，失败也不反复重试
export function ensureColumns(cm, tables, connStore) {
  if (!connStore.currentDatabase) return;
  const known = {};
  connStore.allTables.forEach(t => { known[t.tableName.toLowerCase()] = t.tableName; });
  const missing = [];
  tables.forEach(name => {
    const real = known[String(name || '').toLowerCase()];
    if (real && !connStore.tableColumns[real] && !connStore.triedColumns[real]) {
      connStore.triedColumns = { ...connStore.triedColumns, [real]: true };
      missing.push(real);
    }
  });
  if (!missing.length) return;
  const pos = cm.getCursor();
  Promise.all(missing.map(t => connStore.loadColumns(connStore.currentDatabase, t))).then(() => {
    // loadColumns 内部吞掉了错误，没拿到列的表要把标记撤回，否则一次失败就永久不再尝试
    missing.forEach(t => { if (!connStore.tableColumns[t]) connStore.triedColumns = { ...connStore.triedColumns, [t]: false }; });
    const cur = cm.getCursor();
    if (cur.line === pos.line && cur.ch === pos.ch) showSqlHint(cm, connStore);
  });
}

// 语句里引用到的表 → 「前缀.字段」候选（prefix 是别名，没别名时是表名）。
// 表起了别名后 MySQL 不允许再写原表名，所以有别名时只出「别名.字段」。
export function scopedColumnRefs(cm, connStore) {
  const refs = parseTableRefs(cm.getRange({ line: 0, ch: 0 }, cm.getCursor()));
  const aliased = {};
  Object.keys(refs).forEach(k => {
    const real = refs[k].toLowerCase();
    if (k !== real) aliased[real] = true;
  });
  const keys = Object.keys(connStore.tableColumns);
  const out = [];
  Object.keys(refs).forEach(prefix => {
    const real = refs[prefix];
    if (prefix === real.toLowerCase() && aliased[real.toLowerCase()]) return;
    const realKey = keys.find(k => k.toLowerCase() === real.toLowerCase());
    (realKey ? connStore.tableColumns[realKey] : []).forEach(c => {
      out.push({ prefix: prefix, columnName: c.columnName });
    });
  });
  return out;
}

export function isTableItem(item) {
  return typeof item === 'object' && String((item && item.className) || '').indexOf('CodeMirror-hint-table') !== -1;
}

// addon 返回的表项在反引号上下文里只有 text 带反引号，displayText 不带，
// 而 show-hint 优先显示 displayText，导致「有注释的没反引号、没注释的有反引号」。这里统一。
export function normalizeTableItems(list, quoted, connStore) {
  if (!quoted) return list;
  const commentOf = {};
  connStore.allTables.forEach(t => { commentOf[t.tableName.toLowerCase()] = t.tableComment; });
  list.forEach(item => {
    if (typeof item !== 'object' || !isTableItem(item)) return;
    const plain = String(item.displayText || item.text).split(/\s{2,}/)[0].replace(/`/g, '');
    const label = quoteIdent(plain);
    const comment = commentOf[plain.toLowerCase()];
    item.displayText = comment ? `${label}  ${comment}` : label;
  });
  return list;
}

// 供模糊匹配的候选池：表名 / 「表名.」上下文下的列名 / 关键字
// poolMode: 'table' 只出表名，'keyword' 只出关键字（不出表名），'all' 两者都出
export function buildFuzzyPool(cm, search, poolMode, connStore) {
  const pool = [];
  const isQuoted = search.charAt(0) === '`';
  // 反引号可能是闭合的（光标在 `log` 之后），去掉后再拿去匹配
  const raw = search.replace(/`/g, '');
  const dot = search.lastIndexOf('.');

  if (dot !== -1) {
    // 「表名.」/「别名.」上下文：只对该表的列做模糊；前缀沿用用户输入的写法（含反引号），
    // 列名也跟着一起加反引号，保持和 addon 给出的候选同样的样式
    const tablePart = search.slice(0, dot);
    const quotedTable = tablePart.charAt(0) === '`';
    const cols = connStore.tableColumns[resolveTableName(cm, tablePart.replace(/`/g, ''), connStore)] || [];
    cols.forEach(c => {
      pool.push({
        text: tablePart + '.' + (quotedTable ? quoteIdent(c.columnName) : c.columnName),
        match: c.columnName,
        query: search.slice(dot + 1).replace(/`/g, '')
      });
    });
    return pool;
  }

  if (poolMode !== 'keyword') {
    connStore.allTables.forEach(t => {
      const label = isQuoted ? quoteIdent(t.tableName) : t.tableName;
      pool.push({
        text: label,
        displayText: t.tableComment ? `${label}  ${t.tableComment}` : label,
        className: 'CodeMirror-hint-table',
        match: t.tableName,
        query: raw
      });
    });
  }
  if (poolMode === 'table') return pool;
  // column 上下文：先把语句里已经用到的表的字段按「别名.字段 / 表名.字段」列出来
  if (poolMode === 'keyword') {
    scopedColumnRefs(cm, connStore).forEach(r => {
      pool.push({ text: `${r.prefix}.${r.columnName}`, match: r.columnName, query: raw });
    });
  }
  // 模式里的 keywords 可能是数组、空格分隔的字符串，或 {关键字: true} 映射，都要兼容
  const modeConf = cm.getModeAt(cm.getCursor()).config || {};
  const kwRaw = modeConf.keywords || CodeMirror.resolveMode('text/x-sql').keywords || sqlKeywords;
  const kwList = Array.isArray(kwRaw) ? kwRaw
    : (typeof kwRaw === 'string' ? kwRaw.split(/\s+/) : Object.keys(kwRaw || {}));
  kwList.forEach(k => {
    if (!k) return;
    pool.push({ text: k.toUpperCase(), className: 'CodeMirror-hint-keyword', match: k, query: raw });
  });
  return pool;
}

// 包装 CodeMirror 自带的 sql-hint：保留它的前缀命中（排前面），
// 再追加模糊命中的候选（排后面），去掉重复项
export function sqlHintFuzzy(cm, options, connStore) {
  const base = CodeMirror.hint.sql(cm, options) || { list: [], from: cm.getCursor(), to: cm.getCursor() };
  const search = cm.getRange(base.from, base.to);
  const quoted = search.charAt(0) === '`';
  const context = getSqlContext(cm, base.from, search);
  // LIMIT / OFFSET 这类后面是数字，没有任何可补全的
  if (context === 'none') return { list: [], from: base.from, to: base.to };
  // 语句里引用到的表还没缓存列信息时，后台补拉一次（别名补全靠的就是这份数据）
  const refs = parseTableRefs(cm.getRange({ line: 0, ch: 0 }, cm.getCursor()));
  ensureColumns(cm, Object.keys(refs).map(k => refs[k]), connStore);
  // addon 的结果也要按上下文过滤，否则表名依旧会从前缀命中里漏出来
  let list = base.list;
  if (context === 'table') list = list.filter(isTableItem);
  else if (context === 'column') list = list.filter(it => !isTableItem(it));
  normalizeTableItems(list, quoted, connStore);
  // 光标停在已有闭合反引号之前时（如 FROM `log|`），base.to 只覆盖到 `log，
  // 而补全项自带首尾反引号，旧的那个闭合反引号会残留成 `xxx``。
  // 这里把替换范围向后吃掉「紧邻的标识符字符 + 一个闭合反引号」。
  let to = base.to;
  if (quoted) {
    const line = cm.getLine(to.line) || '';
    const m = /^[\w$]*`/.exec(line.slice(to.ch));
    if (m) to = { line: to.line, ch: to.ch + m[0].length };
  }
  if (!search) {
    // 没输入片段时 column 上下文也要给「别名.字段」，否则像「ON u.id=」这种位置只剩关键字
    if (context === 'column') {
      const seenIdle = {};
      list.forEach(it => { seenIdle[typeof it === 'string' ? it : it.text] = true; });
      const idle = [];
      scopedColumnRefs(cm, connStore).forEach(r => {
        const text = `${r.prefix}.${r.columnName}`;
        if (seenIdle[text]) return;
        seenIdle[text] = true;
        idle.push({ text });
      });
      if (idle.length) return { list: idle.concat(list), from: base.from, to };
    }
    return { list, from: base.from, to };
  }
  const poolMode = context === 'table' ? 'table' : (context === 'column' ? 'keyword' : 'all');
  const seen = {};
  list.forEach(it => { seen[typeof it === 'string' ? it : it.text] = true; });
  const extra = [];
  buildFuzzyPool(cm, search, poolMode, connStore)
    .map(c => ({ c, s: fuzzyScore(c.match, c.query) }))
    // 点号后还没输入字段片段时（query 为空）fuzzyScore 返回 0，不能按 >0 过滤掉，
    // 否则「别名.」后面什么都不弹
    .filter(x => x.s > 0 || !x.c.query)
    .sort((a, b) => b.s - a.s)
    .forEach(({ c }) => {
      if (extra.length >= 50 || seen[c.text]) return;
      seen[c.text] = true;
      const item = { text: c.text };
      if (c.className) item.className = c.className;
      if (c.displayText) item.displayText = c.displayText;
      extra.push(item);
    });
  return { list: list.concat(extra), from: base.from, to };
}

// 便捷工厂：绑定 cm 与 store，返回 { showSqlHint }
export function createSqlHint(cm, connStore) {
  return {
    showSqlHint: () => showSqlHint(cm, connStore)
  };
}
