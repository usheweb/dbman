// SQL 补全引擎：从原 dbman/public/index.html 内联脚本迁移而来，逻辑保持一致
// 触发时实时读取连接 store 的表 / 列信息，避免用编辑器创建时的快照
import CodeMirror from 'codemirror';
import 'codemirror/mode/sql/sql.js';
import 'codemirror/addon/hint/show-hint.js';
import 'codemirror/addon/hint/sql-hint.js';
import { fuzzyScore, sqlKeywords, quoteIdent } from '@/utils/sql';
// 历史 SQL 联想要用到当前连接的执行历史（workspace 不反向依赖本文件，不会成环）
import { useWorkspaceStore } from '@/stores/workspace';

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

// 语句骨架模板：在语句开头敲一个短触发词就带出整段结构（s → SELECT * FROM ）。
// body 里的 $0 标出补全后光标要落的位置（写表名 / 字段的地方），省去再手动挪光标。
// keys 是触发词，从短到长都算命中（敲 s、se、select 都能出「SELECT * FROM 」）
const SQL_SNIPPETS = [
  { keys: ['s'], label: 'SELECT * FROM …', body: 'SELECT * FROM $0' },
  { keys: ['sw'], label: 'SELECT * FROM … WHERE …', body: 'SELECT * FROM $0 WHERE ' },
  { keys: ['sc'], label: 'SELECT COUNT(*) FROM …', body: 'SELECT COUNT(*) FROM $0' },
  { keys: ['ins'], label: 'INSERT INTO … VALUES (…)', body: 'INSERT INTO $0 () VALUES ()' },
  { keys: ['upd'], label: 'UPDATE … SET … WHERE …', body: 'UPDATE $0 SET  WHERE ' },
  { keys: ['del'], label: 'DELETE FROM … WHERE …', body: 'DELETE FROM $0 WHERE ' },
  { keys: ['ct'], label: 'CREATE TABLE …', body: 'CREATE TABLE $0 (\n  \n)' },
  { keys: ['at'], label: 'ALTER TABLE …', body: 'ALTER TABLE $0 ' },
  { keys: ['dt'], label: 'DROP TABLE …', body: 'DROP TABLE $0' }
];

// 常用函数模板：带括号骨架、光标落在第一个参数位，只在表达式位置（列上下文）出现，
// 语句开头已经有整句骨架了，再混函数会很杂乱
const SQL_FUNCTIONS = [
  { keys: ['now'], label: 'NOW()', body: 'NOW()' },
  { keys: ['cnt', 'count'], label: 'COUNT(…)', body: 'COUNT($0)' },
  { keys: ['sum'], label: 'SUM(…)', body: 'SUM($0)' },
  { keys: ['avg'], label: 'AVG(…)', body: 'AVG($0)' },
  { keys: ['max'], label: 'MAX(…)', body: 'MAX($0)' },
  { keys: ['min'], label: 'MIN(…)', body: 'MIN($0)' },
  { keys: ['concat'], label: 'CONCAT(…, …)', body: 'CONCAT($0, )' },
  { keys: ['ifnull'], label: 'IFNULL(…, …)', body: 'IFNULL($0, )' },
  { keys: ['coalesce'], label: 'COALESCE(…, …)', body: 'COALESCE($0, )' },
  { keys: ['substr'], label: 'SUBSTRING(…, …)', body: 'SUBSTRING($0, )' },
  { keys: ['datefmt'], label: "DATE_FORMAT(…, '%Y-%m-%d')", body: "DATE_FORMAT($0, '%Y-%m-%d')" },
  { keys: ['gcd'], label: 'GROUP_CONCAT(…)', body: 'GROUP_CONCAT($0)' },
  { keys: ['case'], label: 'CASE WHEN … THEN … END', body: 'CASE WHEN $0 THEN  ELSE  END' }
];

// 写在列后面的运算符模板：敲 is 就出「IS NULL / IS NOT NULL」，省得再手打一遍空格和 NULL
const SQL_OPERATORS = [
  { keys: ['is'], label: 'IS NULL', body: 'IS NULL' },
  { keys: ['isn', 'isnot'], label: 'IS NOT NULL', body: 'IS NOT NULL' }
];

