// SQL 相关纯函数：类型识别、危险语句判定、排序/筛选改写、模糊匹配、格式化
// 全部从原 dbman/public/index.html 内联脚本迁移而来，逻辑保持一致
import { format } from 'sql-formatter';

export const sqlKeywords = ['SELECT', 'INSERT', 'UPDATE', 'DELETE', 'FROM', 'WHERE', 'AND', 'OR', 'JOIN', 'LEFT', 'RIGHT', 'INNER', 'OUTER', 'ON', 'GROUP', 'BY', 'ORDER', 'LIMIT', 'OFFSET', 'CREATE', 'DROP', 'ALTER', 'TABLE', 'DATABASE', 'INDEX', 'PRIMARY', 'KEY', 'FOREIGN', 'REFERENCES', 'SET', 'VALUES', 'INTO', 'AS', 'DISTINCT', 'HAVING', 'UNION', 'ALL', 'ANY', 'SOME', 'EXISTS', 'IN', 'NOT', 'BETWEEN', 'LIKE', 'IS', 'NULL', 'TRUE', 'FALSE', 'ASC', 'DESC'];

// 模糊匹配打分：前缀 > 子串 > 顺序子序列；完全不匹配返回 -1
export function fuzzyScore(text, query) {
  const t = String(text).toLowerCase();
  const q = String(query).toLowerCase();
  if (!q) return 0;
  if (t.startsWith(q)) return 30000 - t.length;
  const sub = t.indexOf(q);
  if (sub !== -1) return 20000 - sub * 100 - t.length;
  let ti = 0, gaps = 0;
  for (let i = 0; i < q.length; i++) {
    const idx = t.indexOf(q[i], ti);
    if (idx === -1) return -1;
    gaps += idx - ti;
    ti = idx + 1;
  }
  return 10000 - gaps * 10 - t.length;
}

