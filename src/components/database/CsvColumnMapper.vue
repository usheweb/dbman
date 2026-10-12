<script setup>
// CSV 列 → 目标表字段 的关联表（含数据预览）
const props = defineProps({
  headers: { type: Array, default: () => [] },
  rows: { type: Array, default: () => [] },
  columns: { type: Array, default: () => [] },
  mapping: { type: Array, default: () => [] },
  totalRows: { type: Number, default: 0 }
});
const emit = defineEmits(['change']);

function setTarget(index, value) {
  props.mapping[index] = value || null;
  emit('change');
}

function isDuplicate(i) {
  const v = props.mapping[i];
  return !!v && props.mapping.some((x, j) => j !== i && x === v);
}

function cellValue(row, i) {
  const v = row[i];
  if (v === undefined || v === null || v === '') return '（空）';
  return v;
}
</script>

<template>
  <div class="cm-wrap">
    <div class="cm-scroll">
      <table class="cm-table">
        <thead>
          <tr>
            <th class="cm-idx">#</th>
            <th v-for="(h, i) in headers" :key="i" class="cm-col">
              <div class="cm-head-name" :title="h">{{ h }}</div>
              <select
                class="cm-select"
                :class="{ 'cm-dup': isDuplicate(i) }"
                :value="mapping[i] || ''"
                @change="setTarget(i, $event.target.value)"
              >
                <option value="">不导入</option>
                <option v-for="c in columns" :key="c" :value="c">{{ c }}</option>
              </select>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, r) in rows" :key="r">
            <td class="cm-idx">{{ r + 1 }}</td>
            <td v-for="(h, i) in headers" :key="i" :title="row[i]">{{ cellValue(row, i) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <div class="cm-hint">
      预览前 {{ rows.length }} 行，共 {{ totalRows }} 行数据；同一目标字段被多列关联时会标红，请避免重复。
    </div>
  </div>
</template>

<style scoped>
.cm-wrap {
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--bg-1);
}
.cm-scroll {
  max-height: 38vh;
  overflow: auto;
}
.cm-table {
  border-collapse: separate;
  border-spacing: 0;
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: nowrap;
}
.cm-table th,
.cm-table td {
  padding: 6px 10px;
  border-right: 1px solid var(--line);
  border-bottom: 1px solid var(--line);
  text-align: left;
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.cm-table thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--panel-solid);
  vertical-align: top;
}
.cm-idx {
  color: var(--text-dim);
  text-align: right;
  min-width: 40px;
}
.cm-head-name {
  margin-bottom: 6px;
  color: var(--cyan);
  font-weight: 600;
  max-width: 180px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.cm-select {
  width: 100%;
  min-width: 130px;
  padding: 4px 6px;
  background: rgba(var(--void-rgb), .8);
  border: 1px solid var(--line);
  border-radius: 5px;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 12px;
  outline: none;
}
.cm-select:focus {
  border-color: var(--cyan);
}
.cm-select.cm-dup {
  border-color: var(--magenta);
  color: var(--magenta);
}
.cm-table tbody td {
  color: var(--text-dim);
}
.cm-hint {
  padding: 8px 12px;
  border-top: 1px solid var(--line);
  color: var(--text-dim);
  font-size: 12px;
}
</style>
