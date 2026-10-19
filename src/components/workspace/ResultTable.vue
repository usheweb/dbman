<script setup>
// 查询结果表格：错误 / 写结果 / 空结果 / 数据表 + 行号列 / 多行多列选择 + 表头与单元格右键菜单
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue';
import { useWorkspaceStore } from '@/stores/workspace';
import { useUiStore } from '@/stores/ui';
import { rawText } from '@/utils/format';
import { splitTopLevelClauses } from '@/utils/sql';
import ContextMenu from '@/components/common/ContextMenu.vue';
import ResultStatements from './ResultStatements.vue';

const ws = useWorkspaceStore();
const ui = useUiStore();

const rootEl = ref(null);
const menu = ref({ visible: false, x: 0, y: 0, items: [], target: '', value: '', r: -1, c: -1 });

// 多行多列选择：单元格 (r,c) 被选中 ⇔ r ∈ selRows 且 列名 ∈ selCols
const selRows = ref([]);
const selCols = ref([]);
const cellAnchor = ref(null); // { r, c }：Shift 连选的锚点
const rowAnchor = ref(null);  // 行号列锚点
const colAnchor = ref(null);  // 表头锚点

const columns = computed(() => ws.resultColumns);
const colInfo = computed(() => (ws.lastResult && ws.lastResult.colInfo) || {});
const colComments = computed(() => ws.currentColComments());
const modes = computed(() => ws.columnModesForRender);

const selRowSet = computed(() => new Set(selRows.value));
const selColSet = computed(() => new Set(selCols.value));
const allRowIdxs = computed(() => ws.visibleRowIndexes);

// 命中单元格位置映射：`r:c` → 在 matchCells 里的下标（DOM 顺序与之一致）
const matchIndexMap = computed(() => {
  const map = {};
  ws.matchCells.forEach((cell, i) => { map[cell.r + ':' + cell.c] = i; });
  return map;
});

// 勾选过滤时当前页没有可显示的行
const noVisibleRows = computed(() =>
  !!(ws.lastResult && ws.lastResult.data.length)
  && ws.findState.only
  && !!ws.findState.keyword
  && ws.visibleRowIndexes.length === 0
);

// 结果集换了内容（新查询 / 翻页 / 改每页条数）：行索引已失效，清空选择
watch(() => ws.lastResult, () => {
  selRows.value = [];
  selCols.value = [];
  cellAnchor.value = null;
  rowAnchor.value = null;
  colAnchor.value = null;
});

function isConvertible(col) {
  const info = colInfo.value[col];
  return !!(info && (info.canTimestamp || info.enumMap));
}

function isCellSelected(r, c) {
  return selRowSet.value.has(r) && selColSet.value.has(columns.value[c]);
}

function isRowSelected(r) {
  return selRowSet.value.has(r);
}

function cellMidx(r, c) {
  const k = r + ':' + c;
  return Object.prototype.hasOwnProperty.call(matchIndexMap.value, k) ? matchIndexMap.value[k] : null;
}

// list 中 a..b 之间（含端点）的元素；任一端不在 list 里就退化为只选 b
function rangeIn(list, a, b) {
  const ia = list.indexOf(a);
  const ib = list.indexOf(b);
  if (ia === -1 || ib === -1) return [b];
  const lo = Math.min(ia, ib);
  const hi = Math.max(ia, ib);
  return list.slice(lo, hi + 1);
}

// ===== 选择：单元格 / 行号 / 表头 =====
function onClickCell(e, r, c) {
  if (dragMoved) return; // 刚才是拖动框选，不再按单击处理
  const col = columns.value[c];
  if (e.shiftKey && cellAnchor.value) {
    selRows.value = rangeIn(allRowIdxs.value, cellAnchor.value.r, r);
    selCols.value = rangeIn(columns.value, columns.value[cellAnchor.value.c], col);
    return;
  }
  if (e.metaKey || e.ctrlKey) {
    const rs = new Set(selRows.value); rs.add(r);
    const cs = new Set(selCols.value); cs.add(col);
    selRows.value = allRowIdxs.value.filter(x => rs.has(x));
    selCols.value = columns.value.filter(x => cs.has(x));
  } else {
    selRows.value = [r];
    selCols.value = [col];
  }
  cellAnchor.value = { r, c };
}

