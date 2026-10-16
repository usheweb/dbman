<script setup>
import { ref, computed } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { useWorkspaceStore } from '@/stores/workspace';
import { useTableStore } from '@/stores/table';
import { useCsvImportStore } from '@/stores/csvImport';
import ItemIcon from '@/components/common/ItemIcon.vue';
import IconSelect from '@/components/common/IconSelect.vue';
import ContextMenu from '@/components/common/ContextMenu.vue';
import TableSortMenu from '@/components/database/TableSortMenu.vue';
import { sqlIdent } from '@/utils/sql';
import { sortTables, metaText, defaultMetaText, fieldLabel, DEFAULT_TABLE_SORT } from '@/utils/tableSort';

const ui = useUiStore();
const connStore = useConnectionStore();
const ws = useWorkspaceStore();
const tableStore = useTableStore();
const csvStore = useCsvImportStore();

const filterText = ref('');

// 表列表排序（多列组合，持久化在 ui store）
const sortVisible = ref(false);
const sortBtnEl = ref(null);
const sortKeys = computed(() => ui.tableSort);
const sortActive = computed(() => {
  const k = sortKeys.value;
  return !(k.length === DEFAULT_TABLE_SORT.length
    && k.every((x, i) => x.field === DEFAULT_TABLE_SORT[i].field && x.dir === DEFAULT_TABLE_SORT[i].dir));
});
const sortTitle = computed(() => {
  if (!sortKeys.value.length) return '排序：未设置';
  return '排序：' + sortKeys.value
    .map(k => fieldLabel(k.field) + (k.dir === 'asc' ? ' ↑' : ' ↓'))
    .join('，');
});

function updateSort(keys) {
  ui.saveTableSort(keys);
}

function metaOf(table) {
  // 有额外排序维度时展示排序维度值，否则默认展示「行数 · 总大小」
  return metaText(table, sortKeys.value) || defaultMetaText(table);
}

// 右键菜单
const menuVisible = ref(false);
const menuX = ref(0);
const menuY = ref(0);
const menuTable = ref('');

const menuItems = [
  { label: '打开表（查看数据）', value: 'open' },
  { label: '查看建表语句', value: 'ddl' },
  { label: '复制表名', value: 'copy' },
  { separator: true },
  { label: '导出表结构 SQL…', value: 'exportStructure' },
  { label: '导出数据 SQL…', value: 'exportData' },
  { label: '导入 CSV 数据…', value: 'importCsv' },
  { label: '修改表结构…', value: 'designer' },
  { separator: true },
  { label: '创建同样字段的表…', value: 'createLike' },
  { label: '重命名表…', value: 'rename' },
  { label: '清空表（TRUNCATE）', value: 'truncate', danger: true },
  { label: '删除表（DROP）', value: 'drop', danger: true }
];

// 库数量常上百个，下拉支持输入模糊筛选
const dbOptions = computed(() => connStore.currentDatabaseList.map(db => ({ value: db, label: db })));

// 按空白与「中英文交界」自动分词，例：detail积分 → ['detail', '积分']
const tokens = computed(() => filterText.value
  .replace(/([\u4e00-\u9fa5])([A-Za-z0-9_])/g, '$1 $2')
  .replace(/([A-Za-z0-9_])([\u4e00-\u9fa5])/g, '$1 $2')
  .split(/\s+/)
  .filter(Boolean)
  .map(t => t.toLowerCase()));

// 交集模糊：所有关键词都要命中「表名 或 注释」才保留（detail税务 → 两者都含才显示）
const filteredTables = computed(() => {
  const ts = tokens.value;
  return connStore.allTables.filter(table => {
    const hay = (table.tableName + ' ' + (table.tableComment || '')).toLowerCase();
    if (ts.length && !ts.every(t => hay.includes(t))) return false;
    return true;
  });
});

// 过滤后再按排序键（可多列组合）排序
const sortedTables = computed(() => sortTables(filteredTables.value, sortKeys.value));

// 列表上方的数量统计：全量还没回来时先显示加载中（否则表数会从 20 跳到 N），
// 无过滤词时给全量，有过滤词时给「命中 / 总数」
const tableCountText = computed(() => {
  if (connStore.tablesLoading) return '表列表加载中…';
  const total = connStore.allTables.length;
  if (!tokens.value.length) return '共 ' + total + ' 张表';
  return filteredTables.value.length + ' / ' + total + ' 张表';
});

async function selectTable(tableName) {
  connStore.currentTable = tableName;
  const cm = connStore.currentDatabase;
  await connStore.loadColumns(cm, tableName);
  // 不再自带 LIMIT：行数交给结果区底部的「每页显示 + 翻页」控制
  ws.setSqlAndRun('SELECT * FROM ' + sqlIdent(tableName) + ';');
}