export function sqlIdent(name) {
  return '`' + String(name).replace(/`/g, '``') + '`';
}

export function quoteIdent(name) {
  return sqlIdent(name);
}

// 字符串字面量：转义反斜杠与单引号
export function sqlQuote(s) {
  return "'" + String(s).replace(/\\/g, '\\\\').replace(/'/g, "''") + "'";
}

// 判断某列在当前结果里是否都是数字（用于决定要不要给值加引号）
export function isNumericColumn(col, rows) {
  const data = rows || [];
  let seen = false;
  for (const row of data) {
    const v = row ? row[col] : null;
    if (v === null || v === undefined || v === '') continue;
    if (typeof v === 'number') { seen = true; continue; }
    if (typeof v === 'string' && /^-?\d+(\.\d+)?$/.test(v.trim())) { seen = true; continue; }
    return false;
  }
  return seen;
}

// 值 -> SQL 字面量：数字列且值形如数字时不加引号，否则按字符串处理
export function sqlLiteralFor(col, value, rows) {
  const v = String(value == null ? '' : value).trim();
  if (isNumericColumn(col, rows) && /^-?\d+(\.\d+)?$/.test(v)) return v;
  return sqlQuote(v);
}

// 筛选运算符定义：n 为需要的值个数（1 / 2 / 'list' / 0）
export const FILTER_OPS = [
  { k: 'eq', label: '等于 (=)', n: 1 },
  { k: 'ne', label: '不等于 (<>)', n: 1 },
  { k: 'gt', label: '大于 (>)', n: 1 },
  { k: 'ge', label: '大于等于 (>=)', n: 1 },
  { k: 'lt', label: '小于 (<)', n: 1 },
  { k: 'le', label: '小于等于 (<=)', n: 1 },
  { k: 'between', label: '范围 (BETWEEN)', n: 2 },
  { k: 'like', label: '包含 (LIKE %x%)', n: 1 },
  { k: 'nlike', label: '不包含 (NOT LIKE %x%)', n: 1 },
  { k: 'start', label: '以…开头 (LIKE x%)', n: 1 },
  { k: 'end', label: '以…结尾 (LIKE %x)', n: 1 },
  { k: 'rawlike', label: 'LIKE 匹配 (自定义 % _)', n: 1 },
  { k: 'in', label: '在列表中 (IN)', n: 'list' },
  { k: 'nin', label: '不在列表中 (NOT IN)', n: 'list' },
  { k: 'isnull', label: '为 NULL (IS NULL)', n: 0 },
  { k: 'notnull', label: '不为 NULL (IS NOT NULL)', n: 0 },
  { k: 'empty', label: "是空值 (= '')", n: 0 },
  { k: 'notempty', label: "非空值 (<> '')", n: 0 },
  { k: 'raw', label: '自定义表达式 (原文)', n: 1 }
];

export function filterOpDef(k) {
  return FILTER_OPS.find(o => o.k === k) || FILTER_OPS[0];
}

// IN / NOT IN 的取值：按中英文逗号或换行拆分
export function splitFilterList(text) {
  return String(text || '').split(/[,，\n\r]+/).map(s => s.trim()).filter(Boolean);
}

// 把「列 + 运算符 + 值」拼成一条 SQL 条件；值不完整时返回 null
export function buildCondition(row, rows) {
  const col = sqlIdent(row.col);
  const v1 = (row.v1 || '').trim();
  const v2 = (row.v2 || '').trim();
  switch (row.op) {
    case 'eq': return v1 ? `${col} = ${sqlLiteralFor(row.col, v1, rows)}` : null;
    case 'ne': return v1 ? `${col} <> ${sqlLiteralFor(row.col, v1, rows)}` : null;
    case 'gt': return v1 ? `${col} > ${sqlLiteralFor(row.col, v1, rows)}` : null;
    case 'ge': return v1 ? `${col} >= ${sqlLiteralFor(row.col, v1, rows)}` : null;
    case 'lt': return v1 ? `${col} < ${sqlLiteralFor(row.col, v1, rows)}` : null;
    case 'le': return v1 ? `${col} <= ${sqlLiteralFor(row.col, v1, rows)}` : null;
    case 'between': return (v1 && v2) ? `${col} BETWEEN ${sqlLiteralFor(row.col, v1, rows)} AND ${sqlLiteralFor(row.col, v2, rows)}` : null;
    case 'like': return v1 ? `${col} LIKE ${sqlQuote('%' + v1 + '%')}` : null;
    case 'nlike': return v1 ? `${col} NOT LIKE ${sqlQuote('%' + v1 + '%')}` : null;
    case 'start': return v1 ? `${col} LIKE ${sqlQuote(v1 + '%')}` : null;
    case 'end': return v1 ? `${col} LIKE ${sqlQuote('%' + v1)}` : null;
    case 'rawlike': return v1 ? `${col} LIKE ${sqlQuote(v1)}` : null;
    case 'in':
    case 'nin': {
      const items = splitFilterList(v1);
      if (!items.length) return null;
      const list = items.map(x => sqlLiteralFor(row.col, x, rows)).join(', ');
      return `${col} ${row.op === 'in' ? 'IN' : 'NOT IN'} (${list})`;
    }
    case 'isnull': return `${col} IS NULL`;
    case 'notnull': return `${col} IS NOT NULL`;
    case 'empty': return `${col} = ''`;
    case 'notempty': return `${col} <> ''`;
    case 'raw': return v1 ? v1 : null;
    default: return v1 ? `${col} = ${sqlLiteralFor(row.col, v1, rows)}` : null;
  }
}

// 组合全部条件：首行无连接词，其余行用各自 AND / OR 连接
export function buildWhereFromRows(rows, dataRows) {
  const parts = [];
  rows.forEach((row, i) => {
    const cond = buildCondition(row, dataRows);
    if (!cond) return;
    parts.push(i === 0 || !parts.length ? cond : `${row.join || 'AND'} ${cond}`);
  });
  return parts.length ? parts.join(' ') : '';
}

// 去掉 SQL 中的注释与字符串字面量，避免注释/字符串里的关键字造成误判
export function stripSqlLiterals(sql) {
  return String(sql)
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/--[^\n\r]*/g, ' ')
    .replace(/#[^\n\r]*/g, ' ')
    .replace(/'(?:[^'\\]|\\.|'')*'/g, "''")
    .replace(/"(?:[^"\\]|\\.|"")*"/g, '""')
    .replace(/`(?:[^`]|``)*`/g, '``');
}

// 需要二次确认的危险语句规则：命中任意一条即弹确认框
export const DANGER_RULES = [
  { label: 'DROP', re: /^drop\b/i },
  { label: 'TRUNCATE', re: /^truncate\b/i },
  { label: 'DELETE', re: /^delete\b/i },
  { label: 'UPDATE', re: /^update\b/i },
  { label: 'ALTER', re: /^alter\b/i },
  { label: 'RENAME', re: /^rename\b/i },
  { label: 'REPLACE', re: /^replace\b/i },
  { label: 'CREATE', re: /^create\b/i },
  { label: 'INSERT ... ON DUPLICATE KEY UPDATE', re: /^insert\b[\s\S]*\bon\s+duplicate\s+key\s+update\b/i },
  { label: 'LOAD DATA', re: /^load\s+(data|xml)\b/i },
  { label: 'GRANT', re: /^grant\b/i },
  { label: 'REVOKE', re: /^revoke\b/i },
  { label: 'SET GLOBAL', re: /^set\s+(@@)?(global|persist|persist_only)\b/i },
  { label: 'KILL', re: /^kill\b/i },
  { label: 'SHUTDOWN', re: /^shutdown\b/i },
  { label: 'FLUSH', re: /^flush\b/i },
  { label: 'RESET MASTER/SLAVE', re: /^reset\s+(master|slave|replica|binary\s+logs)/i },
  { label: 'PURGE BINARY LOGS', re: /^purge\b/i },
  { label: 'INSTALL/UNINSTALL PLUGIN', re: /^(install|uninstall)\s+plugin\b/i },
  { label: 'CHANGE MASTER/SOURCE', re: /^change\s+(master|replication\s+source)\b/i },
  { label: 'START/STOP SLAVE', re: /^(start|stop)\s+(slave|replica)\b/i },
  { label: 'CALL', re: /^call\b/i }
];

// 在语句顶层（跳过括号内容、字符串与反引号标识符）找出首个命中 words 的关键字位置，找不到返回 -1
export function findTopLevelKeyword(sql, words) {
  let depth = 0, quote = null;
  for (let i = 0; i < sql.length; i++) {
    const ch = sql[i];
    if (quote) {
      if (ch === '\\') { i++; continue; }
      if (ch === quote) { if (sql[i + 1] === quote) { i++; continue; } quote = null; }
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') { quote = ch; continue; }
    if (ch === '(') { depth++; continue; }
    if (ch === ')') { depth--; continue; }
    if (depth !== 0 || !/[A-Za-z_]/.test(ch)) continue;
    if (i > 0 && /[A-Za-z0-9_$]/.test(sql[i - 1])) continue;
    const m = sql.slice(i).match(/^([A-Za-z_]+)/);
    if (m && words.includes(m[1].toLowerCase())) return i;
  }
  return -1;
}

// 找出需要二次确认的危险语句类型，多语句按分号逐条判断
export function detectDangerousStatements(sql) {
  const kinds = [];
  const add = label => { if (!kinds.includes(label)) kinds.push(label); };
  const WITH_MAIN = ['select', 'insert', 'update', 'delete', 'replace', 'alter', 'drop', 'truncate', 'create', 'rename', 'call', 'load'];
  stripSqlLiterals(sql).split(';').forEach(stmt => {
    let s = stmt.trim();
    if (!s) return;
    const first = (s.match(/^([A-Za-z_]+)/) || [])[1];
    if (first && first.toLowerCase() === 'with') {
      const idx = findTopLevelKeyword(s, WITH_MAIN);
      if (idx >= 0) s = s.slice(idx);
    }
    DANGER_RULES.forEach(rule => { if (rule.re.test(s)) add(rule.label); });
  });
  return kinds;
}

// 取语句的首个关键字作为类型（WITH ... 则看正文里的 DML）
export function detectSqlType(sql) {
  const cleaned = stripSqlLiterals(sql);
  const first = cleaned.split(';').map(s => s.trim()).filter(Boolean)[0] || '';
  let kw = ((first.match(/^([A-Za-z]+)/) || [])[1] || '').toLowerCase();
  if (kw === 'with') {
    const body = first.replace(/\([^()]*\)/g, ' ').replace(/\([^()]*\)/g, ' ');
    const inner = body.match(/\b(select|insert|update|delete|replace)\b/i);
    kw = inner ? inner[1].toLowerCase() : 'with';
  }
  return kw ? kw.toUpperCase() : 'SQL';
}

export function sqlLogClass(type) {
  if (['SELECT', 'SHOW', 'DESC', 'DESCRIBE', 'EXPLAIN', 'WITH'].includes(type)) return 'select';
  if (['INSERT', 'UPDATE', 'DELETE', 'REPLACE'].includes(type)) return 'write';
  return 'ddl';
}

// 把 SQL 按顶层关键字切成 body / where / group / having / order / limit
export function splitTopLevelClauses(sql) {
  const s = String(sql || '').trim().replace(/;+\s*$/, '');
  if (!s) return null;
  const keys = [
    { name: 'where', re: /^where\b/i },
    { name: 'group', re: /^group\s+by\b/i },
    { name: 'having', re: /^having\b/i },
    { name: 'order', re: /^order\s+by\b/i },
    { name: 'limit', re: /^limit\b/i }
  ];
  const marks = [];
  let depth = 0, quote = null;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (quote) {
      if (ch === '\\') { i++; continue; }
      if (ch === quote) { if (s[i + 1] === quote) { i++; continue; } quote = null; }
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') { quote = ch; continue; }
    if (ch === '(') { depth++; continue; }
    if (ch === ')') { depth--; continue; }
    if (depth !== 0 || !/[a-z]/i.test(ch)) continue;
    if (i > 0 && /[A-Za-z0-9_$]/.test(s[i - 1])) continue;
    const rest = s.slice(i);
    for (const key of keys) {
      const m = rest.match(key.re);
      if (!m) continue;
      if (marks.some(x => x.name === key.name)) continue;
      marks.push({ name: key.name, start: i, kwEnd: i + m[0].length });
      break;
    }
  }
  marks.sort((a, b) => a.start - b.start);
  const out = { body: marks.length ? s.slice(0, marks[0].start).trim() : s, where: null, group: null, having: null, order: null, limit: null };
  marks.forEach((mk, idx) => {
    const end = idx + 1 < marks.length ? marks[idx + 1].start : s.length;
    const text = s.slice(mk.kwEnd, end).trim();
    out[mk.name] = text || null;
  });
  return out;
}

// 按固定顺序拼回 SELECT 语句
export function rebuildSql(parsed) {
  let out = parsed.body;
  if (parsed.where) out += ' WHERE ' + parsed.where;
  if (parsed.group) out += ' GROUP BY ' + parsed.group;
  if (parsed.having) out += ' HAVING ' + parsed.having;
  if (parsed.order) out += ' ORDER BY ' + parsed.order;
  if (parsed.limit) out += ' LIMIT ' + parsed.limit;
  return out + ';';
}

// ===== 反向解析：把编辑器 SQL 里已有的 WHERE 带入筛选弹窗 =====

// SQL 标识符：可带库/表限定、可带反引号；整体只占一个捕获组（后续 m[2]/m[3] 才是值）
const F_IDENT = '((?:`(?:[^`]|``)+`|[A-Za-z_$][\\w$]*)(?:\\s*\\.\\s*(?:`(?:[^`]|``)+`|[A-Za-z_$][\\w$]*))?)';

export function unquoteSqlValue(raw) {
  const t = String(raw == null ? '' : raw).trim();
  const q = t[0];
  if (t.length >= 2 && (q === "'" || q === '"') && t[t.length - 1] === q) {
    return t.slice(1, -1).split(q + q).join(q).replace(/\\(.)/g, '$1');
  }
  return t;
}

export function unquoteSqlIdent(raw) {
  const parts = String(raw == null ? '' : raw).trim().split('.');
  let t = parts[parts.length - 1].trim();
  if (t.length >= 2 && t[0] === '`' && t[t.length - 1] === '`') t = t.slice(1, -1).split('``').join('`');
  return t;
}

