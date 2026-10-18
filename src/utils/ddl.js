// 表结构设计器：由「原始结构 vs 编辑后结构」生成 ALTER 脚本（纯函数，无副作用）
import { sqlIdent } from './sql';

const qi = sqlIdent;

// ===== 转义 / 字面量 =====
function escapeStr(s) {
  return String(s == null ? '' : s)
    .replace(/\\/g, '\\\\')
    .replace(/'/g, "\\'")
    .replace(/\n/g, '\\n')
    .replace(/\r/g, '\\r')
    .replace(/\0/g, '\\0')
    .replace(/\u001a/g, '\\Z');
}

function literal(s) {
  return "'" + escapeStr(s) + "'";
}

// 默认值渲染：NULL / 数字 / CURRENT_TIMESTAMP 等表达式不加引号，其余按字符串加引号
function renderDefault(v) {
  const s = String(v).trim();
  if (/^null$/i.test(s)) return 'NULL';
  if (/^-?\d+(\.\d+)?$/.test(s)) return s;
  if (/^(current_timestamp|current_date|current_time)(\(\d*\))?$/i.test(s)) return s;
  if (/^now\(\)$/i.test(s)) return s;
  return literal(v);
}

export function hasAutoInc(col) {
  return String(col.extra || '').toLowerCase().includes('auto_increment');
}

export function hasOnUpdate(col) {
  return /on update/i.test(col.extra || '');
}

export function isGenerated(col) {
  return !!col.generationExpression
    || String(col.extra || '').toLowerCase().includes('generated');
}

// 列定义（不含列名）；用于属性比对与 MODIFY / CHANGE
function columnAttrsSql(col) {
  const parts = [col.columnType];
  if (col.charset) parts.push('CHARACTER SET ' + col.charset);
  if (col.collation) parts.push('COLLATE ' + col.collation);
  parts.push(col.nullable ? 'NULL' : 'NOT NULL');
  if (isGenerated(col)) {
    const kind = String(col.extra || '').toUpperCase().includes('STORED') ? 'STORED' : 'VIRTUAL';
    parts.push('GENERATED ALWAYS AS (' + col.generationExpression + ') ' + kind);
  } else if (col.default !== null && col.default !== undefined) {
    parts.push('DEFAULT ' + renderDefault(col.default));
  }
  if (hasAutoInc(col)) parts.push('AUTO_INCREMENT');
  if (hasOnUpdate(col)) parts.push('ON UPDATE CURRENT_TIMESTAMP');
  if (col.comment) parts.push('COMMENT ' + literal(col.comment));
  return parts.join(' ');
}

function columnDefSql(col) {
  return qi(col.name) + ' ' + columnAttrsSql(col);
}

// ===== 索引 =====
function indexDropClause(ix) {
  return ix.name === 'PRIMARY' ? 'DROP PRIMARY KEY' : 'DROP INDEX ' + qi(ix.name);
}

function indexAddClause(ix) {
  const cols = (ix.columns || []).map(qi).join(', ');
  const type = String(ix.indexType || '').toUpperCase();
  if (ix.name === 'PRIMARY') return `ADD PRIMARY KEY (${cols})`;
  if (type === 'FULLTEXT') return `ADD FULLTEXT KEY ${qi(ix.name)} (${cols})`;
  if (type === 'SPATIAL') return `ADD SPATIAL KEY ${qi(ix.name)} (${cols})`;
  return `ADD ${ix.unique ? 'UNIQUE ' : ''}KEY ${qi(ix.name)} (${cols})`;
}

function sameArr(a, b) {
  const x = a || [];
  const y = b || [];
  return x.length === y.length && x.every((v, i) => String(v) === String(y[i]));
}

function indexChanged(orig, cur) {
  if (!orig) return true;
  return orig.unique !== cur.unique
    || String(orig.indexType || '').toUpperCase() !== String(cur.indexType || '').toUpperCase()
    || !sameArr(orig.columns, cur.columns);
}

// ===== 外键 =====
function fkAddClause(fk) {
  const cols = (fk.columns || []).map(qi).join(', ');
  const refCols = (fk.refColumns || []).map(qi).join(', ');
  return `ADD CONSTRAINT ${qi(fk.name)} FOREIGN KEY (${cols})`
    + ` REFERENCES ${qi(fk.refTable)} (${refCols})`
    + ` ON DELETE ${fk.onDelete || 'RESTRICT'} ON UPDATE ${fk.onUpdate || 'RESTRICT'}`;
}

function fkChanged(orig, cur) {
  if (!orig) return true;
  return !sameArr(orig.columns, cur.columns)
    || orig.refTable !== cur.refTable
    || !sameArr(orig.refColumns, cur.refColumns)
    || String(orig.onDelete || '') !== String(cur.onDelete || '')
    || String(orig.onUpdate || '') !== String(cur.onUpdate || '');
}

// ===== 列 =====
function columnClauses(origCols, draftCols) {
  const origNames = origCols.map(c => c.name);
  const draftOrig = draftCols.filter(c => c.origName);
  const draftOrigNames = draftOrig.map(c => c.origName);

  // 已存在列里，相对顺序发生变化的（用于决定是否补 FIRST / AFTER）
  const movedOrig = new Set();
  draftOrig.forEach(c => {
    const from = origNames.indexOf(c.origName);
    const to = draftOrigNames.indexOf(c.origName);
    if (from !== to) movedOrig.add(c.origName);
  });

  const clauses = [];
  draftCols.forEach((c, idx) => {
    const pos = () => (idx === 0 ? ' FIRST' : ' AFTER ' + qi(draftCols[idx - 1].name));
    const orig = c.origName ? origCols.find(o => o.name === c.origName) : null;
    if (!c.origName) {
      clauses.push('ADD COLUMN ' + columnDefSql(c) + pos());
    } else if (c.origName !== c.name) {
      clauses.push('CHANGE COLUMN ' + qi(c.origName) + ' ' + columnDefSql(c)
        + (movedOrig.has(c.origName) ? pos() : ''));
    } else {
      const attrsChanged = columnAttrsSql(orig || {}) !== columnAttrsSql(c);
      const moved = movedOrig.has(c.origName);
      if (attrsChanged || moved) {
        clauses.push('MODIFY COLUMN ' + columnDefSql(c) + (moved ? pos() : ''));
      }
    }
  });

  const keep = new Set(draftOrigNames);
  origCols.forEach(o => {
    if (!keep.has(o.name)) clauses.push('DROP COLUMN ' + qi(o.name));
  });

  return clauses;
}

// ===== 表选项 =====
function optionClauses(orig, cur) {
  const o = orig || {};
  const c = cur || {};
  const clauses = [];
  if (c.engine && c.engine !== o.engine) clauses.push('ENGINE=' + c.engine);
  if (c.charset && c.charset !== o.charset) clauses.push('DEFAULT CHARSET=' + c.charset);
  if (c.collation && c.collation !== o.collation) clauses.push('COLLATE=' + c.collation);
  if (String(c.comment || '') !== String(o.comment || '')) {
    clauses.push('COMMENT=' + literal(c.comment || ''));
  }
  const ai = c.autoIncrement;
  if (ai !== null && ai !== undefined && ai !== '' && String(ai) !== String(o.autoIncrement ?? '')) {
    clauses.push('AUTO_INCREMENT=' + Number(ai));
  }
  return clauses;
}

function alter(table, clauses) {
  return 'ALTER TABLE ' + table + '\n  ' + clauses.join(',\n  ') + ';';
}

// ===== 对外：生成 ALTER 脚本 =====
// original / draft 均为 normalizeStructure 之后的形状；draft.columns 需带 origName
export function buildAlterScript(original, draft, table) {
  const t = qi(table);
  const orig = original || {};
  const origCols = orig.columns || [];
  const origIndexes = orig.indexes || [];
  const origFks = orig.foreignKeys || [];
  const draftCols = (draft && draft.columns) || [];
  const draftIndexes = (draft && draft.indexes) || [];
  const draftFks = (draft && draft.foreignKeys) || [];

  const statements = [];

  // 1) 删除外键（改外键先删后加，必须分两条语句）
  const keptFk = new Set(draftFks.map(f => f.origName).filter(Boolean));
  const fkDrops = origFks.filter(f => !keptFk.has(f.name)).map(f => 'DROP FOREIGN KEY ' + qi(f.name));
  if (fkDrops.length) statements.push(alter(t, fkDrops));

  // 2) 删除索引（被移除或改定义的）
  // 主键要单独处理：MySQL 支持 DROP PRIMARY KEY，但主键列带 AUTO_INCREMENT 时单独执行
  // 会报 ERROR 1075（Incorrect table definition; there can be only one auto column and it
  // must be defined as a key），删完主键那一刻该列不再是任何键的一部分。
  // 所以「改主键」时把 DROP 并进新增那条语句（MySQL 支持同语句内先删后加），
  // 只有「整条删除主键」才单独生成 DROP PRIMARY KEY。
  const draftPrimary = draftIndexes.find(x => x.name === 'PRIMARY');
  const idxDrops = [];
  let primarySwap = false;
  origIndexes.forEach(oi => {
    const di = draftIndexes.find(x => x.origName === oi.name);
    if (di && !indexChanged(oi, di)) return;
    if (oi.name === 'PRIMARY') {
      if (draftPrimary) primarySwap = true;
      else idxDrops.push('DROP PRIMARY KEY');
      return;
    }
    idxDrops.push(indexDropClause(oi));
  });
  if (idxDrops.length) statements.push(alter(t, idxDrops));

  // 3) 主键重建 + 列变更 + 新增索引合并为一条：新增自增列要求同语句内带键
  const cols = columnClauses(origCols, draftCols);
  const idxAdds = [];
  draftIndexes.forEach(di => {
    const oi = di.origName ? origIndexes.find(x => x.name === di.origName) : null;
    if (!oi || indexChanged(oi, di)) idxAdds.push(indexAddClause(di));
  });
  const combined = primarySwap ? ['DROP PRIMARY KEY', ...cols, ...idxAdds] : [...cols, ...idxAdds];
  if (combined.length) statements.push(alter(t, combined));

  // 4) 新增外键
  const fkAdds = [];
  draftFks.forEach(df => {
    const of = df.origName ? origFks.find(x => x.name === df.origName) : null;
    if (!of || fkChanged(of, df)) fkAdds.push(fkAddClause(df));
  });
  if (fkAdds.length) statements.push(alter(t, fkAdds));

  // 5) 表选项
  const opts = optionClauses(orig.options, draft.options);
  if (opts.length) statements.push(alter(t, opts));

  return statements.join('\n\n');
}

// ===== 结构 → 草稿 =====
// 规范化：把空串默认值 / 空生成表达式统一为 null，保证原始与草稿比对一致
export function normalizeStructure(structure) {
  const s = structure || {};
  return {
    columns: (s.columns || []).map(c => ({
      ...c,
      default: c.default === '' || c.default === undefined ? null : c.default,
      generationExpression: c.generationExpression || null
    })),
    indexes: (s.indexes || []).map(ix => ({ ...ix, columns: [...(ix.columns || [])] })),
    foreignKeys: (s.foreignKeys || []).map(fk => ({
      ...fk,
      columns: [...(fk.columns || [])],
      refColumns: [...(fk.refColumns || [])]
    })),
    options: { ...(s.options || {}) }
  };
}

// 规范化后的列 → 草稿列（补 origName）
export function toDraftColumns(columns) {
  return (columns || []).map(c => ({ ...c, origName: c.name }));
}

export function toDraftIndexes(indexes) {
  return (indexes || []).map(ix => ({ ...ix, origName: ix.name, columns: [...(ix.columns || [])] }));
}

export function toDraftForeignKeys(fks) {
  return (fks || []).map(fk => ({
    ...fk,
    origName: fk.name,
    columns: [...(fk.columns || [])],
    refColumns: [...(fk.refColumns || [])]
  }));
}

export function newColumn() {
  return {
    origName: null,
    name: '',
    columnType: 'varchar(255)',
    dataType: 'varchar',
    nullable: true,
    default: null,
    columnKey: null,
    extra: '',
    comment: null,
    charset: null,
    collation: null,
    generationExpression: null,
    ordinal: 0
  };
}

export function newIndex() {
  return { origName: null, name: '', unique: false, indexType: 'BTREE', columns: [] };
}

export function newForeignKey() {
  return {
    origName: null,
    name: '',
    columns: [],
    refTable: '',
    refColumns: [],
    onDelete: 'RESTRICT',
    onUpdate: 'RESTRICT'
  };
}
