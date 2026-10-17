<script setup>
import { computed } from 'vue';
import { newIndex } from '@/utils/ddl';
import { useUiStore } from '@/stores/ui';

const props = defineProps({
  indexes: { type: Array, required: true },
  columns: { type: Array, default: () => [] }
});
const emit = defineEmits(['change']);

const INDEX_TYPES = ['BTREE', 'HASH', 'FULLTEXT', 'SPATIAL'];

const colNames = computed(() => props.columns.map(c => c.name).filter(Boolean));

const ui = useUiStore();

function touch() {
  emit('change');
}

function addIndex() {
  props.indexes.push(newIndex());
  touch();
}

async function removeIndex(i) {
  const ix = props.indexes[i];
  const label = ix.name || '（未命名索引）';
  const ok = await ui.askConfirm({
    title: '删除索引',
    message: `确定删除索引「${label}」吗？`,
    confirmText: '删除',
    danger: true
  });
  if (!ok) return;
  props.indexes.splice(i, 1);
  touch();
}

function setColumns(ix, text) {
  ix.columns = String(text).split(/[,，]+/).map(s => s.trim()).filter(Boolean);
  touch();
}

function setType(ix, type) {
  ix.indexType = type;
  if (type === 'FULLTEXT' || type === 'SPATIAL') ix.unique = false;
  touch();
}

function isPrimary(ix) {
  return ix.name === 'PRIMARY';
}
</script>

<template>
  <div class="dz-fields">
    <div class="dz-toolbar">
      <button class="btn btn-secondary" @click="addIndex">+ 添加索引</button>
      <span class="dz-count">{{ indexes.length }} 个索引</span>
      <span class="dz-hint">索引名填 PRIMARY 即为主键；多个列按顺序用逗号分隔（如 a, b）。可用列：{{ colNames.join(', ') || '无' }}</span>
    </div>

    <div class="dz-body">
      <div class="dz-head dz-grid">
        <span>索引名</span>
        <span>类型</span>
        <span class="dz-c">唯一</span>
        <span>列（按顺序）</span>
        <span class="dz-c">操作</span>
      </div>

      <div class="dz-row dz-grid" v-for="(ix, i) in indexes" :key="i">
        <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          v-model="ix.name" placeholder="索引名 / PRIMARY" @input="touch">
        <select v-model="ix.indexType" :disabled="isPrimary(ix)" @change="setType(ix, $event.target.value)">
          <option v-for="t in INDEX_TYPES" :key="t" :value="t">{{ t }}</option>
        </select>
        <span class="dz-c">
          <input type="checkbox" v-model="ix.unique" :disabled="isPrimary(ix)" @change="touch">
        </span>
        <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          :value="(ix.columns || []).join(', ')" placeholder="col1, col2"
          @input="setColumns(ix, $event.target.value)">
        <span class="dz-c dz-acts">
          <button class="dz-mini dz-danger" v-tip="'删除索引'" @click="removeIndex(i)">&times;</button>
        </span>
      </div>
      <div class="dz-empty" v-if="!indexes.length">暂无索引。</div>
    </div>
  </div>
</template>

<style scoped>
/* 网格列宽：索引名 / 类型 / 唯一 / 列（按顺序） / 操作 */
.dz-grid {
  grid-template-columns: 1.2fr 118px 56px 2fr 64px;
}
</style>
