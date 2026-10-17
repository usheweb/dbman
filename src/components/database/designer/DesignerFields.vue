<script setup>
import { computed } from 'vue';
import { newColumn, hasAutoInc, hasOnUpdate, isGenerated } from '@/utils/ddl';
import IconSelect from '@/components/common/IconSelect.vue';
import { useTableStore } from '@/stores/table';
import { useUiStore } from '@/stores/ui';

const props = defineProps({
  columns: { type: Array, required: true }
});
const emit = defineEmits(['change']);

const table = useTableStore();
const ui = useUiStore();

// 常用基础类型（不带参数）；长度 / 精度由右侧独立的参数输入框负责
const BASE_TYPES = [
  'int', 'int unsigned', 'bigint', 'bigint unsigned', 'tinyint', 'smallint', 'mediumint',
  'decimal', 'numeric', 'float', 'double',
  'varchar', 'char', 'text', 'tinytext', 'mediumtext', 'longtext', 'json',
  'date', 'datetime', 'timestamp', 'time', 'year',
  'binary', 'varbinary', 'bit', 'blob', 'tinyblob', 'mediumblob', 'longblob',
  'enum', 'set'
];

// 需要括号参数的类型 → [新建时的默认值, 输入框提示]；不在表里的类型没有参数
const TYPE_ARGS = {
  varchar: ['255', '长度'], char: ['1', '长度'],
  binary: ['1', '长度'], varbinary: ['16', '长度'], bit: ['1', '位数'],
  tinyint: ['', '显示宽度'], smallint: ['', '显示宽度'], mediumint: ['', '显示宽度'],
  int: ['', '显示宽度'], integer: ['', '显示宽度'], bigint: ['', '显示宽度'],
  decimal: ['10,2', '精度,小数位'], numeric: ['10,2', '精度,小数位'],
  float: ['', '精度,小数位'], double: ['', '精度,小数位'],
  enum: ["'a','b'", '取值'], set: ["'a','b'", '取值']
};

// 下拉里的哨兵值：选中后弹输入框，让用户填列表外的类型
const CUSTOM_TYPE = '\u0000custom';

const KEY_LABEL = { PRI: '主键', UNI: '唯一', MUL: '索引' };

// 拆类型：int(11) unsigned → head=int / args=11 / tail=unsigned
// head 只取第一个词，其余（unsigned、zerofill、precision…）都算后缀，重组时保证参数写在类型名后面
function splitType(raw) {
  const s = String(raw || '').trim().replace(/\s+/g, ' ');
  let headPart = s, args = '', tail = '';
  const open = s.indexOf('(');
  if (open >= 0) {
    const close = s.indexOf(')', open);
    headPart = s.slice(0, open).trim();
    args = (close >= 0 ? s.slice(open + 1, close) : s.slice(open + 1)).trim();
    tail = close >= 0 ? s.slice(close + 1).trim() : '';
  }
  const sp = headPart.indexOf(' ');
  return {
    head: sp < 0 ? headPart : headPart.slice(0, sp),
    tail: sp < 0 ? '' : headPart.slice(sp + 1).trim(),
    args
  };
}

// 下拉展示值：类型名 + 后缀（不含参数），如 int unsigned
function typeBase(raw) {
  const { head, tail } = splitType(raw);
  return tail ? `${head} ${tail}` : head;
}

// 参数输入框的值
function typeArgs(raw) {
  return splitType(raw).args;
}

// 用「类型名 + 参数」重组出完整类型，参数始终插在类型名与后缀之间
function composeType(base, args) {
  const { head, tail } = splitType(base);
  const a = String(args == null ? '' : args).trim();
  return head + (a ? `(${a})` : '') + tail;
}

function typeArgMeta(c) {
  const meta = TYPE_ARGS[splitType(c.columnType).head.toLowerCase()];
  return meta ? { def: meta[0], hint: meta[1] } : null;
}

// 参数框显隐：该类型支持参数，或当前类型本来就带参数（如老表的 time(3)）时显示
function showArgs(c) {
  return !!typeArgs(c.columnType) || !!typeArgMeta(c);
}

