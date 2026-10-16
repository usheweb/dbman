<script setup>
import { computed } from 'vue';
import { newForeignKey } from '@/utils/ddl';

const props = defineProps({
  foreignKeys: { type: Array, required: true },
  columns: { type: Array, default: () => [] },
  tables: { type: Array, default: () => [] }
});
const emit = defineEmits(['change']);

const ACTIONS = ['RESTRICT', 'CASCADE', 'SET NULL', 'NO ACTION', 'SET DEFAULT'];

const colNames = computed(() => props.columns.map(c => c.name).filter(Boolean));

function touch() {
  emit('change');
}

function addFk() {
  props.foreignKeys.push(newForeignKey());
  touch();
}

function removeFk(i) {
  const fk = props.foreignKeys[i];
  const label = fk.name || '（未命名外键）';
  if (!confirm(`确定删除外键「${label}」吗？`)) return;
  props.foreignKeys.splice(i, 1);
  touch();
}

function setColumns(fk, text) {
  fk.columns = String(text).split(/[,，]+/).map(s => s.trim()).filter(Boolean);
  touch();
}

function setRefColumns(fk, text) {
  fk.refColumns = String(text).split(/[,，]+/).map(s => s.trim()).filter(Boolean);
  touch();
}
</script>

<template>
  <div class="dz-fields">
    <div class="dz-toolbar">
      <button class="btn btn-secondary" @click="addFk">+ 添加外键</button>
      <span class="dz-count">{{ foreignKeys.length }} 个外键</span>
      <span class="dz-hint">本地列与被引用列需数量、类型一致；被引用列须有索引。可用列：{{ colNames.join(', ') || '无' }}</span>
    </div>

    <div class="dz-body">
      <div class="dz-head dz-grid">
        <span>约束名</span>
        <span>本地列</span>
        <span>引用表</span>
        <span>引用列</span>
        <span>删除时</span>
        <span>更新时</span>
        <span class="dz-c">操作</span>
      </div>

      <div class="dz-row dz-grid" v-for="(fk, i) in foreignKeys" :key="i">
        <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          v-model="fk.name" placeholder="fk_name" @input="touch">
        <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          :value="(fk.columns || []).join(', ')" placeholder="本地列"
          @input="setColumns(fk, $event.target.value)">
        <select v-model="fk.refTable" @change="touch">
          <option value="">选择表</option>
          <option v-for="t in tables" :key="t" :value="t">{{ t }}</option>
        </select>
        <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          :value="(fk.refColumns || []).join(', ')" placeholder="引用列"
          @input="setRefColumns(fk, $event.target.value)">
        <select v-model="fk.onDelete" @change="touch">
          <option v-for="a in ACTIONS" :key="a" :value="a">{{ a }}</option>
        </select>
        <select v-model="fk.onUpdate" @change="touch">
          <option v-for="a in ACTIONS" :key="a" :value="a">{{ a }}</option>
        </select>
        <span class="dz-c dz-acts">
          <button class="dz-mini dz-danger" v-tip="'删除外键'" @click="removeFk(i)">&times;</button>
        </span>
      </div>
      <div class="dz-empty" v-if="!foreignKeys.length">暂无外键。</div>
    </div>
  </div>
</template>

<style scoped>
/* 网格列宽：约束名 / 本地列 / 引用表 / 引用列 / 删除时 / 更新时 / 操作 */
.dz-grid {
  grid-template-columns: 1.1fr 1.1fr 1.1fr 1.1fr 104px 104px 64px;
}
</style>
