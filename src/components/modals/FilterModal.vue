<script setup>
import { ref, computed, watch } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useWorkspaceStore } from '@/stores/workspace';
import { FILTER_OPS, filterOpDef, buildWhereFromRows, highlightSqlSyntax } from '@/utils/sql';

const ws = useWorkspaceStore();
const rows = ref([]);

// 每次打开弹窗（filterDialog 被整体替换）时把条件行同步到本地状态
watch(() => ws.filterDialog, (v) => {
  if (v.visible) rows.value = v.rows.map(r => ({ ...r }));
});

const firstColumn = computed(() => ws.resultColumns[0] || '');

const preview = computed(() => {
  const where = buildWhereFromRows(rows.value);
  return where ? 'WHERE ' + where : '（条件不完整，请填写筛选值）';
});

function addRow() {
  rows.value.push({ col: firstColumn.value, op: 'eq', v1: '', v2: '', join: 'AND' });
}

function removeRow(i) {
  rows.value.splice(i, 1);
  if (!rows.value.length) {
    rows.value.push({ col: firstColumn.value, op: 'eq', v1: '', v2: '', join: 'AND' });
  }
}

function resetRows() {
  rows.value = [{ col: firstColumn.value, op: 'eq', v1: '', v2: '', join: 'AND' }];
}

function apply(exec) {
  ws.applyFilter(buildWhereFromRows(rows.value), exec);
}
</script>

<template>
  <ModalShell :visible="ws.filterDialog.visible" width="780px" @close="ws.closeFilterDialog()">
    <template #title>筛选条件</template>
    <div class="filter-rows">
      <div class="filter-row" v-for="(row, i) in rows" :key="i">
        <select v-if="i !== 0" class="f-join" v-model="row.join">
          <option value="AND">AND</option>
          <option value="OR">OR</option>
        </select>
        <select v-else class="f-join f-join-empty" disabled>
          <option>AND</option>
        </select>
        <select class="f-col" v-model="row.col">
          <option v-for="c in ws.resultColumns" :key="c" :value="c">{{ c }}</option>
        </select>
        <select class="f-op" v-model="row.op">
          <option v-for="o in FILTER_OPS" :key="o.k" :value="o.k">{{ o.label }}</option>
        </select>
        <div class="f-vals">
          <template v-if="filterOpDef(row.op).n === 0">
            <span class="f-val-none">不需要填写值</span>
          </template>
          <template v-else-if="filterOpDef(row.op).n === 2">
            <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="row.v1" placeholder="最小值">
            <span class="f-val-sep">~</span>
            <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="row.v2" placeholder="最大值">
          </template>
          <template v-else-if="filterOpDef(row.op).n === 'list'">
            <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="row.v1" placeholder="多个值用逗号分隔，如 1,2,3">
          </template>
          <template v-else>
            <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="row.v1" placeholder="筛选值">
          </template>
        </div>
        <button class="f-del" v-tip="'删除该条件'" @click="removeRow(i)">&times;</button>
      </div>
    </div>
    <div style="display: flex; align-items: center; gap: 12px; margin: 12px 0 16px;">
      <button class="btn btn-secondary" @click="addRow">+ 添加条件</button>
      <span class="filter-hint">多行条件按顺序用 AND / OR 连接；应用后会替换 SQL 中原有的 WHERE</span>
    </div>
    <div class="form-group">
      <label>生成的 SQL 条件预览</label>
      <div class="filter-preview" v-html="highlightSqlSyntax(preview)"></div>
    </div>
    <div class="filter-actions">
      <button class="btn btn-secondary" @click="resetRows">清空条件</button>
      <button class="btn btn-secondary" @click="ws.closeFilterDialog()">取消</button>
      <button class="btn btn-secondary" @click="apply(false)">应用不执行</button>
      <button class="btn" @click="apply(true)">应用并执行</button>
    </div>
  </ModalShell>
</template>