// 顶层（不在括号/字符串内）按 AND / OR 切分；BETWEEN 里的 AND 不算分隔符
export function splitTopLevelAndOr(text) {
  const s = String(text || '').trim();
  const out = [];
  let depth = 0, quote = null, segStart = 0, join = 'AND', betweenOpen = 0;
  const flushTo = end => {
    const t = s.slice(segStart, end).trim();
    if (t) out.push({ text: t, join });
  };
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (quote) {
      if (ch === '\\') { i++; continue; }
      if (ch === quote) { if (s[i + 1] === quote) { i++; continue; } quote = null; }
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') { quote = ch; continue; }
    if (ch === '(') { depth++; continue; }
    if (ch === ')') { depth--; continue; }
    if (depth !== 0 || !/[A-Za-z]/.test(ch)) continue;
    if (i > 0 && /[A-Za-z0-9_$]/.test(s[i - 1])) continue;
    const m = s.slice(i).match(/^[A-Za-z_]+/);
    if (!m) continue;
    const word = m[0].toUpperCase();
    const adv = m[0].length;
    if (word === 'BETWEEN') betweenOpen++;
    else if (word === 'AND' && betweenOpen > 0) betweenOpen--;
    else if (word === 'AND' || word === 'OR') {
      flushTo(i);
      join = word;
      segStart = i + adv;
    }
    i += adv - 1;
  }
  flushTo(s.length);
  return out;
}

