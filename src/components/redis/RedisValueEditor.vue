<script setup>
// Redis 值编辑器：按类型给不同的编辑方式
//   string → 多行文本框
//   list   → 有序的逐行输入（可增删）
//   set    → 逐行输入（可增删）
//   zset   → 成员 + 分值成对编辑
//   hash   → 字段 + 值成对编辑
// 提交时统一还原成 redis_get 返回的原始结构，交给后端原子重写。
import { computed, ref, watch } from 'vue';

const props = defineProps({
  type: { type: String, required: true },
  value: { type: [String, Array, Object], default: null },
  saving: { type: Boolean, default: false }
});

const emit = defineEmits(['save', 'cancel']);

const draft = ref(null);
// 列表型内容的关键词筛选：只影响显示，draft 始终是完整数据，保存写入不受筛选影响
const filter = ref('');

// 后端 zset 返回 [member, score, member, score, ...]，转成成对结构便于编辑
function toPairs(v) {
  const arr = Array.isArray(v) ? v : [];
  const rows = [];
  for (let i = 0; i < arr.length; i += 2) {
    rows.push({ member: String(arr[i] ?? ''), score: String(arr[i + 1] ?? '0') });
  }
  return rows;
}

function initDraft() {
  // 换了 key / 值重新载入，旧关键词会误筛掉新内容，一并清空
  filter.value = '';
  const v = props.value;
  switch (props.type) {
    case 'string':
      draft.value = v == null ? '' : String(v);
      break;
    case 'list':
    case 'set':
      draft.value = (Array.isArray(v) ? v : []).map(String);
      break;
    case 'zset':
      draft.value = toPairs(v);
      break;
    case 'hash':
      draft.value = Object.entries(v || {}).map(([field, val]) => ({ field, value: String(val ?? '') }));
      break;
    default:
      draft.value = null;
  }
}

watch(() => props.value, initDraft, { immediate: true });

const rows = computed(() => (Array.isArray(draft.value) ? draft.value : []));

// 一行是否命中关键词：按各行实际有几列分别匹配
function matchRow(item, q) {
  if (typeof item === 'string') return item.toLowerCase().includes(q);
  if (props.type === 'zset') {
    return String(item.member).toLowerCase().includes(q) || String(item.score).toLowerCase().includes(q);
  }
  return String(item.field).toLowerCase().includes(q) || String(item.value).toLowerCase().includes(q);
}

// 可见行带上原始下标：筛选后下标仍指向 draft 里的真实位置，增删/上移下移才不会错位
const visibleRows = computed(() => {
  const q = filter.value.trim().toLowerCase();
  const all = rows.value.map((item, index) => ({ item, index }));
  return q ? all.filter(r => matchRow(r.item, q)) : all;
});

function addRow() {
  // 新行是空的，筛选状态下会被直接滤掉，先清掉关键词让新行可见
  filter.value = '';
  if (props.type === 'zset') draft.value.push({ member: '', score: '0' });
  else if (props.type === 'hash') draft.value.push({ field: '', value: '' });
  else draft.value.push('');
}

function removeRow(index) {
  draft.value.splice(index, 1);
}

function move(index, delta) {
  const target = index + delta;
  if (target < 0 || target >= draft.value.length) return;
  const arr = draft.value;
  const [item] = arr.splice(index, 1);
  arr.splice(target, 0, item);
}

// 逐列校验：只拦会写坏数据的输入，其余交给后端
const error = computed(() => {
  if (props.type === 'set') {
    // SADD 会自动去重，先拦下来避免「保存后元素悄悄变少」
    const items = draft.value || [];
    if (new Set(items).size !== items.length) return '集合元素不能重复';
  }
  if (props.type === 'zset') {
    if (rows.value.some(r => !r.member)) return '成员不能为空';
    if (rows.value.some(r => r.score === '' || Number.isNaN(Number(r.score)))) return '分值必须是数字';
    // ZADD 对重复成员是覆盖分值，同样先拦下
    if (new Set(rows.value.map(r => r.member)).size !== rows.value.length) return '成员不能重复';
  }
  if (props.type === 'hash') {
    const fields = rows.value.map(r => r.field);
    if (fields.some(f => !f)) return '字段名不能为空';
    if (new Set(fields).size !== fields.length) return '字段名不能重复';
  }
  return '';
});

// 还原成后端约定的结构
function buildValue() {
  switch (props.type) {
    case 'string':
      return draft.value;
    case 'list':
    case 'set':
      return draft.value.slice();
    case 'zset':
      return draft.value.flatMap(r => [r.member, r.score]);
    case 'hash':
      return Object.fromEntries(draft.value.map(r => [r.field, r.value]));
    default:
      return null;
  }
}

function submit() {
  if (error.value || props.saving) return;
  emit('save', buildValue());
}
</script>

