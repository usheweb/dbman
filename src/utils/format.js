// 值 / 时间 / 枚举的展示与导出格式化

export function pad2(n) {
  return String(n).padStart(2, '0');
}

export function formatDateTime(date) {
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())} `
    + `${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`;
}

// 原始值 → 文本。JSON 列在驱动里已经是对象/数组，直接 String() 只会得到 "[object Object]"，
// 所以对象统一 JSON 序列化，这样单元格、内容详情弹窗、导出拿到的都是真实内容。
export function rawText(value) {
  if (value === null || value === undefined) return '';
  if (typeof value === 'object' && !(value instanceof Date)) {
    try {
      return JSON.stringify(value);
    } catch (e) {
      return '';
    }
  }
  return String(value);
}

export function formatValue(value) {
  if (value === null || value === undefined) return 'NULL';
  const str = rawText(value);
  return str.length > 100 ? str.substring(0, 97) + '...' : str;
}

// 单个值是否像 Unix 时间戳：10 位（秒）或 13 位（毫秒），且落在合理年份内；0 不算
export function isTimestampValue(v) {
  if (v === null || v === undefined) return false;
  const s = String(v);
  if (!/^\d{10}$|^\d{13}$/.test(s)) return false;
  const year = new Date(s.length === 10 ? Number(s) * 1000 : Number(s)).getFullYear();
  return year >= 2000 && year <= 2100;
}

export function isTimestamp(str) {
  const s = String(str);
  if (!/^\d+$/.test(s)) return false;
  return s.length === 10 || s.length === 13;
}

export function isPossibleDate(str) {
  return /\d{4}[-/]\d{1,2}[-/]\d{1,2}/.test(String(str));
}

// 从字段注释解析枚举映射；只认「数字」或「英文单词」作为 key，其余情况一律不解析
export function parseEnumFromComment(comment) {
  if (!comment) return null;
  const paren = comment.match(/[（(]([^）)]*)[）)]/);
  const seg = paren ? paren[1] : comment;
  // 先按逗号/分号/斜杠等分隔，再在「下一个 key」处二次切分（兼容 "1.成功 2.失败" 这种空格分隔）
  const chunks = seg
    .split(/[,;|、，；\n]/)
    .flatMap(part => part.split(/\s+(?=(?:\d+|[A-Za-z_]\w*)\s*[.\-:：、\s]\s*\S)/));
  const map = {};
  let keyKind = null;
  let homogeneous = true;
  chunks.forEach(chunk => {
    const m = chunk.trim().match(/^(\d+|[A-Za-z_]\w*)\s*[.\-:：、\s]+\s*(.+)$/);
    if (!m) return;
    const key = m[1];
    const label = m[2].trim().replace(/[)\]]+$/, '').trim();
    if (!label) return;
    const kind = /^\d+$/.test(key) ? 'num' : 'word';
    if (keyKind === null) keyKind = kind;
    else if (keyKind !== kind) homogeneous = false;
    map[key] = label;
  });
  const keys = Object.keys(map);
  if (!homogeneous || keys.length < 2) return null;
  return map;
}

// 去掉注释里的枚举说明，只保留前面的描述文字（如「用户状态(1.正常,2.冻结)」→「用户状态」）
export function stripEnumFromComment(comment) {
  if (!comment) return '';
  if (!parseEnumFromComment(comment)) return comment; // 解析不出枚举就原样保留
  const parenIdx = comment.search(/[（(]/);
  if (parenIdx >= 0) return comment.slice(0, parenIdx).trim();
  if (/^(?:\d+|[A-Za-z_]\w*)\s*[.\-:：、\s]+\s*\S/.test(comment)) return '';
  const idx = comment.search(/\s+(?=(?:\d+|[A-Za-z_]\w*)\s*[.\-:：、\s]\s*\S)/);
  return idx > 0 ? comment.slice(0, idx).trim() : comment.trim();
}

export function csvCell(value) {
  if (value === null || value === undefined) return '';
  const s = String(value);
  return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

export function generateId() {
  return Date.now().toString(36) + Math.random().toString(36).substr(2);
}

// 字节数 → 可读大小（B/KB/MB/GB/TB，大于 1KB 时保留 1 位小数）
export function formatBytes(bytes) {
  if (bytes === null || bytes === undefined || bytes === '') return '';
  const n = Number(bytes);
  if (!isFinite(n) || n <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  // 保留 1 位小数后若进位成 1024（如 1048575 字节 → 1024.0 KB），再升一级单位，
  // 避免出现「1024.0 KB」这种本该是「1.0 MB」的尴尬值
  if (i > 0 && Number(v.toFixed(1)) >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return (i === 0 ? String(v) : v.toFixed(1)) + ' ' + units[i];
}

// 行数 / 自增值 → 紧凑文本（万 / 亿），避免长数字撑破窄栏
export function formatRows(n) {
  if (n === null || n === undefined || n === '') return '';
  const v = Number(n);
  if (!isFinite(v)) return '';
  if (v >= 1e8) return (v / 1e8).toFixed(1) + '亿';
  if (v >= 1e4) return (v / 1e4).toFixed(1) + '万';
  return String(v);
}

export function exportFilename() {
  return `dbman_${new Date().toISOString().slice(0, 19).replace(/[-:T]/g, '')}.csv`;
}