// IN 列表按顶层逗号拆分（引号内的逗号不拆）
export function splitSqlList(text) {
  const s = String(text || '');
  const out = [];
  let buf = '', quote = null;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (quote) {
      buf += ch;
      if (ch === '\\') { if (i + 1 < s.length) buf += s[++i]; continue; }
      if (ch === quote) { if (s[i + 1] === quote) { buf += s[++i]; continue; } quote = null; }
      continue;
    }
    if (ch === "'" || ch === '"') { quote = ch; buf += ch; continue; }
    if (ch === ',') { out.push(buf); buf = ''; continue; }
    buf += ch;
  }
  out.push(buf);
  return out.map(x => unquoteSqlValue(x)).filter(x => x !== '');
}

// LIKE 值按 % 位置还原成具体运算符
export function likeOpAndValue(v) {
  const s = String(v);
  const head = s.startsWith('%'), tail = s.endsWith('%');
  if (head && tail && s.length > 2) return { op: 'like', v1: s.slice(1, -1) };
  if (!head && tail && s.length > 1) return { op: 'start', v1: s.slice(0, -1) };
  if (head && !tail && s.length > 1) return { op: 'end', v1: s.slice(1) };
  return { op: 'rawlike', v1: s };
}

// 只剥掉真正包住整条条件的最外层括号（不能误吃 IN (...) 的右括号）
export function stripWrappingParens(text) {
  let s = String(text || '').trim();
  while (s.length >= 2 && s[0] === '(' && s[s.length - 1] === ')') {
    let depth = 0, quote = null, enclosed = true;
    for (let i = 0; i < s.length; i++) {
      const ch = s[i];
      if (quote) {
        if (ch === '\\') { i++; continue; }
        if (ch === quote) { if (s[i + 1] === quote) { i++; continue; } quote = null; }
        continue;
      }
      if (ch === "'" || ch === '"' || ch === '`') { quote = ch; continue; }
      if (ch === '(') depth++;
      else if (ch === ')') {
        depth--;
        if (depth === 0 && i < s.length - 1) { enclosed = false; break; }
      }
    }
    if (!enclosed) break;
    const inner = s.slice(1, -1).trim();
    if (splitTopLevelAndOr(inner).length > 1) break;
    s = inner;
  }
  return s;
}