<template>
  <div class="redis-editor">
    <!-- 列表型内容（list/set/zset/hash）：关键词筛选，只筛显示不动数据 -->
    <div v-if="type !== 'string'" class="redis-editor-filter">
      <input
        v-model="filter"
        type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
        class="redis-filter-input"
        placeholder="输入关键词筛选"
      >
      <span v-if="filter.trim()" class="redis-filter-count">命中 {{ visibleRows.length }} / {{ rows.length }}</span>
    </div>

    <!-- string：单值多行文本 -->
    <textarea
      v-if="type === 'string'"
      v-model="draft"
      class="redis-editor-text large-textarea"
      autocapitalize="off" autocorrect="off" spellcheck="false"
      placeholder="内容"
    ></textarea>

    <!-- list / set：逐行字符串 -->
    <div v-else-if="type === 'list' || type === 'set'" class="redis-editor-list">
      <div v-for="row in visibleRows" :key="row.index" class="redis-row">
        <span class="redis-row-idx">{{ type === 'list' ? row.index : '•' }}</span>
        <input
          v-model="draft[row.index]"
          type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          class="redis-row-input"
          :placeholder="'元素 ' + (row.index + 1)"
        >
        <button v-if="type === 'list'" class="row-btn" title="上移" :disabled="row.index === 0" @click="move(row.index, -1)">↑</button>
        <button v-if="type === 'list'" class="row-btn" title="下移" :disabled="row.index === draft.length - 1" @click="move(row.index, 1)">↓</button>
        <button class="row-btn danger" title="删除该行" @click="removeRow(row.index)">×</button>
      </div>
      <div v-if="!visibleRows.length" class="redis-editor-empty">无匹配元素</div>
    </div>

    <!-- zset：成员 + 分值 -->
    <div v-else-if="type === 'zset'" class="redis-editor-list">
      <div v-for="row in visibleRows" :key="row.index" class="redis-row">
        <span class="redis-row-idx">{{ row.index }}</span>
        <input
          v-model="row.item.member"
          type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          class="redis-row-input"
          placeholder="成员"
        >
        <input
          v-model="row.item.score"
          type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          class="redis-row-input score"
          placeholder="分值"
        >
        <button class="row-btn danger" title="删除该行" @click="removeRow(row.index)">×</button>
      </div>
      <div v-if="!visibleRows.length" class="redis-editor-empty">无匹配元素</div>
    </div>

    <!-- hash：字段 + 值 -->
    <div v-else-if="type === 'hash'" class="redis-editor-list">
      <div v-for="row in visibleRows" :key="row.index" class="redis-row">
        <input
          v-model="row.item.field"
          type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          class="redis-row-input"
          placeholder="字段"
        >
        <input
          v-model="row.item.value"
          type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          class="redis-row-input"
          placeholder="值"
        >
        <button class="row-btn danger" title="删除该行" @click="removeRow(row.index)">×</button>
      </div>
      <div v-if="!visibleRows.length" class="redis-editor-empty">无匹配元素</div>
    </div>

    <p v-if="error" class="redis-editor-err">{{ error }}</p>

    <div class="redis-editor-actions">
      <button class="btn btn-secondary" @click="addRow">
        {{ type === 'hash' ? '+ 添加字段' : (type === 'zset' ? '+ 添加成员' : '+ 添加元素') }}
      </button>
      <span class="redis-editor-spacer"></span>
      <button class="btn btn-secondary" :disabled="saving" @click="emit('cancel')">取消</button>
      <button class="btn" :disabled="saving || !!error" @click="submit">
        {{ saving ? '保存中…' : '保存' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.redis-editor {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

/* string：占满剩余高度，内部滚动 */
.redis-editor-text {
  flex: 1;
  min-height: 0;
  resize: none;
  white-space: pre-wrap;
}

/* 列表型内容顶部：关键词筛选 */
.redis-editor-filter {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.redis-filter-input {
  flex: 1;
  min-width: 0;
  padding: 6px 9px;
  background: rgba(var(--void-rgb), .6);
  border: 1px solid var(--line);
  border-radius: 5px;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 12px;
  outline: none;
  transition: all .2s;
}

.redis-filter-input::placeholder {
  color: var(--text-dim);
}

.redis-filter-input:focus {
  border-color: var(--cyan);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .25);
}

.redis-filter-count {
  flex: 0 0 auto;
  color: var(--cyan);
  font-family: var(--font-mono);
  font-size: 11px;
}

.redis-editor-empty {
  padding: 12px;
  color: var(--text-dim);
  font-size: 12px;
  text-align: center;
}

.redis-editor-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding-right: 4px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: rgba(var(--void-rgb), .72);
  padding: 8px;
}

.redis-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
}

.redis-row-idx {
  flex: 0 0 auto;
  min-width: 22px;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-dim);
  text-align: right;
}

.redis-row-input {
  flex: 1;
  min-width: 0;
  padding: 6px 9px;
  background: rgba(var(--void-rgb), .6);
  border: 1px solid var(--line);
  border-radius: 5px;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 12px;
  outline: none;
  transition: all .2s;
}

.redis-row-input.score {
  flex: 0 0 110px;
}

.redis-row-input:focus {
  border-color: var(--cyan);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .25);
}

.redis-editor-err {
  margin-top: 6px;
  color: var(--magenta);
  font-size: 12px;
}

.redis-editor-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 10px;
}

.redis-editor-spacer {
  flex: 1;
}
</style>