function onClickRowHead(e, r) {
  if (dragMoved) return; // 刚才是拖动框选，不再按单击处理
  if (e.shiftKey && rowAnchor.value !== null) {
    selRows.value = rangeIn(allRowIdxs.value, rowAnchor.value, r);
    selCols.value = columns.value.slice();
    return;
  }
  if (e.metaKey || e.ctrlKey) {
    const rs = new Set(selRows.value);
    if (rs.has(r)) rs.delete(r); else rs.add(r);
    selRows.value = allRowIdxs.value.filter(x => rs.has(x));
  } else {
    selRows.value = [r];
  }
  selCols.value = columns.value.slice();
  rowAnchor.value = r;
}

function onClickHeader(e, col, c) {
  if (e.shiftKey && colAnchor.value !== null) {
    selCols.value = rangeIn(columns.value, columns.value[colAnchor.value], col);
  } else if (e.metaKey || e.ctrlKey) {
    const cs = new Set(selCols.value);
    if (cs.has(col)) cs.delete(col); else cs.add(col);
    selCols.value = columns.value.filter(x => cs.has(x));
  } else {
    selCols.value = [col];
  }
  selRows.value = allRowIdxs.value.slice();
  colAnchor.value = c;
}

const selRowCount = computed(() => selRows.value.length);
const selColCount = computed(() => selCols.value.length);

// ===== 表头排序箭头（支持多列叠加） =====
// 点一下把该列按升序「加入」排序，再点切降序，第三次把这一列移出排序；先点的列排在前面的
// 顺位，也就是主排序键。排序走的仍是「改写编辑器 SQL 的 ORDER BY 再重跑」那一套，
// 箭头亮哪几列、什么方向、第几顺位，都由 SQL 里的 ORDER BY 反推（ws.resultSort）
function sortDirOf(col) {
  const s = ws.resultSort.find(x => x.col === col);
  return s ? s.dir : '';
}

// 该列在排序列表里的顺位（1 起，0 表示没参与排序）
function sortRankOf(col) {
  return ws.resultSort.findIndex(x => x.col === col) + 1;
}

function sortTipOf(col) {
  const dir = sortDirOf(col);
  const cur = ws.resultSort.map(s => s.col + (s.dir === 'ASC' ? ' 升序' : ' 降序'));
  const head = cur.length ? '当前排序：' + cur.join('，') + '；' : '';
  if (dir === 'ASC') return head + '点击「' + col + '」改为降序，再点一次移出排序';
  if (dir === 'DESC') return head + '点击「' + col + '」移出排序';
  return head + '点击按「' + col + '」升序加入排序（可多列叠加，先点的排前面）';
}

function onSortClick(col) {
  ws.toggleSort(col);
}

// ===== 拖动框选：按住左键划过单元格 / 行号列，按矩形范围选中 =====
const dragging = ref(false);
const dragKind = ref('');      // 'cell' | 'row'
let dragMoved = false;         // 本次拖动是否真的划过别的格子（用来吞掉紧随其后的 click）

function startDrag(kind) {
  dragging.value = true;
  dragKind.value = kind;
}

function onCellDown(e, r, c) {
  if (e.button !== 0) return;
  e.preventDefault(); // 拦住原生文本选中，避免拖动时拉出蓝色文字高亮
  dragMoved = false;  // 每次按下都重置
  if (e.shiftKey || e.metaKey || e.ctrlKey) return; // 组合键的扩选交给 click 处理
  startDrag('cell');
  cellAnchor.value = { r, c };
  selRows.value = [r];
  selCols.value = [columns.value[c]];
}