// 单条条件 → 筛选行；认不出来的一律降级为「自定义表达式」并保留原文
export function parseConditionPart(text) {
  const t = stripWrappingParens(text);
  if (!t) return null;
  const asRaw = () => ({ col: '', op: 'raw', v1: t, v2: '', join: 'AND' });
  const pats = [
    { op: 'notnull', re: new RegExp('^' + F_IDENT + '\\s+is\\s+not\\s+null$', 'i') },
    { op: 'isnull', re: new RegExp('^' + F_IDENT + '\\s+is\\s+null$', 'i') },
    { op: 'nin', re: new RegExp('^' + F_IDENT + '\\s+not\\s+in\\s*\\(([\\s\\S]*)\\)$', 'i') },
    { op: 'in', re: new RegExp('^' + F_IDENT + '\\s+in\\s*\\(([\\s\\S]*)\\)$', 'i') },
    { op: 'between', re: new RegExp('^' + F_IDENT + '\\s+not\\s+between\\b.*$', 'i'), skip: true },
    { op: 'between', re: new RegExp('^' + F_IDENT + '\\s+between\\s+(.+?)\\s+and\\s+([\\s\\S]+)$', 'i') },
    { op: 'nlike', re: new RegExp('^' + F_IDENT + '\\s+not\\s+like\\s+([\\s\\S]+)$', 'i') },
    { op: 'rawlike', re: new RegExp('^' + F_IDENT + '\\s+like\\s+([\\s\\S]+)$', 'i') },
    { op: 'ge', re: new RegExp('^' + F_IDENT + '\\s*>=\\s*([\\s\\S]+)$', 'i') },
    { op: 'le', re: new RegExp('^' + F_IDENT + '\\s*<=\\s*([\\s\\S]+)$', 'i') },
    { op: 'ne', re: new RegExp('^' + F_IDENT + '\\s*(?:<>|!=)\\s*([\\s\\S]+)$', 'i') },
    { op: 'gt', re: new RegExp('^' + F_IDENT + '\\s*>\\s*([\\s\\S]+)$', 'i') },
    { op: 'lt', re: new RegExp('^' + F_IDENT + '\\s*<\\s*([\\s\\S]+)$', 'i') },
    { op: 'eq', re: new RegExp('^' + F_IDENT + '\\s*=\\s*([\\s\\S]+)$', 'i') }
  ];
  for (const p of pats) {
    const m = t.match(p.re);
    if (!m) continue;
    if (p.skip) return asRaw();
    const col = unquoteSqlIdent(m[1]);
    if (!col) continue;
    const row = { col, op: p.op, v1: '', v2: '', join: 'AND' };
    if (p.op === 'isnull' || p.op === 'notnull') return row;
    if (p.op === 'between') {
      row.v1 = unquoteSqlValue(m[2]);
      row.v2 = unquoteSqlValue(m[3]);
      return row;
    }
    if (p.op === 'in' || p.op === 'nin') {
      const items = splitSqlList(m[2]);
      if (!items.length || items.some(x => /[,，\n\r]/.test(x))) return asRaw();
      row.v1 = items.join(',');
      return row;
    }
    const val = unquoteSqlValue(m[2]);
    if (p.op === 'rawlike') return { ...row, ...likeOpAndValue(val) };
    if (p.op === 'eq' && val === '') return { ...row, op: 'empty' };
    if (p.op === 'ne' && val === '') return { ...row, op: 'notempty' };
    row.v1 = val;
    return row;
  }
  return asRaw();
}

