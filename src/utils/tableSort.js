// 表列表排序：维度定义（对应后端 information_schema.TABLES 字段）+ 单列/组合排序比较器
import { formatBytes, formatRows } from './format';

// 排序维度，顺序即下拉里的展示顺序
export const TABLE_SORT_FIELDS = [
  { field: 'tableName', label: '表名', type: 'string', text: t => t.tableName || '' },
  { field: 'createTime', label: '创建时间', type: 'string', text: t => t.createTime || '' },
  { field: 'updateTime', label: '更新时间', type: 'string', text: t => t.updateTime || '' },
  { field: 'tableRows', label: '数据行数', type: 'number', text: t => rowsText(t) },
  { field: 'dataLength', label: '数据大小', type: 'number', text: t => formatBytes(t.dataLength) },
  { field: 'indexLength', label: '索引大小', type: 'number', text: t => formatBytes(t.indexLength) },
  { field: 'totalLength', label: '总大小', type: 'number', text: t => formatBytes(t.totalLength) },
  { field: 'engine', label: '存储引擎', type: 'string', text: t => t.engine || '' },
  { field: 'tableComment', label: '表注释', type: 'string', text: t => t.tableComment || '' },
  { field: 'charset', label: '字符集', type: 'string', text: t => t.charset || '' },
  { field: 'tableType', label: '表类型', type: 'string', text: t => t.tableType || '' },
  { field: 'autoIncrement', label: '自增值', type: 'number', text: t => fmtRows(t.autoIncrement) }
];

// 默认按表名升序
export const DEFAULT_TABLE_SORT = [{ field: 'tableName', dir: 'asc' }];

const FIELD_MAP = {};
TABLE_SORT_FIELDS.forEach(f => { FIELD_MAP[f.field] = f; });

function fmtRows(v) {
  if (v === null || v === undefined) return '';
  return formatRows(v) + ' 行';
}

// 数据行数取的是 information_schema.TABLES 的统计值（InnoDB 是采样估算，小表常明显不准），
// 只作量级参考，所以统一加「约」；自增值是精确值，仍走 fmtRows 不加「约」
function approxRows(v) {
  const t = fmtRows(v);
  return t ? '约 ' + t : '';
}

// 精确行数（右键「精确行数」实时 COUNT 得到的值）展示原始整数：
// 不再加「约」，也不能用 formatRows 的「万/亿」缩写，否则精确值看着仍像估算
function exactRows(v) {
  if (v === null || v === undefined || v === '') return '';
  const n = Number(v);
  if (!isFinite(n)) return '';
  return n.toLocaleString('zh-CN') + ' 行';
}

// 表项上的行数文案：没实时统计过的走估算（带「约」），统计过的显示精确值
export function rowsText(t) {
  if (!t) return '';
  return t.tableRowsExact ? exactRows(t.tableRows) : approxRows(t.tableRows);
}

export function fieldLabel(field) {
  return FIELD_MAP[field] ? FIELD_MAP[field].label : field;
}

// 空值（null / 空串）视为「无」，排序时统一排在最后
function isEmpty(v) {
  return v === null || v === undefined || v === '';
}

// 清洗外部传入的排序键：丢弃未知字段、去重、方向非法则按升序
export function normalizeSortKeys(keys) {
  const out = [];
  const seen = {};
  (keys || []).forEach(k => {
    if (!k || !FIELD_MAP[k.field] || seen[k.field]) return;
    seen[k.field] = true;
    out.push({ field: k.field, dir: k.dir === 'desc' ? 'desc' : 'asc' });
  });
  return out;
}

function compareByField(a, b, field, dir) {
  const def = FIELD_MAP[field];
  const av = a[field];
  const bv = b[field];
  const ae = isEmpty(av);
  const be = isEmpty(bv);
  // 空值恒排最后，不随升降序翻转
  if (ae && be) return 0;
  if (ae) return 1;
  if (be) return -1;
  let r;
  if (def.type === 'number') {
    r = Number(av) - Number(bv);
  } else {
    // 表名等文本：带数字的自然序（table2 排在 table10 前）
    r = String(av).localeCompare(String(bv), 'zh', { numeric: true, sensitivity: 'base' });
  }
  return dir === 'desc' ? -r : r;
}

// 按排序键（可为多列组合，靠前者优先级高）返回新数组；无有效键时保持原顺序
export function sortTables(tables, keys) {
  const list = (tables || []).slice();
  const valid = normalizeSortKeys(keys);
  if (!valid.length) return list;
  list.sort((a, b) => {
    for (const k of valid) {
      const r = compareByField(a, b, k.field, k.dir);
      if (r !== 0) return r;
    }
    return 0;
  });
  return list;
}

// 表项下方的一行元信息：只展示参与排序的维度（表名/注释已在表项上，不重复），最多 3 个
export function metaText(table, keys, max = 3) {
  const parts = [];
  for (const k of normalizeSortKeys(keys)) {
    if (k.field === 'tableName' || k.field === 'tableComment') continue;
    const txt = FIELD_MAP[k.field].text(table);
    if (txt) parts.push(txt);
    if (parts.length >= max) break;
  }
  return parts.join(' · ');
}

// 默认元信息：用户没额外指定排序维度时，表项下方固定展示「约 N 行 · 总大小」
export function defaultMetaText(table) {
  const rows = rowsText(table);
  const size = formatBytes(table.totalLength);
  return [rows, size].filter(Boolean).join(' · ');
}
