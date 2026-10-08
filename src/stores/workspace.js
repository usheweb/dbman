// 工作区：SQL 编辑器实例 + 结果集 + 显示模式 + 查找 + 分页 + 导出
import { defineStore } from 'pinia';
import { ref, shallowRef, computed } from 'vue';
import * as api from '@/api';
import { useStoreStore } from './store';
import { useUiStore } from './ui';
import { useConnectionStore } from './connection';
import {
  rawText, formatValue, formatDateTime, isTimestampValue,
  parseEnumFromComment, stripEnumFromComment, csvCell, exportFilename
} from '@/utils/format';
import {
  detectDangerousStatements, detectSqlType, sqlLogClass,
  splitTopLevelClauses, rebuildSql, stripSqlLiterals, sqlIdent, parseWhereToRows
} from '@/utils/sql';

const MODE_LABEL = { raw: '原值', timestamp: '时间', enum: '文字' };

export const useWorkspaceStore = defineStore('workspace', () => {
  const store = useStoreStore();
  const ui = useUiStore();
  const conn = useConnectionStore();

  // CodeMirror 实例：必须 shallowRef，Vue 深代理会破坏 CM 内部结构
  const editor = shallowRef(null);

  const lastSql = ref('');
  const lastResult = ref(null); // { data, fields, colInfo }
  const columnModes = ref({});  // 单列显式覆盖
  const globalDisplay = ref({ timestamp: false, enum: false });

  const pageSize = ref(300);
  const pageNo = ref(1);
  const pageInfo = ref({ hasMore: false, paged: true, elapsed: 0, rows: 0, idle: true, write: false, error: '' });

  const findState = ref({ keyword: '', only: true, index: -1 });
  const matchCells = ref([]); // [{ r, c }]，当前页命中的单元格

  // 危险操作二次确认
  const execConfirm = ref({ visible: false, kinds: [], sql: '' });
  let execConfirmResolver = null;

  // 筛选条件弹窗：初始条件行由「结果表头右键 → 筛选…」预填
  const filterDialog = ref({ visible: false, rows: [], targetCol: '' });

  function init() {
    pageSize.value = parseInt(store.get('dbman_page_size'), 10) || 300;
    Object.assign(globalDisplay.value, store.get('dbman_global_display') || {});
  }

  // ===== 编辑器 =====
  function setEditor(cm) { editor.value = cm; }
  function getSql() { return editor.value ? editor.value.getValue() : ''; }
  function setSql(sql) { if (editor.value) editor.value.setValue(sql); }

  // ===== 列注释 / 显示模式 =====
  function currentColComments() {
    const map = {};
    const table = conn.currentTable;
    if (table && conn.tableColumns[table]) {
      conn.tableColumns[table].forEach(c => { map[c.columnName] = c.columnComment; });
    }
    return map;
  }

  function effectiveColumnMode(col, colInfo) {
    if (columnModes.value[col]) return columnModes.value[col];
    if (globalDisplay.value.timestamp && colInfo && colInfo.canTimestamp) return 'timestamp';
    if (globalDisplay.value.enum && colInfo && colInfo.enumMap) return 'enum';
    return 'raw';
  }

  function applyDisplayMode(value, mode, colInfo) {
    if (mode === 'timestamp' && isTimestampValue(value)) {
      const s = String(value);
      return formatDateTime(new Date(s.length === 10 ? Number(s) * 1000 : Number(s)));
    }
    if (mode === 'enum' && colInfo && colInfo.enumMap) {
      const key = String(value);
      if (Object.prototype.hasOwnProperty.call(colInfo.enumMap, key)) return colInfo.enumMap[key];
    }
    return formatValue(value);
  }

  // 逐列判断能否转换。注释枚举额外要求：至少一个单元格值命中 key，避免误判
  function analyzeColumns(cols, data, colComments) {
    const info = {};
    cols.forEach(col => {
      const values = [];
      const tsValues = [];
      for (let i = 0; i < data.length; i++) {
        if (values.length >= 20 && tsValues.length >= 20) break;
        const v = data[i][col];
        if (v === null || v === undefined || v === '') continue;
        const s = String(v);
        if (values.length < 20) values.push(s);
        if (s !== '0' && tsValues.length < 20) tsValues.push(s);
      }
      const enumMap = parseEnumFromComment(colComments[col]);
      info[col] = {
        canTimestamp: tsValues.length > 0 && tsValues.every(isTimestampValue),
        enumMap: enumMap && values.some(v => Object.prototype.hasOwnProperty.call(enumMap, v)) ? enumMap : null
      };
    });
    return info;
  }

  function toggleGlobalMode(kind) {
    globalDisplay.value[kind] = !globalDisplay.value[kind];
    store.set('dbman_global_display', { ...globalDisplay.value });
  }

  function toggleColumnMode(col) {
    const info = (lastResult.value && lastResult.value.colInfo[col]) || {};
    const modes = ['raw'];
    if (info.canTimestamp) modes.push('timestamp');
    if (info.enumMap) modes.push('enum');
    const cur = columnModes.value[col] || effectiveColumnMode(col, info);
    columnModes.value[col] = modes[(modes.indexOf(cur) + 1) % modes.length];
  }

  // 结果集列名
  const resultColumns = computed(() => {
    const r = lastResult.value;
    if (!r || !r.data || !r.data.length) return [];
    return (r.fields && r.fields.length) ? r.fields.map(f => f.name) : Object.keys(r.data[0]);
  });

  const columnModesForRender = computed(() => {
    const out = {};
    const info = (lastResult.value && lastResult.value.colInfo) || {};
    resultColumns.value.forEach(col => { out[col] = effectiveColumnMode(col, info[col]); });
    return out;
  });

  function cellText(row, col, info) {
    const mode = columnModesForRender.value[col];
    const disp = applyDisplayMode(row[col], mode, info);
    return (disp + '\u0000' + rawText(row[col])).toLowerCase();
  }

  // 行是否命中查找
  function rowHit(row) {
    const info = (lastResult.value && lastResult.value.colInfo) || {};
    const kw = findState.value.keyword.toLowerCase();
    if (!kw) return false;
    return resultColumns.value.some(col => cellText(row, col, info[col]).includes(kw));
  }

  function cellHit(row, col) {
    const kw = findState.value.keyword.toLowerCase();
    if (!kw) return false;
    const info = (lastResult.value && lastResult.value.colInfo) || {};
    return cellText(row, col, info[col]).includes(kw);
  }

  // 可见行索引（勾选「过滤」时只留包含查找内容的行）
  const visibleRowIndexes = computed(() => {
    const data = (lastResult.value && lastResult.value.data) || [];
    const idx = [];
    data.forEach((row, i) => {
      if (findState.value.only && findState.value.keyword && !rowHit(row)) return;
      idx.push(i);
    });
    return idx;
  });

  // 重算命中单元格
  function recomputeMatches() {
    const cells = [];
    const data = (lastResult.value && lastResult.value.data) || [];
    const kw = findState.value.keyword;
    if (kw) {
      data.forEach((row, r) => {
        if (findState.value.only && !rowHit(row)) return;
        resultColumns.value.forEach((col, c) => {
          if (cellHit(row, col)) cells.push({ r, c });
        });
      });
    }
    matchCells.value = cells;
    if (findState.value.index >= cells.length) findState.value.index = -1;
  }

  const matchCount = computed(() => matchCells.value.length);

  // ===== 查询执行 =====
  function resetPageState() {
    pageNo.value = 1;
    pageInfo.value = { hasMore: false, paged: true, elapsed: 0, rows: 0, idle: true, write: false, error: '' };
  }

  // 分页时多取一行，用来判断是否还有下一页（不用额外查 COUNT）
  async function fetchRows(sql, page) {
    return conn.withRetry(id => api.mysqlQuery(
      id, sql, conn.currentDatabase, pageSize.value + 1, (page - 1) * pageSize.value
    ));
  }

  // 收下一批行；paged=false 表示服务端没加 LIMIT（SQL 自带 LIMIT 或不是查询语句）
  function adoptRows(data, page, elapsed) {
    if (data.write) {
      pageNo.value = page;
      pageInfo.value = { hasMore: false, paged: false, elapsed, rows: data.affected || 0, write: true, idle: false, error: '' };
      lastResult.value = null;
      return;
    }
    const paged = data.paged !== false;
    const list = data.rows || [];
    const hasMore = paged && list.length > pageSize.value;
    const rows = hasMore ? list.slice(0, pageSize.value) : list;
    const cols = (data.fields && data.fields.length) ? data.fields.map(f => f.name) : (rows[0] ? Object.keys(rows[0]) : []);
    const colInfo = analyzeColumns(cols, rows, currentColComments());
    lastResult.value = { data: rows, fields: data.fields || [], colInfo };
    pageNo.value = page;
    pageInfo.value = { hasMore, paged, elapsed, rows: rows.length, write: false, idle: false, error: '' };
  }

  // 弹出二次确认框，返回 Promise<boolean>
  function askExecConfirm(kinds, sql) {
    return new Promise(resolve => {
      execConfirmResolver = resolve;
      execConfirm.value = { visible: true, kinds, sql };
    });
  }

  function resolveExecConfirm(ok) {
    execConfirm.value = { ...execConfirm.value, visible: false };
    const resolve = execConfirmResolver;
    execConfirmResolver = null;
    if (resolve) resolve(ok);
  }

  async function executeQuery(confirmed) {
    const sql = getSql();
    if (!sql.trim()) return;
    // 会改动数据 / 结构 / 权限或实例状态的语句，执行前必须二次确认
    if (!confirmed) {
      const kinds = detectDangerousStatements(sql);
      if (kinds.length && !(await askExecConfirm(kinds, sql))) return;
    }
    lastSql.value = sql;
    columnModes.value = {}; // 新一次查询：清空单列覆盖
    resetFindState();
    const started = Date.now();
    try {
      const data = await fetchRows(sql, 1);
      const elapsed = Date.now() - started;
      adoptRows(data, 1, elapsed);
      recomputeMatches();
      addSqlLog(sql, { success: true, elapsed, rows: pageInfo.value.rows });
    } catch (err) {
      renderError(err.message);
      addSqlLog(sql, { success: false, elapsed: Date.now() - started, error: err.message });
    }
  }

  // 翻页 / 改每页条数：复用最近一次执行的 SQL 重新查库，保留查找状态
  async function reloadPage(page) {
    if (!lastSql.value) return;
    const started = Date.now();
    try {
      const data = await fetchRows(lastSql.value, page);
      adoptRows(data, page, Date.now() - started);
      recomputeMatches();
    } catch (err) {
      ui.showToast(err.message, 'error');
    }
  }

  function gotoPage(page) {
    if (!pageInfo.value.paged || page < 1) return;
    if (page > pageNo.value && !pageInfo.value.hasMore) return;
    reloadPage(page);
  }

  function changePageSize(value) {
    pageSize.value = parseInt(value, 10) || 300;
    store.set('dbman_page_size', String(pageSize.value));
    reloadPage(1);
  }

  function renderError(msg) {
    lastResult.value = null;
    matchCells.value = [];
    pageNo.value = 1;
    pageInfo.value = { hasMore: false, paged: false, elapsed: 0, rows: 0, error: msg, idle: false, write: false };
    ui.showToast(msg, 'error');
  }

  // 执行成功后载入编辑器并立即执行（表名点击、排序、筛选都走它）
  function setSqlAndRun(sql) {
    setSql(sql);
    executeQuery();
  }

  // ===== 结果表头右键：排序 / 筛选（改写编辑器 SQL 的对应子句）=====
  // 用新的 where / order 覆盖编辑器 SQL 的对应子句（传 null 表示删除该子句）
  function rewriteEditorSql({ hasWhere, where, hasOrder, order }) {
    const raw = getSql();
    const parsed = splitTopLevelClauses(raw);
    if (!parsed) { ui.showToast('SQL 为空，无法排序/筛选', 'error'); return null; }
    if (!/^\s*(select|with)\b/i.test(parsed.body) || !parsed.body) {
      ui.showToast('仅支持对 SELECT 查询做排序 / 筛选', 'error');
      return null;
    }
    if (stripSqlLiterals(parsed.body).includes(';')) {
      ui.showToast('暂不支持多条语句的排序 / 筛选', 'error');
      return null;
    }
    if (hasWhere) parsed.where = where;
    if (hasOrder) parsed.order = order;
    return rebuildSql(parsed);
  }

  function sortResultBy(col, dir) {
    const sql = rewriteEditorSql({ hasOrder: true, order: `${sqlIdent(col)} ${dir}` });
    if (!sql) return;
    setSqlAndRun(sql);
    ui.showToast(`已按「${col}」${dir === 'ASC' ? '升序' : '降序'}排序`, 'success');
  }

  function clearResultSort() {
    const sql = rewriteEditorSql({ hasOrder: true, order: null });
    if (!sql) return;
    setSqlAndRun(sql);
    ui.showToast('已清除排序', 'success');
  }

  function clearResultFilter() {
    const sql = rewriteEditorSql({ hasWhere: true, where: null });
    if (!sql) return;
    setSqlAndRun(sql);
    ui.showToast('已清除筛选条件', 'success');
  }

  // 打开筛选弹窗：编辑器 SQL 里已有 WHERE 时优先带入，避免「应用」把原条件覆盖掉
  function openFilterDialog(col) {
    const parsed = splitTopLevelClauses(getSql());
    const rows = parsed && parsed.where ? parseWhereToRows(parsed.where) : [];
    if (rows.length) {
      filterDialog.value = { visible: true, rows, targetCol: col || '' };
    } else {
      const cols = resultColumns.value;
      const first = cols.includes(col) ? col : (cols[0] || col || '');
      filterDialog.value = { visible: true, rows: [{ col: first, op: 'eq', v1: '', v2: '', join: 'AND' }], targetCol: col || '' };
    }
  }

  function closeFilterDialog() {
    filterDialog.value = { ...filterDialog.value, visible: false };
  }

  // 应用筛选：把条件写进编辑器 SQL 的 WHERE；exec=false 时只写入编辑器、不执行
  function applyFilter(where, exec = true) {
    if (!where) { ui.showToast('请至少填写一条完整的筛选条件', 'error'); return false; }
    const sql = rewriteEditorSql({ hasWhere: true, where });
    if (!sql) return false;
    setSql(sql);
    closeFilterDialog();
    if (exec) {
      executeQuery();
      ui.showToast('已应用筛选条件', 'success');
    } else {
      ui.showToast('已应用筛选条件（未执行）', 'success');
    }
    return true;
  }

  // ===== 查找 =====
  function resetFindState() {
    findState.value = { keyword: '', only: true, index: -1 };
    matchCells.value = [];
  }

  function applyFind(kw) {
    kw = String(kw || '').trim();
    if (!kw) { clearFind(); return; }
    if (kw !== findState.value.keyword) {
      findState.value.keyword = kw;
      findState.value.index = -1;
    }
    recomputeMatches();
    if (!matchCells.value.length) {
      ui.showToast(`未找到「${kw}」`, 'error');
      return;
    }
    findState.value.index = (findState.value.index + 1) % matchCells.value.length;
  }

  function toggleFindOnly(checked, kw) {
    if (checked && !findState.value.keyword) {
      const k = String(kw || '').trim();
      if (!k) {
        ui.showToast('请先输入查找内容', 'error');
        return false;
      }
      findState.value.keyword = k;
      findState.value.index = -1;
    }
    findState.value.only = !!checked;
    recomputeMatches();
    return true;
  }

  function clearFind() {
    resetFindState();
  }

  // ===== SQL 日志 / 收藏（写在 sql store 里，这里做转发避免循环依赖）=====
  let sqlStoreRef = null;
  function bindSqlStore(s) { sqlStoreRef = s; }
  function addSqlLog(sql, result) {
    if (sqlStoreRef) sqlStoreRef.addLog(sql, result, conn.currentConnData, conn.currentDatabase);
  }

  // ===== 导出 =====
  function exportResultCols() {
    const r = lastResult.value;
    if (!r || !r.data.length) return [];
    return (r.fields && r.fields.length) ? r.fields.map(f => f.name) : Object.keys(r.data[0]);
  }

  function buildExportHeaders(cols) {
    const comments = currentColComments();
    const mode = ui.exportOpts.header;
    return cols.map(col => {
      const raw = comments[col] || '';
      const short = raw ? stripEnumFromComment(raw) : '';
      if (mode === 'field') return col;
      if (mode === 'comment') return short || col;
      return short ? `${col} ${short}` : col;
    });
  }

  function exportValue(value, mode, info) {
    if (value === null || value === undefined) return '';
    if (mode === 'timestamp' && isTimestampValue(value)) {
      const s = String(value);
      return formatDateTime(new Date(s.length === 10 ? Number(s) * 1000 : Number(s)));
    }
    if (mode === 'enum' && info && info.enumMap) {
      const key = String(value);
      if (Object.prototype.hasOwnProperty.call(info.enumMap, key)) return info.enumMap[key];
    }
    return rawText(value);
  }

  async function exportCurrentPage() {
    const data = (lastResult.value && lastResult.value.data) || [];
    const cols = exportResultCols();
    if (!data.length || !cols.length) {
      ui.showToast('没有可导出的数据', 'error');
      return;
    }
    const colInfo = lastResult.value.colInfo || {};
    const lines = [buildExportHeaders(cols).map(v => csvCell(v)).join(',')];
    data.forEach(row => {
      lines.push(cols.map(col => csvCell(exportValue(row[col], effectiveColumnMode(col, colInfo[col]), colInfo[col]))).join(','));
    });
    const path = await api.saveDialog({
      defaultPath: exportFilename(),
      filters: [{ name: 'CSV', extensions: ['csv'] }]
    });
    if (!path) return;
    try {
      await api.writeTextFile(path, '\ufeff' + lines.join('\r\n'));
      ui.showToast(`已导出 ${data.length} 行`, 'success');
    } catch (err) {
      ui.showToast(String(err), 'error');
    }
  }

  async function exportAllRows() {
    const cols = exportResultCols();
    if (!cols.length) {
      ui.showToast('没有可导出的数据', 'error');
      return;
    }
    const colInfo = (lastResult.value && lastResult.value.colInfo) || {};
    const path = await api.saveDialog({
      defaultPath: exportFilename(),
      filters: [{ name: 'CSV', extensions: ['csv'] }]
    });
    if (!path) return;
    try {
      await api.mysqlExport(
        conn.serverConnId, lastSql.value, conn.currentDatabase,
        buildExportHeaders(cols),
        cols.map(col => {
          const info = colInfo[col] || {};
          return { name: col, mode: effectiveColumnMode(col, info), enumMap: info.enumMap || null };
        }),
        path
      );
      ui.showToast('已开始导出全部数据，请稍候…', 'success');
    } catch (err) {
      ui.showToast(String(err), 'error');
    }
  }

  return {
    editor, lastSql, lastResult, columnModes, globalDisplay,
    pageSize, pageNo, pageInfo, findState, matchCells, matchCount,
    execConfirm, filterDialog,
    resultColumns, columnModesForRender, visibleRowIndexes,
    MODE_LABEL,
    init, setEditor, getSql, setSql, setSqlAndRun,
    currentColComments, effectiveColumnMode, applyDisplayMode, analyzeColumns,
    toggleGlobalMode, toggleColumnMode,
    rowHit, cellHit, recomputeMatches,
    executeQuery, reloadPage, gotoPage, changePageSize, resetPageState,
    askExecConfirm, resolveExecConfirm,
    rewriteEditorSql, sortResultBy, clearResultSort, clearResultFilter,
    openFilterDialog, closeFilterDialog, applyFilter,
    applyFind, toggleFindOnly, clearFind, resetFindState,
    bindSqlStore, addSqlLog,
    exportResultCols, buildExportHeaders, exportValue,
    exportCurrentPage, exportAllRows
  };
});