export function parseWhereToRows(whereText) {
  const rows = [];
  splitTopLevelAndOr(whereText).forEach(p => {
    const row = parseConditionPart(p.text);
    if (!row) return;
    row.join = p.join;
    rows.push(row);
  });
  return rows;
}

// SQL 美化：交给 sql-formatter（按 MySQL 方言），关键字转大写、每列一行、子句缩进两格
// 解析不了的语句会抛错，调用方需保留原文，避免把用户写的 SQL 弄丢
export function formatSql(sql) {
  return format(String(sql), {
    language: 'mysql',
    keywordCase: 'upper',
    tabWidth: 2,
    linesBetweenQueries: 2
  });
}

// ===== 只读 SQL 语法着色：弹窗里展示 SQL 时用，配色由 theme.css 的 --sql-* 变量决定 =====

// 参与着色的 MySQL 关键字 / 类型 / 表选项
const HIGHLIGHT_KEYWORDS = new Set(`ADD ALL ALTER AND AS ASC AUTO_INCREMENT BEGIN BETWEEN BIGINT BINARY BLOB BOOLEAN BOTH BY CASCADE CASE CHANGE CHAR CHARACTER CHARSET CHECK COLLATE COLLATION COLUMN COMMENT COMMIT CONSTRAINT CONVERT CREATE CROSS CURRENT_DATE CURRENT_TIME CURRENT_TIMESTAMP DATABASE DATABASES DATE DATETIME DECIMAL DEFAULT DELAYED DELETE DESC DESCRIBE DISTINCT DIV DO DOUBLE DROP DUAL EACH ELSE ELSEIF ENUM ESCAPE EXISTS EXPLAIN FALSE FETCH FLOAT FOR FORCE FOREIGN FROM FULL FUNCTION GRANT GROUP HAVING HIGH_PRIORITY IF IGNORE IN INDEX INFILE INNER INSERT INT INTEGER INTERVAL INTO IS JOIN KEY KEYS KILL LEADING LEFT LIKE LIMIT LINES LOAD LOCALTIME LOCALTIMESTAMP LOCK LONG LONGBLOB LONGTEXT LOW_PRIORITY MATCH MAXVALUE MEDIUMBLOB MEDIUMINT MEDIUMTEXT MODIFIES NATURAL NOT NULL NUMERIC OFFSET ON OPTIMIZE OPTION OR ORDER OUTER OUTFILE PRIMARY PROCEDURE RANGE READ REAL REFERENCES REGEXP RENAME REPEAT REPLACE REQUIRE RESTRICT RETURN REVOKE RIGHT RLIKE ROLLBACK ROW ROWS SELECT SET SHOW SMALLINT SPATIAL SQL SSL STARTING STRAIGHT_JOIN TABLE TABLES TEMPORARY TERMINATED TEXT THEN TIME TIMESTAMP TINYBLOB TINYINT TINYTEXT TO TRAILING TRIGGER TRUE TRUNCATE UNIQUE UNSIGNED UPDATE USAGE USE USING VALUES VARBINARY VARCHAR VIEW WHEN WHERE WHILE WITH WRITE XOR YEAR ZEROFILL`.split(' '));