function argsPlaceholder(c) {
  const meta = typeArgMeta(c);
  return meta ? meta.hint : '参数';
}

// 每行的选项：当前值一定在列表里（编辑老表时会出现列表外的类型），末尾再给一个「自定义类型」
function typeOptions(c) {
  const cur = typeBase(c.columnType);
  const list = cur && !BASE_TYPES.includes(cur) ? [cur, ...BASE_TYPES] : BASE_TYPES;
  return [...list.map(t => ({ value: t, label: t })), { value: CUSTOM_TYPE, label: '自定义类型…' }];
}

// 选类型：类型名没变就保留原参数，换了类型则套用该类型的默认参数
async function setTypeBase(c, val) {
  if (val === CUSTOM_TYPE) {
    const input = await table.askText({
      title: '自定义列类型',
      label: '完整类型定义',
      placeholder: '如 varchar(128)、decimal(18,4)',
      value: c.columnType || '',
      confirmText: '确定'
    });
    const text = String(input == null ? '' : input).trim();
    if (!text) return;
    c.columnType = text;
  } else {
    const same = typeBase(c.columnType).toLowerCase() === val.toLowerCase();
    const meta = TYPE_ARGS[splitType(val).head.toLowerCase()];
    const args = same ? typeArgs(c.columnType) : (meta ? meta[0] : '');
    c.columnType = composeType(val, args);
  }
  touch();
}

// 改参数：只动括号里的内容，类型名与后缀原样保留
function setTypeArgs(c, val) {
  c.columnType = composeType(typeBase(c.columnType), val);
  touch();
}

function touch() {
  emit('change');
}

function addColumn() {
  props.columns.push(newColumn());
  touch();
}

async function removeColumn(i) {
  const c = props.columns[i];
  if (c.name) {
    const ok = await ui.askConfirm({
      title: '删除字段',
      message: `确定删除字段「${c.name}」吗？执行后该字段及其数据将一并移除。`,
      confirmText: '删除',
      danger: true
    });
    if (!ok) return;
  }
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
          <span v-if="c.columnKey" class="dz-key" v-tip="KEY_LABEL[c.columnKey]">{{ c.columnKey }}</span>
        </span>
        <span class="dz-type">
          <IconSelect
            class="dz-type-select"
            :model-value="typeBase(c.columnType)"
            :options="typeOptions(c)"
            placeholder="选择类型"
            searchable
            @update:model-value="val => setTypeBase(c, val)"
          />
          <input
            v-if="showArgs(c)"
            class="dz-args"
            type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
            :value="typeArgs(c.columnType)"
            :placeholder="argsPlaceholder(c)"
            v-tip="argsPlaceholder(c)"
            @input="setTypeArgs(c, $event.target.value)"
          >
        </span>
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
          <button class="dz-mini" v-tip="'上移'" :disabled="isFirstRow(i)" @click="move(i, -1)">&#9650;</button>
          <button class="dz-mini" v-tip="'下移'" :disabled="isLastRow(i)" @click="move(i, 1)">&#9660;</button>
          <button class="dz-mini dz-danger" v-tip="'删除字段'" @click="removeColumn(i)">&times;</button>
        </span>
      </div>
      <div class="dz-empty" v-if="!columns.length">暂无字段，点击「+ 添加字段」新增。</div>
    </div>
  </div>
</template>

<style scoped>
/* 网格列宽：字段名 / 类型(下拉+参数) / 允许NULL / 默认值 / 注释 / 自增 / 更新时 / 操作 */
.dz-grid {
  grid-template-columns: 1.2fr 1.7fr 74px 1.05fr 1.2fr 46px 52px 88px;
}

/* 类型单元格：左边选类型，右边改括号里的长度 / 精度 */
.dz-type {
  display: flex;
  gap: 4px;
  align-items: center;
  min-width: 0;
}

.dz-type .dz-type-select {
  flex: 1 1 auto;
  min-width: 0;
}

.dz-type .dz-args {
  flex: 0 0 auto;
  width: 62px;
  text-align: center;
}
</style>