function onCellEnter(r, c) {
  if (!dragging.value || dragKind.value !== 'cell' || !cellAnchor.value) return;
  dragMoved = true;
  selRows.value = rangeIn(allRowIdxs.value, cellAnchor.value.r, r);
  selCols.value = rangeIn(columns.value, columns.value[cellAnchor.value.c], columns.value[c]);
}

function onRowHeadDown(e, r) {
  if (e.button !== 0) return;
  e.preventDefault();
  dragMoved = false; // 每次按下都重置
  if (e.shiftKey || e.metaKey || e.ctrlKey) return;
  startDrag('row');
  rowAnchor.value = r;
  selRows.value = [r];
  selCols.value = columns.value.slice();
}

function onRowHeadEnter(r) {
  if (!dragging.value || dragKind.value !== 'row' || rowAnchor.value === null) return;
  dragMoved = true;
  selRows.value = rangeIn(allRowIdxs.value, rowAnchor.value, r);
}

// 鼠标抬到表格外面也要正常结束拖动
function onDragEnd() {
  if (!dragging.value) return;
  dragging.value = false;
  dragKind.value = '';
}

onMounted(() => document.addEventListener('mouseup', onDragEnd));
onBeforeUnmount(() => {
  document.removeEventListener('mouseup', onDragEnd);
  window.removeEventListener('mousemove', onResizeMove);
  window.removeEventListener('mouseup', endResize);
  document.body.style.cursor = '';
});

// ===== 列宽拖动：拖表头右边缘调整列宽（按列名记住，翻页 / 刷新后沿用） =====
const MIN_COL_W = 48;
const colWidths = ref({});  // 列名 → 宽度(px)
const resizing = ref(null); // { col, startX, startW }，非 null 表示正在拖

// 同时钉住 width / min-width / max-width：表格是自动布局，只写 width 时内容会把列顶宽，
// 只写 max-width 时内容不足又会缩回去，三个一起写列才稳定停在拖到的宽度
function colStyle(col) {
  const w = colWidths.value[col];
  if (!w) return null;
  const px = w + 'px';
  return { width: px, minWidth: px, maxWidth: px };
}

function startResize(e, col) {
  const th = e.currentTarget.closest('th');
  if (!th) return;
  resizing.value = { col, startX: e.clientX, startW: Math.round(th.getBoundingClientRect().width) };
  document.body.style.cursor = 'col-resize'; // 拖出表头范围后光标依然是拖动样式
  window.addEventListener('mousemove', onResizeMove);
  window.addEventListener('mouseup', endResize);
}

function onResizeMove(e) {
  const st = resizing.value;
  if (!st) return;
  const next = Math.max(MIN_COL_W, st.startW + e.clientX - st.startX);
  colWidths.value = { ...colWidths.value, [st.col]: next };
}

function endResize() {
  if (!resizing.value) return;
  resizing.value = null;
  document.body.style.cursor = '';
  window.removeEventListener('mousemove', onResizeMove);
  window.removeEventListener('mouseup', endResize);
}

// ===== 待暂存的单元格修改 =====
function pendingKind(r, c) {
  return ws.pendingKind(r, c);
}

// 整表所有列被选中（点行号选整行时就是这样）。这种情况下单元格高亮交给
// <tr>.all-cols + CSS 表达，不再给每个单元格改 class：否则一次选中要让
// 300 行 × N 列的单元格全部重算 class，宽表下会明显卡顿
const allColsSelected = computed(
  () => columns.value.length > 0 && selColSet.value.size === columns.value.length
);

function cellClass(row, col, r, c) {
  const kind = pendingKind(r, c);
  return {
    'cell-match': ws.cellHit(row, col),
    // 这里只表达「列」这一维；「行」由 <tr>.row-selected 表达，CSS 里两者同时成立才算选中
    'cell-selected': !allColsSelected.value && selColSet.value.has(col),
    'cell-active': cellMidx(r, c) === ws.findState.index,
    'cell-pending-empty': kind === 'empty',
    'cell-pending-null': kind === 'null',
    'cell-pending-value': kind === 'value',
    // 库里的真值 NULL 单独标记（灰色斜体），与空字符串的空白区分开
    'cell-null': !kind && (row[col] === null || row[col] === undefined)
  };
}