// 触发词命中：敲 s / se / select 都算命中触发词 select（短的打短、长的打长）
function matchTrigger(keys, q) {
  return keys.some(k => k.startsWith(q) || q.startsWith(k));
}

// FROM / JOIN 后面选中表名时顺手补的别名：去掉 t_ / tb_ / sys_ 这类前缀后，
// 取每个下划线段的首字母（admin_menu → am）；已被占用或推不出合法别名就原样插入
function aliasFor(table, taken) {
  const base = String(table).replace(/^(?:t|tb|tbl|sys|app|biz|dim|fact)_/i, '');
  const parts = base.split(/[_\d]+/).filter(Boolean);
  let alias = parts.map(p => p.charAt(0).toLowerCase()).join('') || base.charAt(0).toLowerCase();
  if (!/^[a-z]/.test(alias)) return '';
  if (taken[alias]) {
    let i = 2;
    while (taken[alias + i]) i++;
    alias += i;
  }
  return alias;
}

// 给表名候选套一层「选中后追加别名」的插入动作；不带别名时保持原样
function withAliasPick(item, alias) {
  const base = typeof item === 'string' ? { text: item } : item;
  if (!alias) return base;
  return {
    ...base,
    hint: (cm, data, com) => {
      const from = com.from || data.from;
      const to = com.to || data.to;
      cm.replaceRange(String(com.text || '') + ' ' + alias, from, to, 'complete');
    }
  };
}

// 光标前最近一个「表名类」关键字：只有 FROM / JOIN 后面才补别名，
// INTO / UPDATE / TABLE / TRUNCATE 后面补上反而多余
function tableClauseBefore(cm, pos) {
  const before = cm.getRange({ line: 0, ch: 0 }, pos);
  const seg = before.slice(before.lastIndexOf(';') + 1);
  const words = seg.toUpperCase().match(/[A-Z_][A-Z0-9_]*/g) || [];
  for (let i = words.length - 1; i >= 0; i--) {
    if (TABLE_KEYWORDS.indexOf(words[i]) !== -1) return words[i];
  }
  return '';
}

// 日期/时间列的取值模板：写完「时间列 > 」直接挑「近 7 天」这类现成写法
const DATE_TYPES = ['date', 'datetime', 'timestamp', 'time', 'datetime2', 'smalldatetime', 'timestamptz'];
const DATE_VALUES = [
  ['NOW()', '当前时间'],
  ['CURDATE()', '今天'],
  ['NOW() - INTERVAL 1 DAY', '近 1 天'],
  ['NOW() - INTERVAL 7 DAY', '近 7 天'],
  ['NOW() - INTERVAL 30 DAY', '近 30 天']
];

// 在语句引用到的表里找这一列，拿到它的类型与注释
function findColumnMeta(cm, colRef, connStore) {
  const refs = parseTableRefs(cm.getRange({ line: 0, ch: 0 }, cm.getCursor()));
  const keys = Object.keys(connStore.tableColumns);
  const dot = String(colRef).lastIndexOf('.');
  const wanted = (dot === -1 ? String(colRef) : String(colRef).slice(dot + 1)).toLowerCase();
  const onlyTable = dot === -1 ? '' : refs[String(colRef).slice(0, dot).toLowerCase()];
  const tables = onlyTable ? [onlyTable] : Object.keys(refs).map(k => refs[k]);
  for (const t of tables) {
    const realKey = keys.find(k => k.toLowerCase() === String(t).toLowerCase());
    if (!realKey) continue;
    const col = (connStore.tableColumns[realKey] || []).find(c => String(c.columnName).toLowerCase() === wanted);
    if (col) return col;
  }
  return null;
}

