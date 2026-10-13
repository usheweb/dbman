<script setup>
// 生成 Insert / Update 语句：选择参与的行与列 + 实时预览 + 二次调整
// （批量换值、新增列、取消勾选即删除列、是否保留库名）
import { ref, computed, watch } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useWorkspaceStore } from '@/stores/workspace';
import { useUiStore } from '@/stores/ui';
import { highlightSqlSyntax } from '@/utils/sql';

const ws = useWorkspaceStore();
const ui = useUiStore();

const cols = ref([]);        // 结果集列名（顺序固定）
const rows = ref([]);        // 结果集行索引
const mode = ref('insert');
const colChecked = ref({});  // 列名 → 是否保留
const rowChecked = ref({});  // 行索引 → 是否参与
const overrides = ref({});   // 列名 → { on, value, asNull } 批量换值
const extraCols = ref([]);   // 新增列 [{ name, value, asNull }]
const keepDb = ref(false);
const ctx = ref(null);       // { ref, pkCols, generated }
const loading = ref(false);

const sqlHtml = computed(() => highlightSqlSyntax(outSql.value));

watch(() => ws.sqlGenPanel, async (v) => {
  if (!v.visible) return;
  mode.value = v.mode;
  cols.value = v.cols.slice();
  rows.value = v.rows.slice();
  keepDb.value = !!v.keepDb;
  const cc = {}, rc = {}, ov = {};
  cols.value.forEach(c => { cc[c] = true; ov[c] = { on: false, value: '', asNull: false }; });
  rows.value.forEach(r => { rc[r] = true; });
  colChecked.value = cc;
  rowChecked.value = rc;
  overrides.value = ov;
  extraCols.value = [];
  ctx.value = null;
  loading.value = true;
  try {
    ctx.value = await ws.sqlGenContext();
  } finally {
    loading.value = false;
  }
});

const selectedCols = computed(() => cols.value.filter(c => colChecked.value[c]));
const selectedRows = computed(() => rows.value.filter(r => rowChecked.value[r]));

const outSql = computed(() => {
  if (!ctx.value || !selectedRows.value.length) return '';
  const opts = { keepDb: keepDb.value, overrides: overrides.value, extraCols: extraCols.value };
  return mode.value === 'update'
    ? ws.buildUpdateSql(ctx.value, selectedCols.value, selectedRows.value, opts)
    : ws.buildInsertSql(ctx.value, selectedCols.value, selectedRows.value, opts);
});

const noPkUpdate = computed(() =>
  mode.value === 'update' && !!ctx.value && !ctx.value.pkCols.length
);

function setAllCols(flag) {
  const cc = { ...colChecked.value };
  cols.value.forEach(c => { cc[c] = flag; });
  colChecked.value = cc;
}

function setAllRows(flag) {
  const rc = { ...rowChecked.value };
  rows.value.forEach(r => { rc[r] = flag; });
  rowChecked.value = rc;
}

function addExtraCol() {
  extraCols.value = [...extraCols.value, { name: '', value: '', asNull: false }];
}

function removeExtraCol(i) {
  extraCols.value = extraCols.value.filter((_, idx) => idx !== i);
}

async function copyOut() {
  if (!outSql.value) { ui.showToast('没有可复制的 SQL', 'error'); return; }
  try {
    await navigator.clipboard.writeText(outSql.value);
    ui.showToast(`已复制 ${selectedRows.value.length} 条语句到剪贴板`, 'success');
  } catch (err) {
    ui.showToast('复制失败：' + err.message, 'error');
  }
}
</script>