function escapeForHtml(s) {
  return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// 在已转义的片段里再套一层检索命中（log-hit）；关键词含 & < > 时直接跳过，避免破坏结构
function wrapHit(escaped, kw) {
  if (!kw || /[&<>]/.test(kw)) return escaped;
  const lower = escaped.toLowerCase();
  const needle = kw.toLowerCase();
  let out = '';
  let pos = 0;
  let i = lower.indexOf(needle, pos);
  while (i !== -1) {
    out += escaped.slice(pos, i) + '<span class="log-hit">' + escaped.slice(i, i + needle.length) + '</span>';
    pos = i + needle.length;
    i = lower.indexOf(needle, pos);
  }
  return out + escaped.slice(pos);
}

// SQL -> 带 class 的 HTML。searchKeyword 可选，用于日志/收藏弹窗里同时保留检索命中高亮
export function highlightSqlSyntax(sql, searchKeyword) {
  const src = String(sql == null ? '' : sql);
  const kw = searchKeyword ? String(searchKeyword) : '';
  const out = [];
  const plain = t => wrapHit(escapeForHtml(t), kw);
  const token = (cls, t) => '<span class="' + cls + '">' + wrapHit(escapeForHtml(t), kw) + '</span>';
  let i = 0;
  while (i < src.length) {
    const ch = src[i];
    const two = src.slice(i, i + 2);
    if (two === '/*') {
      const end = src.indexOf('*/', i + 2);
      const stop = end === -1 ? src.length : end + 2;
      out.push(token('sql-comment', src.slice(i, stop)));
      i = stop;
    } else if (two === '--' || ch === '#') {
      let stop = i;
      while (stop < src.length && src[stop] !== '\n' && src[stop] !== '\r') stop++;
      out.push(token('sql-comment', src.slice(i, stop)));
      i = stop;
    } else if (ch === "'" || ch === '"' || ch === '`') {
      // 字符串 / 反引号标识符：'' 与 `` 是转义写法，反斜杠转义只在字符串里生效
      let j = i + 1;
      while (j < src.length) {
        if (src[j] === '\\' && ch !== '`') { j += 2; continue; }
        if (src[j] === ch) {
          j++;
          if (src[j] === ch) { j++; continue; }
          break;
        }
        j++;
      }
      out.push(token(ch === '`' ? 'sql-ident' : 'sql-str', src.slice(i, j)));
      i = j;
    } else if (/[0-9]/.test(ch) || (ch === '.' && /[0-9]/.test(src[i + 1] || ''))) {
      const m = src.slice(i).match(/^(?:0x[0-9a-fA-F]+|(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?)/);
      out.push(token('sql-num', m[0]));
      i += m[0].length;
    } else if (/[A-Za-z_$@]/.test(ch)) {
      const m = src.slice(i).match(/^[A-Za-z_$@][\w$@]*/);
      out.push(HIGHLIGHT_KEYWORDS.has(m[0].toUpperCase()) ? token('sql-kw', m[0]) : plain(m[0]));
      i += m[0].length;
    } else {
      out.push(plain(ch));
      i++;
    }
  }
  return out.join('');
}
