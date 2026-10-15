// 工作区：SQL 编辑器实例 + 结果集 + 显示模式 + 查找 + 分页 + 导出
import { defineStore } from 'pinia';
import { ref, shallowRef, computed, watch } from 'vue';
import * as api from '@/api';
import { useStoreStore } from './store';
import { useUiStore } from './ui';
import { useConnectionStore } from './connection';
import {
  rawText, formatValue, formatDateTime, isTimestampValue, generateId,
  parseEnumFromComment, stripEnumFromComment, csvCell, exportFilename
} from '@/utils/format';
import {
  detectDangerousStatements, detectSqlType, sqlLogClass,
  splitTopLevelClauses, rebuildSql, stripSqlLiterals, sqlIdent, sqlQuote,
  sqlLiteralFor, sqlLiteralForTyped, parseWhereToRows,
  parseSqlNameError, rankSimilar, queryTableNames, replaceIdent
} from '@/utils/sql';

const MODE_LABEL = { raw: '原值', timestamp: '时间', enum: '文字' };

export const useWorkspaceStore = defineStore('workspace', () => {
  const store = useStoreStore();
  const ui = useUiStore();
  const conn = useConnectionStore();

  // CodeMirror 实例：必须 shallowRef，Vue 深代理会破坏 CM 内部结构
  const editor = shallowRef(null);

  // 当前连接的 SQL 草稿：编辑器是全局单实例，切换连接时靠它换入换出
  const draftSql = ref('');

  // 查询 Tab：按连接分组（{ [connId]: { list: [{ id, name }], activeId, seq } }）
  const queryTabs = ref({});
  // 各查询 Tab 的工作区快照（非响应式，切 Tab 时换入换出）
  const tabSnapshots = {};
  // 已换入 live 状态的 Tab（非响应式镜像：连接切走后 activeTabId 会立刻变 null，拿不到来源 Tab）
  let liveTabId = null;

  const activeTabId = computed(() => {
    const g = queryTabs.value[conn.currentConnId];
    return g ? g.activeId : null;
  });
  const activeTabs = computed(() => {
    const g = queryTabs.value[conn.currentConnId];
    return g ? g.list : [];
  });

  const lastSql = ref('');
  const lastResult = ref(null); // { data, fields, colInfo }
  const loading = ref(false);   // 本次执行是否还在等数据库回包
  const refreshing = ref(false); // 刷新动作是否在进行（含最短可见时长，见 refreshResult）
  // 结果区刷新按钮的可见态：执行中变暗 + 图标转圈
  const busy = computed(() => loading.value || refreshing.value);
  const columnModes = ref({});  // 单列显式覆盖
  const globalDisplay = ref({ timestamp: false, enum: false });

  const pageSize = ref(300);
  const pageNo = ref(1);
  const pageInfo = ref({ hasMore: false, paged: true, elapsed: 0, rows: 0, idle: true, write: false, error: '' });

  const findState = ref({ keyword: '', only: true, index: -1 });
  const matchCells = ref([]); // [{ r, c }]，当前页命中的单元格

  // 查询结果历史：按连接分开保存并落盘，结果区下拉可切换回看
  const histories = ref({});     // { [connId]: [entry] }，最新的一条排在最前
  const historyView = ref(null); // 当前正在查看的历史条目 id；null 表示看最新一次执行的结果
  let liveSnapshot = null;       // 切到历史结果前暂存的最新结果，选「最新结果」时还原
  let historyPersistTimer = null;

  const historyList = computed(() => histories.value[conn.currentConnId] || []);

  // 危险操作二次确认
  const execConfirm = ref({ visible: false, kinds: [], sql: '' });
  let execConfirmResolver = null;

  // 筛选条件弹窗：初始条件行由「结果表头右键 → 筛选…」预填
  const filterDialog = ref({ visible: false, rows: [], targetCol: '' });

  // 单元格大编辑弹窗（右键「编辑内容…」）：长文本 / JSON 用，确定后写回暂存
  const cellEdit = ref({ visible: false, r: 0, c: 0, table: '', col: '', original: '', text: '' });

  // 结果表编辑：单元格修改在表内先暂存（'r:c' → { kind, value }），由工具栏「保存修改」统一生成 UPDATE 提交
  const pendingEdits = ref({});
  const hasPendingEdits = computed(() => Object.keys(pendingEdits.value).length > 0);
  const pendingEditCount = computed(() => Object.keys(pendingEdits.value).length);

  // 通用确认弹窗（保存修改 / 删除记录 / 复制为表格数据 共用），Promise 化
  const resultConfirm = ref({
    visible: false, title: '', desc: '', estimate: '', sql: '',
    checkboxLabel: '', checkboxValue: false, confirmText: '确定', danger: false
  });
  let resultConfirmResolver = null;

  // 表名 / 字段名纠错弹窗：SQL 报「找不到表/字段」时，按相似度列出候选让用户挑选，Promise 化
  const identSuggest = ref({ visible: false, kind: 'table', name: '', db: null, candidates: [] });
  let identSuggestResolver = null;

  // Insert / Update 生成面板：打开时的行列初值
  const sqlGenPanel = ref({ visible: false, mode: 'insert', rows: [], cols: [], keepDb: false });

  function init() {
    pageSize.value = parseInt(store.get('dbman_page_size'), 10) || 300;
    Object.assign(globalDisplay.value, store.get('dbman_global_display') || {});
    const hist = store.get('dbman_result_history');
    histories.value = (hist && typeof hist === 'object' && !Array.isArray(hist)) ? hist : {};
  }

  // ===== 查询结果历史 =====
  function historyLimit() {
    const n = parseInt(ui.viewOpts.resultHistoryMax, 10);
    return Number.isFinite(n) && n > 0 ? n : 100;
  }

  // 结果集可能很大，写盘开销高，这里做防抖，避免连续查询反复序列化
  function persistHistory() {
    clearTimeout(historyPersistTimer);
    historyPersistTimer = setTimeout(() => {
      store.set('dbman_result_history', JSON.parse(JSON.stringify(histories.value)));
    }, 1500);
  }

  // 执行成功后缓存一份结果（写语句 lastResult 为 null，自然跳过）
  // connId/db/src 由调用方在开跑时记下：结果回来时用户可能已经切走 Tab 或连接
  function pushHistory(sql, connId, db, src) {
    const r = src && src.lastResult;
    if (!connId || !r) return;
    const list = histories.value[connId] || (histories.value[connId] = []);
    list.unshift({
      id: generateId(),
      sql,
      db: db || '',
      time: Date.now(),
      elapsed: src.pageInfo.elapsed,
      pageNo: src.pageNo,
      pageInfo: { ...src.pageInfo },
      data: r.data,
      fields: r.fields,
      colInfo: r.colInfo
    });
    const limit = historyLimit();
    if (list.length > limit) list.splice(limit);
    persistHistory();
  }

  // 上限调小后，已缓存的历史立即裁剪到新上限
  function trimHistories() {
    const limit = historyLimit();
    let changed = false;
    Object.keys(histories.value).forEach(k => {
      const list = histories.value[k];
      if (Array.isArray(list) && list.length > limit) { list.splice(limit); changed = true; }
    });
    const cur = histories.value[conn.currentConnId] || [];
    if (historyView.value && !cur.some(e => e.id === historyView.value)) historyView.value = null;
    if (changed) persistHistory();
  }

  // 下拉切换历史结果；entryId 为空表示回到最新一次执行的结果，loadSql 表示同时载入 SQL
  function selectHistory(entryId, loadSql) {
    // 历史条目是定型快照、行序与当前结果不同：切换时清掉暂存的单元格修改，避免保存时改错行
    if (Object.keys(pendingEdits.value).length) clearPendingEdits();
    if (!entryId) {
      if (liveSnapshot) {
        lastResult.value = liveSnapshot.lastResult;
        lastSql.value = liveSnapshot.lastSql;
        pageNo.value = liveSnapshot.pageNo;
        pageInfo.value = liveSnapshot.pageInfo;
        liveSnapshot = null;
      }
      historyView.value = null;
      resetFindState();
      recomputeMatches();
      refreshResultNullables();
      return;
    }
    const entry = (histories.value[conn.currentConnId] || []).find(e => e.id === entryId);
    if (!entry) return;
    // 首次从「最新结果」切到历史时，把最新结果暂存起来
    if (historyView.value === null && lastResult.value) {
      liveSnapshot = {
        lastResult: lastResult.value,
        lastSql: lastSql.value,
        pageNo: pageNo.value,
        pageInfo: pageInfo.value
      };
    }
    lastResult.value = { data: entry.data, fields: entry.fields, colInfo: entry.colInfo };
    lastSql.value = entry.sql;
    pageNo.value = entry.pageNo;
    pageInfo.value = { ...entry.pageInfo };
    historyView.value = entry.id;
    resetFindState();
    recomputeMatches();
    if (loadSql) setSql(entry.sql);
    refreshResultNullables();
  }

  // ===== 编辑器 =====
  function setEditor(cm) { editor.value = cm; }
  function getSql() { return editor.value ? editor.value.getValue() : draftSql.value; }
  function setSql(sql) {
    draftSql.value = sql;
    if (editor.value) editor.value.setValue(sql);
  }

  // 编辑器（重新）挂载后，把当前连接的草稿灌进去
  function applyDraft() {
    if (editor.value) editor.value.setValue(draftSql.value);
  }

  // ===== 查询 Tab：状态按 Tab 存取，切换时换入换出 =====
  const IDLE_PAGE = () => ({ hasMore: false, paged: true, elapsed: 0, rows: 0, idle: true, write: false, error: '' });

  // AI store 的引用：workspace → ai 会形成循环依赖，这里由 App.vue 注入（同 bindSqlStore）
  let aiRef = null;
  function bindAiStore(s) { aiRef = s; }

  function blankTabState() {
    return {
      lastSql: '', lastResult: null, columnModes: {},
      findState: { keyword: '', only: true, index: -1 }, matchCells: [],
      pageNo: 1, pageInfo: IDLE_PAGE(), pendingEdits: {},
      historyView: null, liveSnapshot: null, sql: ''
    };
  }

  // 收集当前 live 状态为一份 Tab 快照；sql 取编辑器实时内容（不是 draftSql）
  function captureTab() {
    return {
      lastSql: lastSql.value,
      lastResult: lastResult.value,
      columnModes: columnModes.value,
      findState: findState.value,
      matchCells: matchCells.value,
      pageNo: pageNo.value,
      pageInfo: pageInfo.value,
      pendingEdits: pendingEdits.value,
      historyView: historyView.value,
      liveSnapshot,
      sql: getSql()
    };
  }

  // withSql=false：只换结果类状态，不动编辑器内容（用于查询回包后把当前 Tab 的状态换回来）
  function applyTabSnapshot(s, withSql = true) {
    const t = s || blankTabState();
    lastSql.value = t.lastSql || '';
    lastResult.value = t.lastResult || null;
    columnModes.value = t.columnModes || {};
    findState.value = t.findState || { keyword: '', only: true, index: -1 };
    matchCells.value = t.matchCells || [];
    pageNo.value = t.pageNo || 1;
    pageInfo.value = t.pageInfo || IDLE_PAGE();
    pendingEdits.value = t.pendingEdits || {};
    draftSql.value = t.sql || '';
    historyView.value = t.historyView || null;
    liveSnapshot = t.liveSnapshot || null;
    if (withSql && editor.value) editor.value.setValue(t.sql || '');
  }

  // 该连接的 Tab 组；list 为空时自动建一个「查询 1」
  function ensureTabs(connId) {
    let g = queryTabs.value[connId];
    if (!g) {
      g = { list: [], activeId: null, seq: 0 };
      queryTabs.value[connId] = g;
    }
    if (!g.list.length) {
      const id = generateId();
      g.seq = 1;
      g.list.push({ id, name: '查询 1' });
      g.activeId = id;
    }
    return g;
  }

  // 换入某个 Tab：写回它的状态、切换 live 标记，并换入该 Tab 的 AI 栏状态
  function enterTab(toId) {
    if (aiRef) aiRef.saveTabState(liveTabId); // 先存下当前 Tab 的 AI 栏状态
    applyTabSnapshot(toId ? tabSnapshots[toId] : null);
    liveTabId = toId || null;
    if (aiRef) aiRef.loadTabState(liveTabId);
  }

  // 切 Tab 前把 Tab 级弹窗收干净：确认弹窗挂着的 Promise 必须 resolve(false)，
  // 否则 savePendingEdits / execBatchDml 会永久悬挂
  function cancelTabScopedDialogs() {
    if (execConfirm.value.visible) resolveExecConfirm(false);
    if (resultConfirm.value.visible) resolveResultConfirm(false);
    if (identSuggest.value.visible) resolveIdentSuggest(null);
    if (filterDialog.value.visible) closeFilterDialog();
    if (sqlGenPanel.value.visible) closeSqlGenPanel();
    if (cellEdit.value.visible) closeCellEdit();
  }

  function switchTab(toId) {
    const fromId = liveTabId;
    if (fromId === toId) return;
    cancelTabScopedDialogs();
    if (fromId) tabSnapshots[fromId] = captureTab();
    enterTab(toId);
    // 当前连接的 Tab 组里把选中态同步过来（新建 / 点击切换都走这里）
    const g = queryTabs.value[conn.currentConnId];
    if (g) g.activeId = toId;
  }

  function activateQueryTab(id) { switchTab(id); }

  // 新建 Tab：编号接在该连接已有编号之后
  function newQueryTab() {
    const connId = conn.currentConnId;
    if (!connId) return;
    const g = ensureTabs(connId);
    g.seq += 1;
    const id = generateId();
    g.list.push({ id, name: '查询 ' + g.seq });
    switchTab(id);
  }

  // 每个连接至少保留一个 Tab；关闭当前 Tab 且有待保存修改时先问一句
  function closeQueryTab(id) {
    const g = queryTabs.value[conn.currentConnId];
    if (!g || g.list.length <= 1) return;
    if (id === liveTabId && !confirmDiscardEdits()) return;
    const idx = g.list.findIndex(t => t.id === id);
    if (idx < 0) return;
    delete tabSnapshots[id];
    if (aiRef) aiRef.dropTabState(id);
    g.list.splice(idx, 1);
    if (id !== liveTabId) return;
    // 关的是当前 Tab：接右邻，其次左邻
    const next = g.list[idx] || g.list[idx - 1];
    g.activeId = next.id;
    liveTabId = null; // 防止把已关闭 Tab 的 live 状态再存回去
    enterTab(next.id);
  }

  // 查询结果落到非当前 Tab 时，把结果类字段写回它的快照
  function stashResultTo(tabId) {
    const snap = tabSnapshots[tabId] || (tabSnapshots[tabId] = blankTabState());
    snap.lastSql = lastSql.value;
    snap.lastResult = lastResult.value;
    snap.columnModes = columnModes.value;
    snap.findState = findState.value;
    snap.matchCells = matchCells.value;
    snap.pageNo = pageNo.value;
    snap.pageInfo = pageInfo.value;
    snap.pendingEdits = pendingEdits.value;
  }

  // 供 AI 生成的后台回包使用：目标 Tab 不是当前 Tab 就只写它的快照
  function setSqlForTab(tabId, sql) {
    if (!tabId || tabId === liveTabId) { setSql(sql); return; }
    const snap = tabSnapshots[tabId] || (tabSnapshots[tabId] = blankTabState());
    snap.sql = sql;
  }

  // 切换连接：进入目标连接上次的 Tab（没有就自动建「查询 1」）
  watch(() => conn.currentConnId, (nid) => {
    if (nid) {
      switchTab(ensureTabs(nid).activeId);
    } else {
      if (liveTabId) tabSnapshots[liveTabId] = captureTab();
      applyTabSnapshot(null);
      liveTabId = null;
      if (aiRef) aiRef.loadTabState(null);
    }
  });

  // 连接关闭后一并丢弃它的所有 Tab 与快照
  watch(() => conn.openIds, ids => {
    const keep = new Set(ids);
    Object.keys(queryTabs.value).forEach(connId => {
      if (keep.has(connId)) return;
      queryTabs.value[connId].list.forEach(t => {
        delete tabSnapshots[t.id];
        if (aiRef) aiRef.dropTabState(t.id);
      });
      delete queryTabs.value[connId];
    });
  });

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
    if (!r || !r.data) return [];
    // 0 行时列元数据仍在 fields 里（空结果也要能显示表头）
    if (r.fields && r.fields.length) return r.fields.map(f => f.name);
    return r.data.length ? Object.keys(r.data[0]) : [];
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
  // 分页时多取一行，用来判断是否还有下一页（不用额外查 COUNT）
  async function fetchRows(sql, page) {
    return conn.withRetry(id => api.dbQuery(
      id, sql, conn.currentDatabase, pageSize.value + 1, (page - 1) * pageSize.value
    ));
  }

  // 收下一批行；paged=false 表示服务端没加 LIMIT（SQL 自带 LIMIT 或不是查询语句）
  function adoptRows(data, page, elapsed) {
    // 结果集换了内容：待保存的单元格修改与行/列选择都失效，一并清掉
    pendingEdits.value = {};
    if (data.write) {
      pageNo.value = page;
      pageInfo.value = { hasMore: false, paged: false, elapsed, rows: data.affected || 0, write: true, idle: false, error: '' };
      lastResult.value = null;
      resultNullables.value = {};
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
    refreshResultNullables();
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

  // 执行一段指定 SQL：整段执行与行内「执行这一行」共用同一套流程与日志
  // retried：纠错后自动重跑时置 true，避免再次进入纠错逻辑造成死循环
  async function runSql(sql, confirmed, retried) {
    if (!sql || !sql.trim()) return false;
    // 开跑时记下来源 Tab / 连接 / 库：结果回来时用户可能已经切走 Tab 或连接
    const tabId = liveTabId;
    const connId = conn.currentConnId;
    const db = conn.currentDatabase;
    const connData = conn.currentConnData;
    // 会改动数据 / 结构 / 权限或实例状态的语句，执行前必须二次确认
    if (!confirmed) {
      const kinds = detectDangerousStatements(sql);
      if (kinds.length && !(await askExecConfirm(kinds, sql))) return false;
      if (tabId !== liveTabId) return false; // 确认期间切走了 Tab，放弃这次执行
    }
    lastSql.value = sql;
    columnModes.value = {}; // 新一次查询：清空单列覆盖
    resetFindState();
    const started = Date.now();
    loading.value = true;
    try {
      const data = await fetchRows(sql, 1);
      const elapsed = Date.now() - started;
      adoptRows(data, 1, elapsed);
      const src = { lastResult: lastResult.value, pageNo: pageNo.value, pageInfo: { ...pageInfo.value } };
      if (tabId !== liveTabId) {
        // 回包时已切走：结果落到原 Tab 快照，再把当前 Tab 的状态换回来（不动编辑器内容）
        stashResultTo(tabId);
        const snap = tabSnapshots[tabId];
        snap.historyView = null;
        snap.liveSnapshot = null;
        applyTabSnapshot(tabSnapshots[liveTabId], false);
      } else {
        historyView.value = null; // 新结果回来：退出历史结果查看
        liveSnapshot = null;
        recomputeMatches();
      }
      pushHistory(sql, connId, db, src);
      addSqlLog(sql, { success: true, elapsed, rows: src.pageInfo.rows }, connData, db);
      return true;
    } catch (err) {
      const msg = err.message;
      // 表名 / 字段名写错：解析报错文案，给出相似名称让用户挑选，替换后写回编辑器并重跑（只重试一次）
      if (!retried) {
        const hit = await tryFixIdentifiers(msg, sql);
        if (hit) {
          writeBackFix(tabId, hit, sql);
          return runSql(hit.fixed, true, true);
        }
      }
      renderError(msg, tabId);
      addSqlLog(sql, { success: false, elapsed: Date.now() - started, error: msg }, connData, db);
      return false;
    } finally {
      loading.value = false;
    }
  }

  function executeQuery(confirmed) {
    return runSql(getSql(), confirmed);
  }

  // 编辑器行内「执行这一行」：直接跑传入的文本，不覆盖编辑器里的完整脚本
  function executeSqlText(sql) {
    return runSql(sql);
  }

  // 刷新按钮的最短可见时长：本地小表往往几十毫秒就返回，不兜底用户看不到任何反馈
  const REFRESH_MIN_FEEDBACK_MS = 600;

  // 刷新当前结果：重新执行「产生当前这张表的那条 SQL」，不读编辑器（用户可能已经改过编辑器内容）
  // 不清空旧数据（避免闪烁 / 慢查询时干等），完成后弹提示反馈
  async function refreshResult() {
    if (!lastSql.value || !lastSql.value.trim()) {
      ui.showToast('当前没有可刷新的查询', 'error');
      return;
    }
    if (refreshing.value) return; // 正在刷新，忽略连点
    if (!confirmDiscardEdits()) return;
    refreshing.value = true;
    const started = Date.now();
    try {
      const ok = await runSql(lastSql.value, false);
      // 查询再快也把可见态兜满最短时长，否则按钮来不及变暗就恢复了
      const rest = REFRESH_MIN_FEEDBACK_MS - (Date.now() - started);
      if (rest > 0) await new Promise(r => setTimeout(r, rest));
      if (ok) {
        const info = pageInfo.value;
        ui.showToast(info.write ? `已刷新 · 影响 ${info.rows} 行 · ${info.elapsed}ms` : `已刷新 · ${info.rows} 行 · ${info.elapsed}ms`);
      }
    } finally {
      refreshing.value = false;
    }
  }

  // 翻页 / 改每页条数：复用最近一次执行的 SQL 重新查库，保留查找状态
  async function reloadPage(page) {
    if (!lastSql.value) return;
    const tabId = liveTabId;
    const started = Date.now();
    try {
      const data = await fetchRows(lastSql.value, page);
      adoptRows(data, page, Date.now() - started);
      if (tabId !== liveTabId) {
        // 回包时已切走：结果落到原 Tab 快照，再把当前 Tab 的状态换回来
        stashResultTo(tabId);
        applyTabSnapshot(tabSnapshots[liveTabId], false);
        return;
      }
      recomputeMatches();
    } catch (err) {
      ui.showToast(err.message, 'error');
    }
  }

  // 有未保存的修改时翻页会丢弃它们，先问一句
  function confirmDiscardEdits() {
    if (!hasPendingEdits.value) return true;
    return window.confirm('当前有未保存的单元格修改，翻页会丢弃这些修改，是否继续？');
  }

  function gotoPage(page) {
    if (!pageInfo.value.paged || page < 1) return;
    if (page > pageNo.value && !pageInfo.value.hasMore) return;
    if (!confirmDiscardEdits()) return;
    return reloadPage(page);
  }

  function changePageSize(value) {
    if (!confirmDiscardEdits()) return;
    pageSize.value = parseInt(value, 10) || 300;
    store.set('dbman_page_size', String(pageSize.value));
    reloadPage(1);
  }

  // tabId：开跑时记下的来源 Tab；回包时已切走则把错误状态写回它的快照
  function renderError(msg, tabId) {
    const failed = { hasMore: false, paged: false, elapsed: 0, rows: 0, error: msg, idle: false, write: false };
    if (tabId !== liveTabId) {
      const snap = tabSnapshots[tabId] || (tabSnapshots[tabId] = blankTabState());
      snap.lastResult = null;
      snap.matchCells = [];
      snap.pageNo = 1;
      snap.pageInfo = failed;
      applyTabSnapshot(tabSnapshots[liveTabId], false);
    } else {
      lastResult.value = null;
      matchCells.value = [];
      pageNo.value = 1;
      pageInfo.value = failed;
    }
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
  // connData/db 可由调用方在开跑时记下传入：回包时当前连接可能已经切换
  function addSqlLog(sql, result, connData, db) {
    if (sqlStoreRef) sqlStoreRef.addLog(sql, result, connData || conn.currentConnData, db || conn.currentDatabase);
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
      await api.dbExport(
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

  // ===== 结果表编辑：暂存单元格修改 =====
  // 单元格 → 待改项：{ kind: 'empty' 空白字符串 | 'null' | 'value' 手输值, value?: string }
  function pendingKind(r, c) {
    const e = pendingEdits.value[r + ':' + c];
    return (e && e.kind) || null;
  }

  // kind 为 'value' 时的新值
  function pendingValue(r, c) {
    const e = pendingEdits.value[r + ':' + c];
    return e && e.kind === 'value' ? e.value : '';
  }

  // 入参归一化：兼容 'empty' | 'null' 字符串与 { kind, value } 对象
  function normalizeEdit(edit) {
    if (edit === 'empty' || edit === 'null') return { kind: edit };
    return { kind: 'value', value: String((edit && edit.value) == null ? '' : edit.value) };
  }

  // cells: [{ r, c }]，edit: 'empty' | 'null' | { kind:'value', value }
  function setPendingEdits(cells, edit) {
    if (!cells || !cells.length) return 0;
    const next = { ...pendingEdits.value };
    const e = normalizeEdit(edit);
    cells.forEach(({ r, c }) => { next[r + ':' + c] = e; });
    pendingEdits.value = next;
    return cells.length;
  }

  // 行内编辑单个单元格：写回的新值与原值相同就撤销该暂存项，避免产生无意义的 UPDATE
  function setCellEdit(r, c, text) {
    const row = (lastResult.value && lastResult.value.data[r]) || null;
    if (!row) return;
    const key = r + ':' + c;
    const same = String(text == null ? '' : text) === rawText(row[resultColumns.value[c]]);
    if (same) {
      if (key in pendingEdits.value) {
        const next = { ...pendingEdits.value };
        delete next[key];
        pendingEdits.value = next;
      }
      return;
    }
    const next = { ...pendingEdits.value };
    next[key] = { kind: 'value', value: String(text == null ? '' : text) };
    pendingEdits.value = next;
  }

  // 结果单元格是否可改：查看历史结果时条目已定型，认不出表名时无法生成 UPDATE
  function canEditCell() {
    if (historyView.value) return '正在查看历史结果，请先切回「最新结果」再修改';
    if (!resultTableRef()) return '无法识别结果对应的数据表（SQL 中缺少 FROM 表名），不能修改';
    return '';
  }

  function clearPendingEdits() {
    pendingEdits.value = {};
  }

  // 放弃修改：工具栏按钮用，确认后清掉全部暂存的单元格修改
  function discardPendingEdits() {
    if (!hasPendingEdits.value) return false;
    if (!window.confirm(`确定放弃 ${pendingEditCount.value} 个未保存的单元格修改吗？`)) return false;
    clearPendingEdits();
    return true;
  }

  // 打开单元格大编辑弹窗：以该单元格所在表、列名与原值为初值
  function openCellEdit(r, c) {
    const why = canEditCell();
    if (why) { ui.showToast(why, 'error'); return; }
    const row = (lastResult.value && lastResult.value.data[r]) || null;
    if (!row) return;
    const ref = resultTableRef();
    const col = resultColumns.value[c];
    const original = rawText(row[col]);
    cellEdit.value = {
      visible: true, r, c,
      table: ref ? qualTable(ref, false) : '',
      col, original, text: original
    };
  }

  function closeCellEdit() {
    cellEdit.value = { ...cellEdit.value, visible: false };
  }

  // 大编辑弹窗「确定」：写回暂存（值与原值相同会自动撤销该项）后关闭
  function commitCellEdit() {
    const e = cellEdit.value;
    if (e.visible) setCellEdit(e.r, e.c, e.text);
    closeCellEdit();
  }

  // ===== 通用确认弹窗 =====
  function askResultConfirm(payload) {
    return new Promise(resolve => {
      resultConfirmResolver = resolve;
      resultConfirm.value = {
        visible: true, title: '', desc: '', estimate: '', sql: '',
        checkboxLabel: '', checkboxValue: false, confirmText: '确定', danger: false,
        ...payload
      };
    });
  }

  function resolveResultConfirm(ok, checked) {
    const cur = resultConfirm.value;
    resultConfirm.value = { ...cur, visible: false };
    const resolve = resultConfirmResolver;
    resultConfirmResolver = null;
    if (resolve) resolve({ ok: !!ok, checked: checked === undefined ? cur.checkboxValue : !!checked });
  }

  // ===== 表名 / 字段名纠错弹窗 =====
  function askIdentSuggest(payload) {
    return new Promise(resolve => {
      identSuggestResolver = resolve;
      identSuggest.value = { visible: true, kind: 'table', name: '', db: null, candidates: [], ...payload };
    });
  }

  function resolveIdentSuggest(choice) {
    const cur = identSuggest.value;
    identSuggest.value = { ...cur, visible: false };
    const resolve = identSuggestResolver;
    identSuggestResolver = null;
    if (resolve) resolve(choice || null);
  }

  // ===== 表名 / 字段名智能纠错 =====
  // 当前连接的可见表名清单：优先用侧栏已加载的（零开销），为空时再查一次
  async function tableNamesForFix() {
    const local = conn.allTables || [];
    if (local.length) return local.map(t => t.tableName).filter(Boolean);
    try {
      const data = await conn.withRetry(id => api.dbTables(id, conn.currentDatabase, null));
      return (data || []).map(t => t.tableName).filter(Boolean);
    } catch (e) {
      return [];
    }
  }

  // 这批表所有列的并集（多表 JOIN 时无法定位列归属，合并后一起比对）
  async function columnsForFix(tables) {
    const out = [];
    const seen = new Set();
    const push = list => (list || []).forEach(c => {
      const name = c && (c.columnName || c.name);
      if (!name) return;
      const key = name.toLowerCase();
      if (seen.has(key)) return;
      seen.add(key);
      out.push(name);
    });
    for (const t of tables) {
      const cached = conn.tableColumns && conn.tableColumns[t];
      if (cached && cached.length) { push(cached); continue; }
      try {
        const data = await conn.withRetry(id => api.dbColumns(id, conn.currentDatabase, t));
        push(data);
      } catch (e) { /* 拿不到这张表的列，跳过 */ }
    }
    return out;
  }

  // 按相似度排序后弹窗让用户挑选；用户选了就返回替换后的 SQL 以及「错名 → 正名」
  async function suggestReplacement(info, pool, sql) {
    // 候选最多 3 个，按相似度降序（rankSimilar 已排好序）
    const candidates = rankSimilar(info.name, pool, 3);
    if (!candidates.length) return null;
    const picked = await askIdentSuggest({ kind: info.kind, name: info.name, db: info.db, candidates });
    if (!picked) return null;
    const fixed = replaceIdent(sql, info.name, picked);
    if (fixed === sql) return null;
    return { fixed, wrong: info.name, right: picked };
  }

  // 纠正结果回写编辑器。本次执行的 SQL 恰好等于编辑器全文时整篇替换；
  // 行内执行 / 刷新按钮（执行的 SQL 未必等于编辑器内容）时只精确替换那一个标识符，不覆盖用户正在写的内容
  function writeBackFix(tabId, hit, executedSql) {
    const current = tabId === liveTabId ? getSql() : ((tabSnapshots[tabId] || {}).sql || '');
    const next = current === executedSql ? hit.fixed : replaceIdent(current, hit.wrong, hit.right);
    if (next && next !== current) setSqlForTab(tabId, next);
  }

  // 解析报错里点名的表名 / 字段名，给出 { fixed, wrong, right }；无法纠正或用户取消时返回 null
  async function tryFixIdentifiers(msg, sql) {
    const info = parseSqlNameError(msg);
    if (!info) return null;
    const known = await tableNamesForFix();
    if (!known.length) return null;

    if (info.kind === 'table') {
      // 本地清单里已有这张表 → 不是名字写错（可能是权限/库选错），不提示
      if (known.some(n => n.toLowerCase() === info.name.toLowerCase())) return null;
      return suggestReplacement(info, known, sql);
    }

    // 字段名：先确认引用到的表名都是对的，再拿这些表的列做相似度比对
    const tables = queryTableNames(sql);
    if (!tables.length) return null;
    const knownSet = new Set(known.map(n => n.toLowerCase()));
    if (!tables.every(t => knownSet.has(t.toLowerCase()))) return null;
    const columns = await columnsForFix(tables);
    if (!columns.length) return null;
    // 列其实存在 → 报错另有原因（如表别名限定），不误判成写错
    if (columns.some(c => c.toLowerCase() === info.name.toLowerCase())) return null;
    return suggestReplacement(info, columns, sql);
  }

  // ===== 结果集对应的数据表 / 主键 =====
  // 从最近一次 SQL 的 FROM 子句解析目标表（支持 `db`.`tbl` / db.tbl / tbl）
  function resultTableRef() {
    const parsed = splitTopLevelClauses(lastSql.value);
    if (!parsed || !parsed.body) return null;
    const m = parsed.body.match(/\bfrom\s+((?:`[^`]+`|[A-Za-z_$][\w$]*)(?:\s*\.\s*(?:`[^`]+`|[A-Za-z_$][\w$]*))?)/i);
    if (!m) return null;
    const parts = m[1].split('.').map(s => s.trim().replace(/^`|`$/g, '').replace(/``/g, '`'));
    const table = parts[parts.length - 1];
    if (!table) return null;
    const db = parts.length > 1 ? parts[0] : (conn.currentDatabase || '');
    return { db, table, key: (db ? db + '.' : '') + table };
  }

  function qualTable(ref, keepDb) {
    const t = sqlIdent(ref.table);
    return keepDb && ref.db ? `${sqlIdent(ref.db)}.${t}` : t;
  }

  // 表结构（含列的 columnKey，用主键定位记录），按「连接 + 库.表」缓存避免重复请求
  const structureCache = {};
  // 结果集对应表各列是否允许 NULL（来自表结构）：{ [列名]: bool }
  // 用于右键菜单里把 NOT NULL 字段的「设置为 NULL」置灰；拿不到结构时为空
  const resultNullables = ref({});

  async function loadResultStructure(ref) {
    if (!ref) return null;
    const key = (conn.currentConnId || '') + '|' + ref.key;
    if (key in structureCache) return structureCache[key];
    let st = null;
    try {
      st = await conn.withRetry(id => api.dbTableStructure(
        id, ref.db || conn.currentDatabase || '', ref.table
      ));
    } catch (e) {
      st = null;
    }
    structureCache[key] = st;
    return st;
  }

  // 结果集换内容后刷新一次各列的可空信息；拿不到表结构就留空（菜单保持可用，不误禁）
  async function refreshResultNullables() {
    const ref = resultTableRef();
    if (!ref) { resultNullables.value = {}; return; }
    const st = await loadResultStructure(ref);
    // 期间结果可能已切到别的表，别把旧结构写进来
    const cur = resultTableRef();
    if (!cur || cur.key !== ref.key) return;
    const map = {};
    if (st && st.columns) st.columns.forEach(c => { map[c.name] = c.nullable !== false; });
    resultNullables.value = map;
  }

  // 该列是否允许写 NULL；未知（没有表结构）时返回 true，保持菜单可用
  function columnNullable(col) {
    const m = resultNullables.value;
    return (col in m) ? m[col] : true;
  }

  function pkColumnsOf(structure) {
    if (!structure || !structure.columns) return [];
    return structure.columns.filter(c => String(c.columnKey || '').toUpperCase() === 'PRI').map(c => c.name);
  }

  // 一行 → WHERE 条件（cols 指定参与比较的列，NULL 用 IS NULL）
  function rowWhereClause(row, cols, dataRows) {
    return cols.map(col => {
      const v = row[col];
      if (v === null || v === undefined) return `${sqlIdent(col)} IS NULL`;
      return `${sqlIdent(col)} = ${sqlLiteralFor(col, rawText(v), dataRows)}`;
    }).join(' AND ');
  }

  function resultData() {
    return (lastResult.value && lastResult.value.data) || [];
  }

  // 新增列的取值字面量：纯数字不加引号，其余按字符串转义
  function literalForRaw(v) {
    const s = String(v == null ? '' : v);
    return /^-?\d+(\.\d+)?$/.test(s.trim()) ? s.trim() : sqlQuote(s);
  }

  // 无主键时：用全列原值条件预估受影响的记录数，供用户判断是否会误改数据
  async function estimateAffected(ref, rowIdxs, cols) {
    const data = resultData();
    const conds = rowIdxs
      .map(r => data[r])
      .filter(Boolean)
      .map(row => `(${rowWhereClause(row, cols, data)})`);
    if (!conds.length) return null;
    try {
      const sql = `SELECT COUNT(*) AS c FROM ${qualTable(ref, false)} WHERE ${conds.join(' OR ')}`;
      const res = await conn.withRetry(id => api.dbQuery(
        id, sql, ref.db || conn.currentDatabase, 0, 0
      ));
      const first = (res.rows && res.rows[0]) || null;
      return first ? Number(first.c) : null;
    } catch (e) {
      return null;
    }
  }

  // 批量执行多条 DML（db_exec_script 原生支持多语句），成功后刷新当前页并清空暂存
  async function execBatchDml(statements, label, confirmed) {
    const sqlText = statements.join('\n');
    const tabId = liveTabId;
    const connData = conn.currentConnData;
    const db = conn.currentDatabase;
    if (!confirmed) {
      const kinds = detectDangerousStatements(sqlText);
      if (kinds.length && !(await askExecConfirm(kinds, sqlText))) return false;
      if (tabId !== liveTabId) return false; // 确认期间切走了 Tab
    }
    const started = Date.now();
    try {
      await conn.withRetry(id => api.dbExecScript(id, conn.currentDatabase || '', sqlText));
      addSqlLog(sqlText, { success: true, elapsed: Date.now() - started, rows: statements.length }, connData, db);
      if (tabId === liveTabId) {
        clearPendingEdits();
        await reloadPage(pageNo.value);
      } else if (tabSnapshots[tabId]) {
        // 执行期间切走了 Tab：只清原 Tab 的待保存标记，不去刷新当前 Tab
        tabSnapshots[tabId].pendingEdits = {};
      }
      ui.showToast(`${label}成功，共 ${statements.length} 条`, 'success');
      return true;
    } catch (err) {
      addSqlLog(sqlText, { success: false, elapsed: Date.now() - started, error: err.message }, connData, db);
      ui.showToast(`${label}失败：${err.message}`, 'error');
      return false;
    }
  }

  // 保存修改：把表内暂存的单元格修改汇总成多条 UPDATE
  async function savePendingEdits() {
    const keys = Object.keys(pendingEdits.value);
    if (!keys.length) { ui.showToast('没有待保存的修改', 'error'); return false; }
    const why = canEditCell();
    if (why) { ui.showToast(why, 'error'); return false; }
    const ref = resultTableRef();
    if (!ref) { ui.showToast('无法识别结果对应的数据表（SQL 中缺少 FROM 表名）', 'error'); return false; }
    const data = resultData();
    const cols = resultColumns.value;
    // 按行汇总：该行要改哪几列
    const byRow = {};
    keys.forEach(k => {
      const [r, c] = k.split(':');
      (byRow[r] || (byRow[r] = [])).push(Number(c));
    });
    const rowIdxs = Object.keys(byRow).map(Number).sort((a, b) => a - b);
    const structure = await loadResultStructure(ref);
    const pkCols = pkColumnsOf(structure);
    const wCols = pkCols.length ? pkCols : cols;
    const statements = rowIdxs.map(r => {
      const row = data[r];
      const sets = byRow[r].map(c => {
        const col = cols[c];
        const e = pendingEdits.value[r + ':' + c];
        if (e.kind === 'null') return `${sqlIdent(col)} = NULL`;
        if (e.kind === 'empty') return `${sqlIdent(col)} = ''`;
        return `${sqlIdent(col)} = ${sqlLiteralForTyped(col, e.value, data)}`;
      });
      return `UPDATE ${qualTable(ref, false)} SET ${sets.join(', ')} WHERE ${rowWhereClause(row, wCols, data)};`;
    });
    let estimate = '';
    if (pkCols.length) {
      estimate = `按主键（${pkCols.join(', ')}）定位记录`;
    } else {
      const n = await estimateAffected(ref, rowIdxs, cols);
      estimate = n === null
        ? '该表没有主键，将使用所有列的原值作为条件'
        : `该表没有主键，将使用所有列的原值作为条件；预计影响 ${n} 行`;
    }
    const picked = await askResultConfirm({
      title: '保存修改',
      desc: `共 ${rowIdxs.length} 行、${keys.length} 个单元格待修改，将生成 ${statements.length} 条 UPDATE。`,
      estimate,
      sql: statements.join('\n'),
      confirmText: '执行修改',
      danger: true
    });
    if (!picked.ok) return false;
    return execBatchDml(statements, '保存修改', true);
  }

  // 删除记录：按选中行生成 DELETE（主键优先，无主键用全列原值并给出预估）
  async function deleteResultRows(rowIdxs) {
    const rows = (rowIdxs || []).filter(r => r !== null && r !== undefined);
    if (!rows.length) { ui.showToast('请先选择要删除的行', 'error'); return false; }
    const why = canEditCell();
    if (why) { ui.showToast(why, 'error'); return false; }
    const ref = resultTableRef();
    if (!ref) { ui.showToast('无法识别结果对应的数据表（SQL 中缺少 FROM 表名）', 'error'); return false; }
    const data = resultData();
    const cols = resultColumns.value;
    const structure = await loadResultStructure(ref);
    const pkCols = pkColumnsOf(structure);
    const wCols = pkCols.length ? pkCols : cols;
    const sorted = rows.slice().sort((a, b) => a - b);
    const statements = sorted
      .filter(r => data[r])
      .map(r => `DELETE FROM ${qualTable(ref, false)} WHERE ${rowWhereClause(data[r], wCols, data)} LIMIT 1;`);
    if (!statements.length) { ui.showToast('没有可删除的行', 'error'); return false; }
    let estimate = '';
    if (pkCols.length) {
      estimate = `按主键（${pkCols.join(', ')}）定位记录`;
    } else {
      const n = await estimateAffected(ref, sorted, cols);
      estimate = n === null
        ? '该表没有主键，将使用所有列的原值作为条件'
        : `该表没有主键，将使用所有列的原值作为条件；预计影响 ${n} 行`;
    }
    const picked = await askResultConfirm({
      title: '删除记录',
      desc: `将删除选中的 ${statements.length} 行，生成 ${statements.length} 条 DELETE。`,
      estimate,
      sql: statements.join('\n'),
      confirmText: '确认删除',
      danger: true
    });
    if (!picked.ok) return false;
    return execBatchDml(statements, '删除记录', true);
  }

  // 复制为表格数据（Tab 分隔，可直接粘贴到 Excel / CSV）
  async function copyResultAsTsv(rowIdxs, colNames) {
    const data = resultData();
    const allCols = resultColumns.value;
    const cols = (colNames && colNames.length) ? allCols.filter(c => colNames.includes(c)) : allCols;
    const rows = (rowIdxs && rowIdxs.length) ? rowIdxs.slice().sort((a, b) => a - b) : [];
    if (!rows.length || !cols.length) { ui.showToast('请先选择要复制的行和列', 'error'); return false; }
    const picked = await askResultConfirm({
      title: '复制为表格数据',
      desc: `将把选中的 ${rows.length} 行 × ${cols.length} 列复制为制表符（Tab）分隔文本，可直接粘贴到 Excel 或另存为 CSV。`,
      checkboxLabel: '包含表头（字段名）',
      checkboxValue: true,
      confirmText: '复制'
    });
    if (!picked.ok) return false;
    const clean = v => String(v == null ? '' : v).replace(/[\t\r\n]/g, ' ');
    const lines = [];
    if (picked.checked) lines.push(cols.map(clean).join('\t'));
    rows.forEach(r => lines.push(cols.map(col => clean(rawText(data[r][col]))).join('\t')));
    try {
      await navigator.clipboard.writeText(lines.join('\n'));
      ui.showToast(`已复制 ${rows.length} 行 × ${cols.length} 列到剪贴板`, 'success');
      return true;
    } catch (err) {
      ui.showToast('复制失败：' + err.message, 'error');
      return false;
    }
  }

  // ===== Insert / Update 生成面板 =====
  function openSqlGenPanel(mode, rowIdxs, colNames) {
    const cols = (colNames && colNames.length) ? colNames : resultColumns.value;
    const rows = (rowIdxs && rowIdxs.length) ? rowIdxs.slice().sort((a, b) => a - b) : visibleRowIndexes.value.slice();
    sqlGenPanel.value = { visible: true, mode: mode === 'update' ? 'update' : 'insert', rows, cols, keepDb: false };
  }

  function closeSqlGenPanel() {
    sqlGenPanel.value = { ...sqlGenPanel.value, visible: false };
  }

  // 面板上下文：表定位 + 主键列 + 生成列（生成列不能显式写入）
  async function sqlGenContext() {
    const ref = resultTableRef();
    if (!ref) return null;
    const structure = await loadResultStructure(ref);
    const generated = new Set();
    if (structure && structure.columns) {
      structure.columns.forEach(c => {
        const extra = String(c.extra || '').toLowerCase();
        if (extra.includes('generated') || c.generationExpression) generated.add(c.name);
      });
    }
    return { ref, pkCols: pkColumnsOf(structure), generated };
  }

  // opts: { keepDb, overrides: { col: { on, asNull, value } }, extraCols: [{ name, value, asNull }] }
  function columnValue(row, col, overrides, data) {
    const ov = (overrides || {})[col];
    if (ov && ov.on) return ov.asNull ? 'NULL' : sqlLiteralFor(col, ov.value, data);
    const v = row[col];
    return v === null || v === undefined ? 'NULL' : sqlLiteralFor(col, rawText(v), data);
  }

  function buildInsertSql(ctx, colNames, rowIdxs, opts) {
    const data = resultData();
    const cols = colNames.filter(c => !ctx.generated.has(c));
    const extra = (opts.extraCols || []).filter(x => x.name);
    if (!cols.length && !extra.length) return '';
    const names = [...cols, ...extra.map(x => x.name)].map(sqlIdent).join(', ');
    const tbl = qualTable(ctx.ref, opts.keepDb);
    return rowIdxs.map(r => {
      const row = data[r];
      const vals = cols.map(c => columnValue(row, c, opts.overrides, data));
      extra.forEach(x => vals.push(x.asNull ? 'NULL' : literalForRaw(x.value)));
      return `INSERT INTO ${tbl} (${names}) VALUES (${vals.join(', ')});`;
    }).join('\n');
  }

  function buildUpdateSql(ctx, colNames, rowIdxs, opts) {
    const data = resultData();
    const cols = colNames.filter(c => !ctx.generated.has(c));
    const extra = (opts.extraCols || []).filter(x => x.name);
    if (!cols.length && !extra.length) return '';
    const wCols = ctx.pkCols.length ? ctx.pkCols : resultColumns.value;
    const tbl = qualTable(ctx.ref, opts.keepDb);
    return rowIdxs.map(r => {
      const row = data[r];
      const sets = cols.map(c => `${sqlIdent(c)} = ${columnValue(row, c, opts.overrides, data)}`);
      extra.forEach(x => sets.push(`${sqlIdent(x.name)} = ${x.asNull ? 'NULL' : literalForRaw(x.value)}`));
      return `UPDATE ${tbl} SET ${sets.join(', ')} WHERE ${rowWhereClause(row, wCols, data)};`;
    }).join('\n');
  }

  return {
    editor, draftSql, lastSql, lastResult, busy, columnModes, globalDisplay,
    queryTabs, activeTabId, activeTabs, newQueryTab, activateQueryTab, closeQueryTab, setSqlForTab,
    pageSize, pageNo, pageInfo, findState, matchCells, matchCount,
    execConfirm, filterDialog,
    pendingEdits, hasPendingEdits, pendingEditCount, pendingKind, pendingValue, canEditCell,
    setPendingEdits, setCellEdit, clearPendingEdits, discardPendingEdits, savePendingEdits,
    cellEdit, openCellEdit, closeCellEdit, commitCellEdit,
    resultConfirm, askResultConfirm, resolveResultConfirm,
    identSuggest, askIdentSuggest, resolveIdentSuggest,
    sqlGenPanel, openSqlGenPanel, closeSqlGenPanel, sqlGenContext,
    buildInsertSql, buildUpdateSql,
    resultTableRef, deleteResultRows, copyResultAsTsv,
    resultNullables, columnNullable, refreshResultNullables,
    histories, historyList, historyView, selectHistory, trimHistories,
    resultColumns, columnModesForRender, visibleRowIndexes,
    MODE_LABEL,
    init, setEditor, getSql, setSql, setSqlAndRun, applyDraft,
    currentColComments, effectiveColumnMode, applyDisplayMode, analyzeColumns,
    toggleGlobalMode, toggleColumnMode,
    rowHit, cellHit, recomputeMatches,
    executeQuery, executeSqlText, refreshResult, reloadPage, gotoPage, changePageSize,
    askExecConfirm, resolveExecConfirm,
    rewriteEditorSql, sortResultBy, clearResultSort, clearResultFilter,
    openFilterDialog, closeFilterDialog, applyFilter,
    applyFind, toggleFindOnly, clearFind, resetFindState,
    bindSqlStore, addSqlLog,
    bindAiStore,
    exportResultCols, buildExportHeaders, exportValue,
    exportCurrentPage, exportAllRows
  };
});