<template>
  <ModalShell
    :visible="ws.sqlGenPanel.visible"
    width="980px"
    @close="ws.closeSqlGenPanel()"
  >
    <template #title>{{ mode === 'update' ? '生成 Update 语句' : '生成 Insert 语句' }}</template>

    <div v-if="loading" class="rsq-loading">正在读取表结构…</div>
    <div v-else-if="!ctx" class="rsq-loading">无法识别结果对应的数据表（SQL 中缺少 FROM 表名），请改用带 FROM 的查询。</div>

    <div v-else class="rsq-panel">
      <div class="rsq-col rsq-col-left">
        <div class="rsq-toolbar">
          <span class="rsq-label">字段（取消勾选即从语句中删除该列）</span>
          <button class="rsq-mini" @click="setAllCols(true)">全选</button>
          <button class="rsq-mini" @click="setAllCols(false)">全不选</button>
          <label class="rsq-check rsq-inline">
            <input type="checkbox" v-model="keepDb">
            <span>保留库名</span>
          </label>
        </div>

        <div class="rsq-list">
          <div class="rsq-row" v-for="col in cols" :key="col">
            <label class="rsq-check">
              <input type="checkbox" v-model="colChecked[col]">
            </label>
            <span
              class="rsq-name"
              :class="{ 'rsq-generated': ctx.generated.has(col) }"
              :title="ctx.generated.has(col) ? '生成列：不会写入语句' : col"
            >{{ col }}</span>
            <input
              class="rsq-input"
              type="text"
              autocapitalize="off" autocorrect="off" spellcheck="false"
              v-model="overrides[col].value"
              :placeholder="overrides[col].on ? '批量替换为该值' : '批量换值…'"
              @input="overrides[col].on = true"
              @blur="overrides[col].on = overrides[col].value !== '' || overrides[col].asNull"
            >
            <button
              class="rsq-mini"
              :class="{ active: overrides[col].asNull }"
              title="该列批量改写为 NULL"
              @click="overrides[col].asNull = !overrides[col].asNull; overrides[col].on = true"
            >NULL</button>
          </div>
        </div>

        <div class="rsq-extra-head">
          <span class="rsq-label">新增列（追加到每条语句）</span>
          <button class="rsq-mini" @click="addExtraCol">+ 新增列</button>
        </div>
        <div class="rsq-list rsq-list-extra">
          <div class="rsq-row" v-for="(x, i) in extraCols" :key="i">
            <input
              class="rsq-input rsq-input-name"
              type="text"
              autocapitalize="off" autocorrect="off" spellcheck="false"
              v-model="x.name"
              placeholder="列名"
            >
            <input
              class="rsq-input"
              type="text"
              autocapitalize="off" autocorrect="off" spellcheck="false"
              v-model="x.value"
              :disabled="x.asNull"
              placeholder="值"
            >
            <button
              class="rsq-mini"
              :class="{ active: x.asNull }"
              title="该新增列写为 NULL"
              @click="x.asNull = !x.asNull"
            >NULL</button>
            <button class="rsq-mini rsq-del" title="删除该新增列" @click="removeExtraCol(i)">&times;</button>
          </div>
          <div v-if="!extraCols.length" class="rsq-empty">暂无新增列</div>
        </div>

        <div class="rsq-toolbar">
          <span class="rsq-label">参与的行（第 N 行按当前页顺序）</span>
          <button class="rsq-mini" @click="setAllRows(true)">全选</button>
          <button class="rsq-mini" @click="setAllRows(false)">全不选</button>
        </div>
        <div class="rsq-list rsq-list-rows">
          <label class="rsq-check rsq-row-item" v-for="r in rows" :key="r">
            <input type="checkbox" v-model="rowChecked[r]">
            <span>第 {{ r + 1 }} 行</span>
          </label>
        </div>
      </div>

      <div class="rsq-col rsq-col-right">
        <div class="rsq-toolbar">
          <span class="rsq-label">输出 SQL（{{ selectedRows.length }} 行 × {{ selectedCols.length }} 列）</span>
        </div>
        <div v-if="noPkUpdate" class="rsq-warn">该表没有主键，WHERE 将使用所有列的原值作为条件。</div>
        <pre class="rsq-preview" v-html="sqlHtml"></pre>
        <div class="rsq-actions">
          <button class="btn btn-secondary" @click="ws.closeSqlGenPanel()">关闭</button>
          <button class="btn" @click="copyOut">复制结果</button>
        </div>
      </div>
    </div>
  </ModalShell>
</template>