function onContextMenu(e, tableName) {
  menuTable.value = tableName;
  menuX.value = e.clientX;
  menuY.value = e.clientY;
  menuVisible.value = true;
}

async function copyTableName(name) {
  try {
    await navigator.clipboard.writeText(name);
    ui.showToast('已复制表名', 'success');
  } catch (e) {
    ui.showToast('复制失败', 'error');
  }
}

async function renameTable(name) {
  const next = await tableStore.askText({ title: '重命名表', label: '新表名', placeholder: name, value: name });
  if (!next || next === name) return;
  await tableStore.renameTable(name, next);
}

async function createLike(name) {
  const next = await tableStore.askText({ title: '创建同样字段的表', label: '新表名', placeholder: `${name}_copy` });
  if (!next || next === name) return;
  await tableStore.createTableLike(name, next);
}

async function truncateTable(name) {
  if (!confirm(`确定要清空表「${name}」吗？该操作不可撤销，且会重置自增 ID。`)) return;
  await tableStore.truncateTable(name);
}

async function dropTable(name) {
  if (!confirm(`确定要删除表「${name}」吗？该操作不可撤销，表结构与数据将全部丢失。`)) return;
  await tableStore.dropTable(name);
}

function onMenuSelect(item) {
  menuVisible.value = false;
  const name = menuTable.value;
  if (!name || !item) return;
  switch (item.value) {
    case 'open': selectTable(name); break;
    case 'ddl': tableStore.openDdl(name); break;
    case 'copy': copyTableName(name); break;
    case 'exportStructure': tableStore.openExportSql(name, 'structure'); break;
    case 'exportData': tableStore.openExportSql(name, 'data'); break;
    case 'importCsv': csvStore.open(name); break;
    case 'designer': tableStore.openDesigner(name); break;
    case 'createLike': createLike(name); break;
    case 'rename': renameTable(name); break;
    case 'truncate': truncateTable(name); break;
    case 'drop': dropTable(name); break;
  }
}
</script>

<template>
  <div class="db-tree" :class="{ collapsed: ui.panelState.tree }">
    <div class="tree-header-row" :class="{ hidden: connStore.currentDatabaseList.length === 0 }">
      <IconSelect
        class="db-picker"
        title="切换数据库"
        :model-value="connStore.currentDatabase"
        :options="dbOptions"
        searchable
        @update:model-value="connStore.selectDatabase"
      />
    </div>
    <div class="tree-filter-row">
      <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" class="filter-input" v-model="filterText" placeholder="过滤表名/注释...">
      <div class="sort-wrap">
        <button
          ref="sortBtnEl"
          class="tree-sort-btn"
          :class="{ active: sortActive }"
          :title="sortTitle"
          @click.stop="sortVisible = !sortVisible"
        >
          <span class="sort-arrow">⇅</span>
          <span v-if="sortKeys.length > 1" class="sort-badge">{{ sortKeys.length }}</span>
          <span class="btn-label">排序</span>
        </button>
        <TableSortMenu
          v-if="sortVisible"
          :model-value="sortKeys"
          :anchor="sortBtnEl"
          @update:model-value="updateSort"
          @close="sortVisible = false"
        />
      </div>
      <button
        class="panel-toggle"
        @click="ui.togglePanel('tree')"
        :title="(ui.panelState.tree ? '展开' : '收起') + ui.panelTitle('tree') + '列'"
      >
        <span class="toggle-glyph">{{ ui.panelState.tree ? '›' : '‹' }}</span>
        <span class="btn-label">{{ ui.panelState.tree ? '展开' : '收起' }}</span>
      </button>
    </div>
    <div v-if="connStore.currentDatabase" class="tree-count-row" :title="tableCountText">{{ tableCountText }}</div>
    <div id="tableList">
      <div
        v-for="t in sortedTables"
        :key="t.tableName"
        class="tree-item tree-table"
        :class="{ active: connStore.currentTable === t.tableName }"
        @click="selectTable(t.tableName)"
        @contextmenu.prevent="onContextMenu($event, t.tableName)"
      >
        <ItemIcon name="table" />
        <span class="tree-table-body">
          <span class="tree-table-name">{{ t.tableName }}</span>
          <span v-if="ui.viewOpts.tableComment && t.tableComment" class="tree-table-comment">{{ t.tableComment }}</span>
          <span v-if="ui.viewOpts.tableMeta && metaOf(t)" class="tree-table-meta" :title="metaOf(t)">{{ metaOf(t) }}</span>
        </span>
      </div>
    </div>
    <ContextMenu
      v-if="menuVisible"
      :x="menuX"
      :y="menuY"
      :items="menuItems"
      @select="onMenuSelect"
      @close="menuVisible = false"
    />
  </div>
</template>
