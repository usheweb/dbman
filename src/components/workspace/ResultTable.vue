<script setup>
// 查询结果表格：错误 / 写结果 / 空结果 / 数据表 + 表头与单元格右键菜单
import { ref, computed, watch, nextTick } from 'vue';
import { useWorkspaceStore } from '@/stores/workspace';
import { useUiStore } from '@/stores/ui';
import { rawText } from '@/utils/format';
import { splitTopLevelClauses } from '@/utils/sql';
import ContextMenu from '@/components/common/ContextMenu.vue';

const ws = useWorkspaceStore();
const ui = useUiStore();

const rootEl = ref(null);
const selected = ref(null); // { r, c }
const menu = ref({ visible: false, x: 0, y: 0, items: [], target: '', value: '' });

const columns = computed(() => ws.resultColumns);
const colInfo = computed(() => (ws.lastResult && ws.lastResult.colInfo) || {});
const colComments = computed(() => ws.currentColComments());
const modes = computed(() => ws.columnModesForRender);

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

function isConvertible(col) {
  const info = colInfo.value[col];
  return !!(info && (info.canTimestamp || info.enumMap));
}

function isSelected(r, c) {
  return !!selected.value && selected.value.r === r && selected.value.c === c;
}

function cellMidx(r, c) {
  const k = r + ':' + c;
  return Object.prototype.hasOwnProperty.call(matchIndexMap.value, k) ? matchIndexMap.value[k] : null;
}

function selectCell(r, c) {
  selected.value = { r, c };
}

// ===== 右键菜单 =====
function headerItems() {
  const parsed = splitTopLevelClauses(ws.getSql());
  const hasSort = !!(parsed && parsed.order);
  const hasWhere = !!(parsed && parsed.where);
  const items = [
    { label: '升序排序', value: 'sort-asc' },
    { label: '降序排序', value: 'sort-desc' },
    { label: '筛选…', value: 'filter' }
  ];
  if (hasSort || hasWhere) items.push({ separator: true });
  if (hasSort) items.push({ label: '清除排序', value: 'clear-sort', danger: true });
  if (hasWhere) items.push({ label: '清除筛选', value: 'clear-where', danger: true });
  return items;
}

function showCellMenu(e, r, c, value) {
  // 右键同样要落在选中态上，和左键点击表现一致
  selectCell(r, c);
  menu.value = { visible: true, x: e.clientX, y: e.clientY, items: [{ label: '复制', value: 'copy' }], target: '', value };
}

function showHeaderMenu(e, col) {
  menu.value = { visible: true, x: e.clientX, y: e.clientY, items: headerItems(), target: col, value: '' };
}

function onMenuSelect(item) {
  const val = (item && item.value) || (typeof item === 'string' ? item : '');
  const col = menu.value.target;
  const cellValue = menu.value.value;
  menu.value = { ...menu.value, visible: false };
  if (val === 'copy') {
    navigator.clipboard.writeText(cellValue);
    ui.showToast('已复制到剪贴板', 'success');
  } else if (val === 'sort-asc') {
    ws.sortResultBy(col, 'ASC');
  } else if (val === 'sort-desc') {
    ws.sortResultBy(col, 'DESC');
  } else if (val === 'filter') {
    ws.openFilterDialog(col);
  } else if (val === 'clear-sort') {
    ws.clearResultSort();
  } else if (val === 'clear-where') {
    ws.clearResultFilter();
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
    <div v-if="ws.pageInfo.error" style="color:var(--magenta);font-family:var(--font-mono);">错误: {{ ws.pageInfo.error }}</div>
    <div v-else-if="ws.pageInfo.write" style="color:var(--green);font-family:var(--font-mono);">执行成功，影响 {{ ws.pageInfo.rows }} 行</div>
    <div v-else-if="ws.pageInfo.idle"></div>
    <div v-else-if="ws.lastResult && !ws.lastResult.data.length" style="color:var(--text-dim);">查询结果为空</div>
    <div v-else-if="noVisibleRows" style="color:var(--text-dim);font-family:var(--font-cjk);letter-spacing:1px;">当前页没有包含「{{ ws.findState.keyword }}」的行</div>
    <table v-else-if="ws.lastResult" class="data-table">
      <thead>
        <tr>
          <th
            v-for="col in columns"
            :key="col"
            :data-col="col"
            title="右键：排序 / 筛选"
            @contextmenu.prevent="showHeaderMenu($event, col)"
          >
            {{ col }}
            <div
              v-if="colComments[col]"
              class="column-comment"
              :title="colComments[col]"
              :data-value="colComments[col]"
              @dblclick="ui.openContent(colComments[col])"
            >{{ colComments[col] }}</div>
            <button
              v-if="isConvertible(col)"
              class="col-mode"
              :class="{ active: modes[col] !== 'raw' }"
              @click.stop="ws.toggleColumnMode(col)"
            >{{ ws.MODE_LABEL[modes[col]] }}</button>
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="r in ws.visibleRowIndexes" :key="r">
          <td
            v-for="(col, c) in columns"
            :key="col"
            :class="{
              'cell-match': ws.cellHit(ws.lastResult.data[r], col),
              'cell-selected': isSelected(r, c),
              'cell-active': cellMidx(r, c) === ws.findState.index
            }"
            :data-value="rawText(ws.lastResult.data[r][col])"
            :data-midx="cellMidx(r, c)"
            @click="selectCell(r, c)"
            @dblclick="ui.openContent(rawText(ws.lastResult.data[r][col]))"
            @contextmenu.prevent="showCellMenu($event, r, c, rawText(ws.lastResult.data[r][col]))"
          >{{ ws.applyDisplayMode(ws.lastResult.data[r][col], modes[col], colInfo[col]) }}</td>
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
