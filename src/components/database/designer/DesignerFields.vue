<script setup>
import { computed } from 'vue';
import { newColumn, hasAutoInc, hasOnUpdate, isGenerated } from '@/utils/ddl';

const props = defineProps({
  columns: { type: Array, required: true }
});
const emit = defineEmits(['change']);

const COMMON_TYPES = [
  'int', 'int unsigned', 'bigint', 'bigint unsigned', 'tinyint(1)', 'smallint',
  'decimal(10,2)', 'float', 'double',
  'varchar(255)', 'varchar(64)', 'char(36)', 'text', 'longtext', 'mediumtext', 'json',
  'date', 'datetime', 'timestamp', 'time', 'year', 'blob', 'longblob', 'varbinary(16)'
];

const KEY_LABEL = { PRI: '主键', UNI: '唯一', MUL: '索引' };

function touch() {
  emit('change');
}

function addColumn() {
  props.columns.push(newColumn());
  touch();
}

function removeColumn(i) {
  const c = props.columns[i];
  if (c.name && !confirm(`确定删除字段「${c.name}」吗？执行后该字段及其数据将一并移除。`)) return;
  props.columns.splice(i, 1);
  touch();
}

function move(i, delta) {
  const j = i + delta;
  if (j < 0 || j >= props.columns.length) return;
  const arr = props.columns;
  const tmp = arr[i];
  arr[i] = arr[j];
  arr[j] = tmp;
  touch();
}

// extra 只保留自增 / ON UPDATE 两个可编辑开关，其余（GENERATED 等）由 generationExpression 决定
function setFlags(c, autoInc, onUpdate) {
  const parts = [];
  if (autoInc) parts.push('auto_increment');
  if (onUpdate) parts.push('on update CURRENT_TIMESTAMP');
  c.extra = parts.join(' ');
  touch();
}

const total = computed(() => props.columns.length);
const isFirstRow = i => i === 0;
const isLastRow = i => i === props.columns.length - 1;
</script>

<template>
  <div class="dz-fields">
    <div class="dz-toolbar">
      <button class="btn btn-secondary" @click="addColumn">+ 添加字段</button>
      <span class="dz-count">{{ total }} 个字段</span>
      <span class="dz-hint">默认值留空表示「无默认值」；可填数字、NULL、CURRENT_TIMESTAMP 或文本。生成列的类型与表达式不可在此修改。</span>
    </div>

    <datalist id="dz-col-types">
      <option v-for="t in COMMON_TYPES" :key="t" :value="t" />
    </datalist>

    <div class="dz-body">
      <div class="dz-head dz-grid">
        <span>字段名</span>
        <span>类型</span>
        <span class="dz-c">允许 NULL</span>
        <span>默认值</span>
        <span>注释</span>
        <span class="dz-c">自增</span>
        <span class="dz-c">更新时</span>
        <span class="dz-c">操作</span>
      </div>

      <div class="dz-row dz-grid" v-for="(c, i) in columns" :key="i">
        <span class="dz-cell">
          <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
            v-model="c.name" placeholder="字段名" @input="touch">
          <span v-if="c.columnKey" class="dz-key" :title="KEY_LABEL[c.columnKey]">{{ c.columnKey }}</span>
        </span>
        <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" list="dz-col-types"
          v-model="c.columnType" placeholder="如 varchar(255)" @input="touch">
        <span class="dz-c">
          <input type="checkbox" v-model="c.nullable" :disabled="isGenerated(c)" @change="touch">
        </span>
        <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          :value="c.default == null ? '' : c.default" placeholder="无"
          @input="c.default = $event.target.value === '' ? null : $event.target.value; touch()">
        <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          :value="c.comment || ''" placeholder="注释"
          @input="c.comment = $event.target.value || null; touch()">
        <span class="dz-c">
          <input type="checkbox" :checked="hasAutoInc(c)" @change="setFlags(c, $event.target.checked, hasOnUpdate(c))">
        </span>
        <span class="dz-c">
          <input type="checkbox" :checked="hasOnUpdate(c)" @change="setFlags(c, hasAutoInc(c), $event.target.checked)">
        </span>
        <span class="dz-c dz-acts">
          <button class="dz-mini" title="上移" :disabled="isFirstRow(i)" @click="move(i, -1)">&#9650;</button>
          <button class="dz-mini" title="下移" :disabled="isLastRow(i)" @click="move(i, 1)">&#9660;</button>
          <button class="dz-mini dz-danger" title="删除字段" @click="removeColumn(i)">&times;</button>
        </span>
      </div>
      <div class="dz-empty" v-if="!columns.length">暂无字段，点击「+ 添加字段」新增。</div>
    </div>
  </div>
</template>

<style scoped>
/* 网格列宽：字段名 / 类型 / 允许NULL / 默认值 / 注释 / 自增 / 更新时 / 操作 */
.dz-grid {
  grid-template-columns: 1.3fr 1.3fr 74px 1.1fr 1.3fr 46px 52px 88px;
}
</style>