// 待改单元格直接显示将写入的值（'' / NULL / 手输的新值），便于提交前核对
function cellText(row, col, r, c) {
  const kind = pendingKind(r, c);
  if (kind === 'null') return 'NULL';
  if (kind === 'empty') return "''";
  if (kind === 'value') return ws.pendingValue(r, c);
  return ws.applyDisplayMode(row[col], modes.value[col], colInfo.value[col]);
}

function cellTitle(row, col, r, c) {
  const kind = pendingKind(r, c);
  if (kind === 'empty') return '待保存：设置为空白字符串';
  if (kind === 'null') return '待保存：设置为 NULL';
  if (kind === 'value') return '待保存：' + ws.pendingValue(r, c);
  return '';
}

// ===== 行内编辑：双击单元格就地改，输入即写回暂存 =====
const editing = ref(null);   // { r, c }，null 表示没有单元格处于编辑态
const editValue = ref('');
let editorEl = null;

function startEdit(r, c) {
  const why = ws.canEditCell();
  if (why) { ui.showToast(why, 'error'); return; }
  const row = ws.lastResult && ws.lastResult.data[r];
  if (!row) return;
  editing.value = { r, c };
  editValue.value = rawText(row[columns.value[c]]);
  nextTick(() => {
    if (editorEl) { editorEl.focus(); editorEl.select(); }
  });
}

// 输入即写回：不等 blur（WKWebView 下点按钮常常不触发 blur，会静默丢输入）
function onEditInput(r, c, e) {
  editValue.value = e.target.value;
  ws.setCellEdit(r, c, editValue.value);
}

function endEdit() {
  editing.value = null;
}

function cancelEdit(r, c) {
  const row = ws.lastResult && ws.lastResult.data[r];
  if (row) ws.setCellEdit(r, c, rawText(row[columns.value[c]]));
  editing.value = null;
}

// 点编辑框以外的任何位置都要收起输入框。
// 表格里各处 mousedown 都调了 preventDefault（拦原生文本选中 / 拖动框选），
// 而「焦点转移到被点的元素上」正是 mousedown 的默认行为之一——被 preventDefault 之后
// input 就不失焦了，@blur 永远不触发，输入框会一直赖在单元格上。
// 所以在 document 的捕获阶段兜底：命中点不在编辑框内就直接收起编辑态
// （值在输入时已实时写回暂存，收起不会丢内容）。
function onDocMouseDown(e) {
  if (!editing.value) return;
  if (editorEl && editorEl.contains(e.target)) return;
  endEdit();
}

onMounted(() => document.addEventListener('mousedown', onDocMouseDown, true));
onBeforeUnmount(() => document.removeEventListener('mousedown', onDocMouseDown, true));

// 切查询 Tab / 换结果集时结束编辑态（值已实时写回，不会丢）
watch(() => [ws.activeTabId, ws.lastResult], () => { editing.value = null; });

// ===== 右键菜单 =====
function headerItems() {
  const parsed = splitTopLevelClauses(ws.getSql());
  const hasSort = !!(parsed && parsed.order);
  const hasWhere = !!(parsed && parsed.where);
  const items = [
    { label: '仅按此列升序', value: 'sort-asc' },
    { label: '仅按此列降序', value: 'sort-desc' },
    { label: '筛选', value: 'filter' },
    // 手写 SQL 时要引用某列，直接从结果表头拿名字，省得再去翻表结构
    { separator: true },
    { label: '插入列名到编辑器', value: 'insert-col' }
  ];
  if (hasSort || hasWhere) items.push({ separator: true });
  if (hasSort) items.push({ label: '清除排序', value: 'clear-sort', danger: true });
  if (hasWhere) items.push({ label: '清除筛选', value: 'clear-where', danger: true });
  return items;
}