// 光标左边正好停在「列 比较符 」之后时，给这一列的取值模板（按类型 / 注释推导）
function buildValueItems(cm, pos, search, connStore) {
  if (search && (search.indexOf('.') !== -1 || search.charAt(0) === '`')) return [];
  const before = cm.getRange({ line: 0, ch: 0 }, pos);
  const seg = before.slice(before.lastIndexOf(';') + 1);
  const m = /([`\w]+(?:\.[`\w]+)?)\s*(?:=|<>|!=|>=|<=|>|<|\bLIKE)\s*$/i.exec(seg);
  if (!m) return [];
  const meta = findColumnMeta(cm, m[1].replace(/`/g, ''), connStore);
  if (!meta) return [];
  const q = String(search || '').toLowerCase();
  const items = [];
  const push = (text, show, label, cls) => {
    const s = fuzzyScore(show, q);
    if (q && s <= 0) return;
    const item = { text, displayText: show + (label ? '  ' + label : ''), _score: q ? s : 0 };
    if (cls) item.className = cls;
    items.push(item);
  };
  if (DATE_TYPES.indexOf(String(meta.dataType || '').toLowerCase()) !== -1) {
    DATE_VALUES.forEach(([v, l]) => push(v, v, l, 'CodeMirror-hint-snippet'));
  }
  // 注释里常写「1=正常 2=禁用」，据此给出可选值，省得再去翻文档
  const comment = String(meta.columnComment || '');
  const re = /([A-Za-z0-9_'-]+)\s*[:=]\s*([^\s,，;；、]+)/g;
  let mm;
  const seenVal = {};
  while ((mm = re.exec(comment))) {
    const raw = mm[1].replace(/^['"]|['"]$/g, '');
    if (!raw || seenVal[raw]) continue;
    seenVal[raw] = true;
    // 数字原样插入，其余按字符串加引号，避免拼出语法错误的 SQL
    const text = /^-?\d+(\.\d+)?$/.test(raw) ? raw : "'" + raw.replace(/'/g, "''") + "'";
    push(text, raw, mm[2], 'CodeMirror-hint-snippet');
  }
  items.sort((a, b) => b._score - a._score);
  items.forEach(it => { delete it._score; });
  return items;
}

// 模板候选（常用函数 / 运算符）：列上下文且正在输入一个完整词时按触发词匹配
function buildTemplateItems(context, search, defs) {
  if (!search || context !== 'column') return [];
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(search)) return [];
  const q = search.toLowerCase();
  const items = [];
  defs.forEach(f => {
    if (!matchTrigger(f.keys, q)) return;
    items.push({
      text: f.body.replace('$0', ''),
      displayText: f.label,
      className: 'CodeMirror-hint-snippet',
      hint: snippetHint(f.body, null)
    });
  });
  return items;
}

// 历史 SQL 联想：语句开头输入时，把当前库跑过的语句按最近优先列出来，选中即整段插入
function buildHistoryItems(cm, pos, search, connStore) {
  if (!search || search.indexOf('.') !== -1 || search.charAt(0) === '`') return [];
  if (!atStatementStart(cm, pos)) return [];
  const q = search.toLowerCase();
  // 只敲一个字母时几乎每条历史都能 includes 命中，会挤掉真正有用的候选，至少两个字才联想
  if (q.length < 2) return [];
  const db = connStore.currentDatabase || '';
  const items = [];
  (useWorkspaceStore().historyList || []).forEach(e => {
    if (!e || !e.sql || items.length >= 5) return;
    if (db && e.db && e.db !== db) return;
    const one = String(e.sql).replace(/\s+/g, ' ').trim();
    if (!one.toLowerCase().includes(q)) return;
    items.push({
      text: e.sql,
      displayText: '⟲ ' + (one.length > 60 ? one.slice(0, 60) + '…' : one),
      className: 'CodeMirror-hint-snippet'
    });
  });
  return items;
}

// 当前光标是否处在「一条语句还没开始写」的位置：只看到上一个分号为止，
// 去掉正在输入的那半截词后全是空白就说明还没写内容（多语句脚本的第二条也算）
export function atStatementStart(cm, pos) {
  const before = cm.getRange({ line: 0, ch: 0 }, pos);
  const seg = before.slice(before.lastIndexOf(';') + 1);
  return !/\S/.test(seg.replace(/\s*[`\w.]*$/, ''));
}

// 选中模板后的落地动作：把 [from, to] 换成模板正文，再把光标放到 $0 标出的位置。
// 骨架末尾就是表名的位置时（如 SELECT * FROM ）顺手把表名下拉弹出来，接着敲就能挑表
function snippetHint(body, connStore) {
  return (cm, data, com) => {
    const from = com.from || data.from;
    const to = com.to || data.to;
    const at = body.indexOf('$0');
    const text = body.replace('$0', '');
    cm.replaceRange(text, from, to, 'complete');
    let cursor = from;
    if (at !== -1) {
      const head = text.slice(0, at);
      const nl = head.lastIndexOf('\n');
      cursor = nl === -1
        ? { line: from.line, ch: from.ch + head.length }
        : { line: from.line + head.split('\n').length - 1, ch: head.length - nl - 1 };
      cm.setCursor(cursor);
    }
    // 光标停在刚插入内容的末尾，说明接下来就该写表名了，直接给出表名候选。
    // 函数模板没有 connStore（它就是一段表达式，不需要表名），跳过这次自动弹出
    if (connStore && (at === -1 || at === text.length)) {
      setTimeout(() => showSqlHint(cm, connStore), 0);
    }
  };
}

// 语句开头才给模板：WHERE / ON 这些位置冒出「SELECT * FROM 」只会干扰；
// 反引号或「表名.」上下文是在写标识符，也不该混进模板
function buildSnippetItems(cm, pos, search, connStore) {
  if (!search || search.charAt(0) === '`' || search.indexOf('.') !== -1) return [];
  if (!atStatementStart(cm, pos)) return [];
  const q = search.toLowerCase();
  const items = [];
  SQL_SNIPPETS.forEach(s => {
    if (!s.keys.some(k => k.startsWith(q) || q.startsWith(k))) return;
    items.push({
      text: s.body.replace('$0', ''),
      displayText: s.label,
      className: 'CodeMirror-hint-snippet',
      hint: snippetHint(s.body, connStore)
    });
  });
  return items;
}

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
  // 下拉里每一项都带序号（1. 2. 3. …），直接敲数字就选中对应项。
  // 这些数字键注册在 show-hint 自己的 keyMap 上，只在下拉打开时生效，不会落进正文。
  // 不选中时必须返回 CodeMirror.Pass 把按键让回给编辑器（返回 false 等于「吃掉但什么都不做」，
  // 数字就打不进去了）；序号对不上的数字同样放行
  const digitKeys = {};
  '123456789'.split('').forEach((d, i) => {
    digitKeys[d] = (c, handle) => {
      // 手填值的位置（如 where clicks=18）不能拦数字，序号本来也不显示
      if (handle.data && handle.data.valuePos) return CodeMirror.Pass;
      const list = (handle.data && handle.data.list) || [];
      if (i >= list.length) return CodeMirror.Pass;
      handle.setFocus(i);
      handle.pick();
    };
  });
  cm.showHint({
    completeSingle: false,
    tables: buildSqlHintTables(connStore),
    hint: (cm2, options) => sqlHintFuzzy(cm2, options, connStore),
    extraKeys: digitKeys
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
      // 带上类型与注释：补全项要显示「列名  类型/注释」，光有列名不知道是什么字段
      out.push({ prefix: prefix, columnName: c.columnName, dataType: c.dataType, columnComment: c.columnComment });
    });
  });
  return out;
}

// 列补全项的展示文案：列名后跟类型，有注释再接上注释
function columnDisplay(columnName, dataType, comment) {
  let s = columnName;
  if (dataType) s += '  ' + dataType;
  if (comment) s += '  ' + comment;
  return s;
}

// 当前 SQL 模式的关键字集：用来把候选分成「标识符」与「关键字」两组，
// 排序时标识符（表名 / 字段名）永远排在关键字前面
function sqlKeywordSet(cm) {
  const conf = cm.getModeAt(cm.getCursor()).config || {};
  const raw = conf.keywords || CodeMirror.resolveMode('text/x-sql').keywords || sqlKeywords;
  const list = Array.isArray(raw) ? raw
    : (typeof raw === 'string' ? raw.split(/\s+/) : Object.keys(raw || {}));
  const set = {};
  list.forEach(k => { if (k) set[String(k).toUpperCase()] = true; });
  return set;
}

// 把候选分成三组：ids=表名/字段名，tpl=模板类（函数 / 取值 / 历史），kws=关键字。
// 写 SQL 时最常补的就是标识符，所以要排在关键字前面，不然得翻过一堆关键字才找到表 / 字段
function groupCandidates(cm, items) {
  const kwSet = sqlKeywordSet(cm);
  const ids = [], tpl = [], kws = [];
  items.forEach(it => {
    const cls = String((typeof it === 'object' && it && it.className) || '');
    const plain = String((typeof it === 'string' ? it : it.text) || '').replace(/`/g, '');
    if (cls.indexOf('CodeMirror-hint-table') !== -1) ids.push(it);
    else if (cls.indexOf('CodeMirror-hint-snippet') !== -1) tpl.push(it);
    else if (cls.indexOf('CodeMirror-hint-keyword') !== -1) kws.push(it);
    else if (kwSet[plain.toUpperCase()]) kws.push(it);
    else ids.push(it);
  });
  return { ids, tpl, kws };
}

// 下拉项分色渲染：字段名 / 类型 / 注释各占一段，名字跟说明一眼就能分出来。
// show-hint 会把候选的 render(elt, data, item) 当成渲染函数（见 addon 的 Widget 构造）
function hintSpan(cls, text) {
  const span = document.createElement('span');
  span.className = cls;
  span.textContent = text;
  return span;
}

function hintRender(elt, data, item) {
  const p = (item && item._parts) || {};
  if (item && item._num) elt.appendChild(hintSpan('hint-num', item._num));
  if (p.name) elt.appendChild(hintSpan('hint-name', p.name));
  if (p.type) elt.appendChild(hintSpan('hint-type', p.type));
  if (p.comment) elt.appendChild(hintSpan('hint-comment', p.comment));
}

// 给候选加序号前缀（1. 2. 3. …）：只在展示层，插入正文的仍是原文；
// 配合「下拉打开时敲数字直接选中对应项」。手填值的位置不加序号（数字要原样输入）
function numberize(list, enabled) {
  return list.map((it, i) => {
    const base = typeof it === 'string' ? { text: it } : it;
    const next = { ...base };
    if (enabled) {
      next._num = (i + 1) + '.';
      // 分色显示的项由 render 自己画序号；其余项把序号拼进展示文案
      if (!next._parts) next.displayText = next._num + ' ' + String(base.displayText || base.text);
    }
    if (next._parts) next.render = hintRender;
    return next;
  });
}

// 光标是否正停在「列 比较符 」之后：这里用户在手动填值（如 age > 18），
// 数字得原样输入，不能被下拉的序号选中拦走
export function inValuePosition(cm, pos) {
  const before = cm.getRange({ line: 0, ch: 0 }, pos);
  const seg = before.slice(before.lastIndexOf(';') + 1);
  return /(?:=|<>|!=|>=|<=|>|<|\bLIKE)\s*$/i.test(seg);
}

// 语句只引用了一张表、且这张表没起别名时，列名前缀毫无意义（不会有歧义），
// 补全就不该带表名：敲 `where good` 期望拿到 goods_code，而不是 t.goods_code。
// 多表 / 有别名时前缀是区分列归属用的，必须保留。
export function bareColumnSingleTable(cm, connStore) {
  const refs = parseTableRefs(cm.getRange({ line: 0, ch: 0 }, cm.getCursor()));
  const keys = Object.keys(refs);
  return keys.length === 1 && keys[0] === refs[keys[0]].toLowerCase();
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
      const text = tablePart + '.' + (quotedTable ? quoteIdent(c.columnName) : c.columnName);
      pool.push({
        text,
        displayText: columnDisplay(text, c.dataType, c.columnComment),
        // 分色渲染用：字段名 / 类型 / 注释各占一段
        _parts: { name: text, type: c.dataType || '', comment: c.columnComment || '' },
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
        // 分色渲染用：表名青、注释暗灰，一眼分出哪个是名字
        _parts: { name: label, type: '', comment: t.tableComment || '' },
        // 注释一起进匹配：输中文「订单明细」也能找到表，不必记住英文表名
        match: t.tableComment ? t.tableName + ' ' + t.tableComment : t.tableName,
        query: raw
      });
    });
  }
  if (poolMode === 'table') return pool;
  // column 上下文：先把语句里已经用到的表的字段列出来（单表不加前缀，见 bareColumnSingleTable）
  if (poolMode === 'keyword') {
    const bare = bareColumnSingleTable(cm, connStore);
    scopedColumnRefs(cm, connStore).forEach(r => {
      const text = bare ? r.columnName : `${r.prefix}.${r.columnName}`;
      pool.push({
        text,
        displayText: columnDisplay(text, r.dataType, r.columnComment),
        // 分色渲染用
        _parts: { name: text, type: r.dataType || '', comment: r.columnComment || '' },
        match: r.columnName,
        query: raw
      });
    });
  }
  // 关键字候选：当前 SQL 模式的关键字（含 addon 默认的那批）
  Object.keys(sqlKeywordSet(cm)).forEach(k => {
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
  // 停在比较符后面就是在手填值：这里不做序号选中，数字要能原样打进 SQL
  const valuePos = inValuePosition(cm, base.from);
  // LIMIT / OFFSET 这类后面是数字，没有任何可补全的
  if (context === 'none') return { list: [], from: base.from, to: base.to, valuePos };
  // 语句里引用到的表还没缓存列信息时，后台补拉一次（别名补全靠的就是这份数据）
  const refs = parseTableRefs(cm.getRange({ line: 0, ch: 0 }, cm.getCursor()));
  ensureColumns(cm, Object.keys(refs).map(k => refs[k]), connStore);
  // addon 的结果也要按上下文过滤，否则表名依旧会从前缀命中里漏出来
  let list = base.list;
  if (context === 'table') list = list.filter(isTableItem);
  else if (context === 'column') list = list.filter(it => !isTableItem(it));
  normalizeTableItems(list, quoted, connStore);
  // addon 返回的表项没有 _parts，按「表名  注释」的约定拆出来补上，让它也走分色渲染
  list.forEach(it => {
    if (typeof it !== 'object' || !isTableItem(it) || it._parts) return;
    const segs = String(it.displayText || it.text || '').split(/\s{2,}/);
    it._parts = { name: segs[0] || '', type: '', comment: segs.slice(1).join('  ') };
  });
  // 光标停在已有闭合反引号之前时（如 FROM `log|`），base.to 只覆盖到 `log，
  // 而补全项自带首尾反引号，旧的那个闭合反引号会残留成 `xxx``。
  // 这里把替换范围向后吃掉「紧邻的标识符字符 + 一个闭合反引号」。
  let to = base.to;
  if (quoted) {
    const line = cm.getLine(to.line) || '';
    const m = /^[\w$]*`/.exec(line.slice(to.ch));
    if (m) to = { line: to.line, ch: to.ch + m[0].length };
  }
  // FROM / JOIN 后面选中表名时顺手补别名（多表 JOIN 少写一步手写别名）。
  // INTO / UPDATE 后面补别名没有意义；已有别名占用的名字要避开，避免补出重名
  const aliasHere = context === 'table'
    && ['FROM', 'JOIN'].indexOf(tableClauseBefore(cm, base.from)) !== -1;
  const takenAlias = {};
  if (aliasHere) {
    Object.keys(refs).forEach(k => { if (k !== refs[k].toLowerCase()) takenAlias[k] = true; });
  }
  const addAlias = it => {
    if (!aliasHere || !isTableItem(it)) return it;
    const plain = String((typeof it === 'string' ? it : it.text) || '').replace(/`/g, '');
    return withAliasPick(it, aliasFor(plain, takenAlias));
  };
  if (aliasHere) list = list.map(addAlias);
  if (!search) {
    // 没输入片段时 column 上下文也要给「别名.字段」，否则像「ON u.id=」这种位置只剩关键字
    if (context === 'column') {
      const seenIdle = {};
      list.forEach(it => { seenIdle[typeof it === 'string' ? it : it.text] = true; });
      const idle = [];
      const bare = bareColumnSingleTable(cm, connStore);
      scopedColumnRefs(cm, connStore).forEach(r => {
        const text = bare ? r.columnName : `${r.prefix}.${r.columnName}`;
        if (seenIdle[text]) return;
        seenIdle[text] = true;
        idle.push({
          text,
          displayText: columnDisplay(text, r.dataType, r.columnComment),
          _parts: { name: text, type: r.dataType || '', comment: r.columnComment || '' }
        });
      });
      // 光标停在「时间列 > 」这类比较符后面时，取值模板排最前面（近 7 天 / 状态枚举值）
      const valueItems = buildValueItems(cm, base.from, '', connStore);
      if (valueItems.length || idle.length) {
        const g = groupCandidates(cm, list);
        // 表名 / 字段名（idle 与 addon 命中里的标识符）排关键字前面
        return {
          list: numberize(valueItems.concat(idle, g.ids, g.tpl, g.kws), !valuePos),
          from: base.from, to, valuePos
        };
      }
    }
    const g = groupCandidates(cm, list);
    return { list: numberize(g.ids.concat(g.tpl, g.kws), !valuePos), from: base.from, to, valuePos };
  }
  const poolMode = context === 'table' ? 'table' : (context === 'column' ? 'keyword' : 'all');
  // 语句开头的骨架模板排在候选最前面，敲 s 时第一项就是「SELECT * FROM 」
  const snippets = buildSnippetItems(cm, base.from, search, connStore);
  const historyItems = buildHistoryItems(cm, base.from, search, connStore);
  const funcItems = buildTemplateItems(context, search, SQL_FUNCTIONS);
  const opItems = buildTemplateItems(context, search, SQL_OPERATORS);
  const valueItems = buildValueItems(cm, base.from, search, connStore);
  const seen = {};
  const indexOf = {};
  // addon 命中的项（可能只是裸字符串）先登记：模糊池里带类型 / 注释的项会补到同一项上，
  // 而不是另插一条重复候选，否则用户拿到的还是没类型没注释的裸列名
  list.forEach((it, i) => {
    const key = typeof it === 'string' ? it : it.text;
    seen[key] = true;
    indexOf[key] = i;
  });
  snippets.concat(valueItems, historyItems, funcItems, opItems).forEach(it => { seen[it.text] = true; });
  const extra = [];
  buildFuzzyPool(cm, search, poolMode, connStore)
    .map(c => ({ c, s: fuzzyScore(c.match, c.query) }))
    // 点号后还没输入字段片段时（query 为空）fuzzyScore 返回 0，不能按 >0 过滤掉，
    // 否则「别名.」后面什么都不弹
    .filter(x => x.s > 0 || !x.c.query)
    .sort((a, b) => b.s - a.s)
    .forEach(({ c }) => {
      const key = c.text;
      if (seen[key]) {
        // 已有同名候选（addon 给的裸列名）：把类型 / 注释 / 配色补到那一项上，
        // 纯字符串项原地升级成对象，避免同一列在列表里出现两次
        const i = indexOf[key];
        if (i === undefined) return;
        const cur = list[i];
        if (typeof cur === 'string') {
          const up = { text: cur };
          if (c.displayText) up.displayText = c.displayText;
          if (c.className) up.className = c.className;
          if (c._parts) up._parts = c._parts;
          list[i] = addAlias(up);
        } else {
          if (c.displayText && !cur.displayText) cur.displayText = c.displayText;
          if (c.className && !cur.className) cur.className = c.className;
          if (c._parts && !cur._parts) cur._parts = c._parts;
        }
        return;
      }
      if (extra.length >= 50) return;
      seen[key] = true;
      const item = addAlias({ text: key });
      if (c.className) item.className = c.className;
      if (c.displayText) item.displayText = c.displayText;
      if (c._parts) item._parts = c._parts;
      extra.push(item);
    });
  // 排序：取值模板（在比较符后面时）> 整句模板 > 表名/字段名 > 函数与历史 > 关键字。
  // 表名与字段名永远排在关键字前面：写 SQL 时最常补的就是它们
  const g = groupCandidates(cm, list.concat(extra));
  const merged = valueItems.concat(snippets, g.ids, historyItems, funcItems, opItems, g.tpl, g.kws);
  return { list: numberize(merged, !valuePos), from: base.from, to, valuePos };
}

// 便捷工厂：绑定 cm 与 store，返回 { showSqlHint }
export function createSqlHint(cm, connStore) {
  return {
    showSqlHint: () => showSqlHint(cm, connStore)
  };
}