// 选中的列里存在 NOT NULL 字段时，「设置为 NULL」提交必然失败，置灰该项
function nullDisabled() {
  return columns.value.some(col => selColSet.value.has(col) && ws.columnNullable(col) === false);
}

// 行号列菜单：没有具体的单元格值，去掉「复制」这一项
function rowHeadItems() {
  return [
    { label: '设置为空白字符串', value: 'set-empty' },
    { label: '设置为 NULL', value: 'set-null', disabled: nullDisabled() },
    { separator: true },
    { label: '复制为表格数据（Tab 分隔）', value: 'copy-tsv' },
    { label: '复制为 Insert 语句', value: 'copy-insert' },
    { label: '复制为 Update 语句', value: 'copy-update' },
    { separator: true },
    { label: '删除记录', value: 'delete-rows', danger: true }
  ];
}

function cellItems() {
  return [
    { label: '复制', value: 'copy' },
    { label: '查看内容', value: 'view' },
    { label: '编辑内容', value: 'edit' },
    { separator: true },
    { label: '设置为空白字符串', value: 'set-empty' },
    { label: '设置为 NULL', value: 'set-null', disabled: nullDisabled() },
    { separator: true },
    { label: '复制为表格数据（Tab 分隔）', value: 'copy-tsv' },
    { label: '复制为 Insert 语句', value: 'copy-insert' },
    { label: '复制为 Update 语句', value: 'copy-update' },
    { separator: true },
    { label: '删除记录', value: 'delete-rows', danger: true }
  ];
}

function showCellMenu(e, r, c, value) {
  // 右键落在选区外时把选区收敛到该单元格，保证后续操作目标明确
  if (!isCellSelected(r, c)) {
    selRows.value = [r];
    selCols.value = [columns.value[c]];
    cellAnchor.value = { r, c };
  }
  menu.value = { visible: true, x: e.clientX, y: e.clientY, items: cellItems(), target: '', value, r, c };
}

function showRowHeadMenu(e, r) {
  // 右键行号时若该行不在选区里，就把选区收敛到这一行
  if (!isRowSelected(r)) {
    selRows.value = [r];
    selCols.value = columns.value.slice();
    rowAnchor.value = r;
  }
  menu.value = { visible: true, x: e.clientX, y: e.clientY, items: rowHeadItems(), target: '', value: '' };
}

function showHeaderMenu(e, col) {
  menu.value = { visible: true, x: e.clientX, y: e.clientY, items: headerItems(), target: col, value: '' };
}

// 选区内的单元格列表（行 × 列交叉）
function selectedCells() {
  const cells = [];
  columns.value.forEach((col, c) => {
    if (!selColSet.value.has(col)) return;
    selRows.value.forEach(r => cells.push({ r, c }));
  });
  return cells;
}

function onMenuSelect(item) {
  const val = (item && item.value) || (typeof item === 'string' ? item : '');
  const col = menu.value.target;
  const cellValue = menu.value.value;
  const mr = menu.value.r;
  const mc = menu.value.c;
  menu.value = { ...menu.value, visible: false };
  if (val === 'copy') {
    navigator.clipboard.writeText(cellValue);
    ui.showToast('已复制到剪贴板', 'success');
  } else if (val === 'view') {
    ui.openContent(cellValue);
  } else if (val === 'edit') {
    ws.openCellEdit(mr, mc);
  } else if (val === 'sort-asc') {
    ws.sortResultBy(col, 'ASC');
  } else if (val === 'sort-desc') {
    ws.sortResultBy(col, 'DESC');
  } else if (val === 'filter') {
    ws.openFilterDialog(col);
  } else if (val === 'clear-sort') {
    ws.clearResultSort();
  } else if (val === 'insert-col') {
    ws.insertAtEditorCursor(col);
    ui.showToast(`已插入列名 ${col}`, 'success');
  } else if (val === 'clear-where') {
    ws.clearResultFilter();
  } else if (val === 'set-empty' || val === 'set-null') {
    const n = ws.setPendingEdits(selectedCells(), val === 'set-null' ? 'null' : 'empty');
    ui.showToast(`已暂存 ${n} 个单元格，点结果区「保存修改」提交`, 'success');
  } else if (val === 'copy-tsv') {
    ws.copyResultAsTsv(selRows.value, selCols.value);
  } else if (val === 'copy-insert') {
    ws.openSqlGenPanel('insert', selRows.value, selCols.value);
  } else if (val === 'copy-update') {
    ws.openSqlGenPanel('update', selRows.value, selCols.value);
  } else if (val === 'delete-rows') {
    ws.deleteResultRows(selRows.value);
  }
}

// 定位到当前查找命中处并滚动到可视区
watch([() => ws.findState.index, () => ws.findState.keyword], async () => {
  await nextTick();
  const el = rootEl.value?.querySelector('[data-midx="' + ws.findState.index + '"]');
  if (el) el.scrollIntoView({ block: 'center', inline: 'nearest' });
});
</script>

<template>
  <div class="result-content" ref="rootEl">
    <ResultStatements v-if="ws.stmtResults" />
    <div v-else-if="ws.pageInfo.error" style="color:var(--magenta);font-family:var(--font-mono);">错误: {{ ws.pageInfo.error }}</div>
    <div v-else-if="ws.pageInfo.write" style="color:var(--green);font-family:var(--font-mono);">执行成功，影响 {{ ws.pageInfo.rows }} 行</div>
    <div v-else-if="ws.pageInfo.idle"></div>
    <div v-else-if="ws.lastResult && !ws.lastResult.data.length && !columns.length" class="result-empty">
      <svg class="result-empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <rect x="3" y="4.5" width="18" height="15" rx="2"/>
        <path d="M3 9.5h18"/>
        <path d="M9 9.5v10"/>
      </svg>
      <div class="result-empty-text">查询结果为空</div>
    </div>
    <div v-else-if="noVisibleRows" style="color:var(--text-dim);font-family:var(--font-cjk);letter-spacing:1px;">当前页没有包含「{{ ws.findState.keyword }}」的行</div>
    <table v-else-if="ws.lastResult && columns.length" class="data-table" :class="{ dragging }">
      <thead>
        <tr>
          <th class="row-head" v-tip="'点击选择整行；Shift 连选；⌘/Ctrl 增减'"></th>
          <th
            v-for="(col, c) in columns"
            :key="col"
            :data-col="col"
            :style="colStyle(col)"
            v-tip="'点击选择整列；Shift 连选；⌘/Ctrl 增减；点列名后的箭头叠加排序，可多列；右键：排序 / 筛选；右边缘可拖动调整列宽'"
            @click="onClickHeader($event, col, c)"
            @contextmenu.prevent="showHeaderMenu($event, col)"
          >
            {{ col }}<span
              class="col-sort"
              :class="'sort-' + (sortDirOf(col) || 'none').toLowerCase()"
              v-tip="sortTipOf(col)"
              @click.stop="onSortClick(col)"
              @mousedown.stop
              @dblclick.stop
            ><span class="sort-fold"
            ><span class="sort-glyph up">▲</span><span class="sort-glyph down">▼</span></span><span
              v-if="sortRankOf(col) && ws.resultSort.length > 1"
              class="sort-rank"
            >{{ sortRankOf(col) }}</span></span>
            <div v-if="colComments[col]" class="col-sub">
              <div
                class="column-comment"
                v-tip="colComments[col]"
                :data-value="colComments[col]"
                @dblclick="ui.openContent(colComments[col])"
              >{{ colComments[col] }}</div>
              <button
                v-if="isConvertible(col)"
                class="col-mode"
                :class="{ active: modes[col] !== 'raw' }"
                @click.stop="ws.toggleColumnMode(col)"
              >{{ ws.MODE_LABEL[modes[col]] }}</button>
            </div>
            <!-- 没有注释的列（如纯时间戳字段）没有第二行可压，按钮保持常规排版 -->
            <button
              v-else-if="isConvertible(col)"
              class="col-mode col-mode-solo"
              :class="{ active: modes[col] !== 'raw' }"
              @click.stop="ws.toggleColumnMode(col)"
            >{{ ws.MODE_LABEL[modes[col]] }}</button>
            <!-- 列宽拖动手柄：压在表头右内边距上，按下即进入拖动，吞掉点击避免连带选中整列 -->
            <span
              class="col-resizer"
              :class="{ active: resizing && resizing.col === col }"
              v-tip="'拖动调整列宽'"
              @mousedown.stop.prevent="startResize($event, col)"
              @click.stop
              @dblclick.stop
              @contextmenu.stop.prevent
            ></span>
          </th>
        </tr>
      </thead>
      <tbody v-if="!ws.lastResult.data.length">
        <!-- 0 行：表头照常显示，下面给一行空结果提示，字段名不丢 -->
        <tr>
          <td class="empty-cell" :colspan="columns.length + 1">
            <div class="result-empty result-empty-compact">
              <svg class="result-empty-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <rect x="3" y="4.5" width="18" height="15" rx="2"/>
                <path d="M3 9.5h18"/>
                <path d="M9 9.5v10"/>
              </svg>
              <div class="result-empty-text">查询结果为空</div>
            </div>
          </td>
        </tr>
      </tbody>
      <tbody v-else>
        <tr
          v-for="r in ws.visibleRowIndexes"
          :key="r"
          :class="{ 'row-selected': isRowSelected(r), 'all-cols': allColsSelected }"
        >
          <td
            class="row-head"
            :class="{ 'row-selected': isRowSelected(r) }"
            v-tip="'第 ' + (r + 1) + ' 行：点击选择整行；按住拖动可选多行；Shift 连选；⌘/Ctrl 增减；右键：编辑 / 复制 / 删除'"
            @mousedown="onRowHeadDown($event, r)"
            @mouseenter="onRowHeadEnter(r)"
            @click="onClickRowHead($event, r)"
            @contextmenu.prevent="showRowHeadMenu($event, r)"
          >{{ r + 1 }}</td>
          <td
            v-for="(col, c) in columns"
            :key="col"
            :class="cellClass(ws.lastResult.data[r], col, r, c)"
            :style="colStyle(col)"
            :data-value="rawText(ws.lastResult.data[r][col])"
            :data-midx="cellMidx(r, c)"
            v-tip="cellTitle(ws.lastResult.data[r], col, r, c)"
            @mousedown="onCellDown($event, r, c)"
            @mouseenter="onCellEnter(r, c)"
            @click="onClickCell($event, r, c)"
            @dblclick="startEdit(r, c)"
            @contextmenu.prevent="showCellMenu($event, r, c, rawText(ws.lastResult.data[r][col]))"
          ><span
            class="cell-view"
            :class="{ 'cell-view-hidden': editing && editing.r === r && editing.c === c }"
          >{{ cellText(ws.lastResult.data[r], col, r, c) }}</span><input
            v-if="editing && editing.r === r && editing.c === c"
            :ref="el => { if (el) editorEl = el }"
            class="cell-editor"
            type="text"
            autocapitalize="off" autocorrect="off" spellcheck="false"
            :value="editValue"
            :placeholder="ws.lastResult.data[r][col] === null || ws.lastResult.data[r][col] === undefined ? 'NULL' : ''"
            v-tip="'原值：' + rawText(ws.lastResult.data[r][col])"
            @input="onEditInput(r, c, $event)"
            @keydown.enter.stop.prevent="endEdit()"
            @keydown.esc.stop.prevent="cancelEdit(r, c)"
            @blur="endEdit()"
            @mousedown.stop
            @click.stop
            @dblclick.stop
          ></td>
        </tr>
      </tbody>
    </table>

    <ContextMenu
      v-if="menu.visible"
      :x="menu.x"
      :y="menu.y"
      :items="menu.items"
      @select="onMenuSelect"
      @close="menu.visible = false"
    />
  </div>
</template>
