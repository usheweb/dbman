<script setup>
// 表列表排序面板：维护一组排序键（字段 + 升/降序），靠前者优先级高
// 面板挂到 body 上（侧栏有 overflow 裁切，不能在里面做绝对定位的弹层）
import { computed, nextTick, onMounted, onBeforeUnmount, ref } from 'vue';
import { TABLE_SORT_FIELDS, DEFAULT_TABLE_SORT, normalizeSortKeys } from '@/utils/tableSort';
import IconSelect from '@/components/common/IconSelect.vue';

const props = defineProps({
  modelValue: { type: Array, default: () => [] },
  // 触发按钮元素：用于定位，也用于判断「点击是否发生在面板外」
  anchor: { type: Object, default: null }
});

const emit = defineEmits(['update:modelValue', 'close']);

const panelEl = ref(null);
const left = ref(4);
const top = ref(4);

// 贴住触发按钮右侧展开；下方空间不足时向上翻转
function place() {
  const a = props.anchor;
  const el = panelEl.value;
  if (!a || !el) return;
  const rect = a.getBoundingClientRect();
  const w = el.offsetWidth;
  const h = el.offsetHeight;
  let l = rect.right - w;
  if (l + w > window.innerWidth - 4) l = window.innerWidth - 4 - w;
  if (l < 4) l = 4;
  let t = rect.bottom + 6;
  if (t + h > window.innerHeight - 4) t = Math.max(4, rect.top - 6 - h);
  left.value = l;
  top.value = t;
}

// 点面板与触发按钮以外的地方关闭
function onDocMousedown(ev) {
  const t = ev.target;
  if (panelEl.value && panelEl.value.contains(t)) return;
  if (props.anchor && props.anchor.contains(t)) return;
  // 维度下拉的菜单 Teleport 到 body，不在面板内，需放行
  if (t instanceof Element && t.closest('.icon-select-menu')) return;
  emit('close');
}

onMounted(async () => {
  await nextTick();
  place();
  document.addEventListener('mousedown', onDocMousedown);
});
onBeforeUnmount(() => document.removeEventListener('mousedown', onDocMousedown));

const keys = computed(() => normalizeSortKeys(props.modelValue));
const usedFields = computed(() => keys.value.map(k => k.field));
const canAdd = computed(() => keys.value.length < TABLE_SORT_FIELDS.length);

// 某一行可选的字段：本行已选 + 其他行未占用的，避免重复维度
function optionsFor(index) {
  const others = usedFields.value.filter((_, i) => i !== index);
  return TABLE_SORT_FIELDS.filter(f => !others.includes(f.field));
}

function commit(next) {
  emit('update:modelValue', next);
  nextTick(place);
}

function setField(index, field) {
  const next = keys.value.slice();
  next[index] = { ...next[index], field };
  commit(next);
}

function toggleDir(index) {
  const next = keys.value.slice();
  const dir = next[index].dir === 'asc' ? 'desc' : 'asc';
  next[index] = { ...next[index], dir };
  commit(next);
}

function removeKey(index) {
  const next = keys.value.slice();
  next.splice(index, 1);
  commit(next);
}

// 与相邻行交换：delta -1 提高优先级（上移），1 降低优先级（下移）
function move(index, delta) {
  const target = index + delta;
  if (target < 0 || target >= keys.value.length) return;
  const next = keys.value.slice();
  const [item] = next.splice(index, 1);
  next.splice(target, 0, item);
  commit(next);
}

function addKey() {
  const used = usedFields.value;
  const free = TABLE_SORT_FIELDS.find(f => !used.includes(f.field));
  if (!free) return;
  commit([...keys.value, { field: free.field, dir: 'asc' }]);
}

function reset() {
  commit(DEFAULT_TABLE_SORT.map(k => ({ ...k })));
}
</script>

<template>
  <Teleport to="body">
    <div ref="panelEl" class="sort-panel" :style="{ left: left + 'px', top: top + 'px' }">
      <div class="sort-head">
        <span>排序（前者优先）</span>
        <button class="sort-close" v-tip="'关闭'" @click="emit('close')">✕</button>
      </div>

      <div class="sort-list">
        <div v-if="!keys.length" class="sort-empty">未设置排序，按数据库返回顺序显示</div>
        <div v-for="(k, i) in keys" :key="k.field" class="sort-row">
          <span class="sort-priority">{{ i + 1 }}</span>
          <IconSelect
            class="sort-field"
            :model-value="k.field"
            :options="optionsFor(i).map(f => ({ value: f.field, label: f.label }))"
            @update:model-value="setField(i, $event)"
          />
          <button
            class="sort-btn sort-text"
            v-tip="k.dir === 'asc' ? '当前升序，点击切换为降序' : '当前降序，点击切换为升序'"
            @click="toggleDir(i)"
          >{{ k.dir === 'asc' ? '升序' : '降序' }}</button>
          <button
            class="sort-btn sort-text"
            :disabled="keys.length < 2"
            v-tip="keys.length < 2
              ? '只有一个排序维度，无需调整顺序'
              : (i === 0 ? '下移一位，降低该维度的排序优先级' : '上移一位，提高该维度的排序优先级')"
            @click="move(i, i === 0 ? 1 : -1)"
          >{{ i === 0 ? '下移' : '上移' }}</button>
          <button class="sort-btn sort-remove" v-tip="'移除该排序维度'" @click="removeKey(i)">✕</button>
        </div>
      </div>

      <div class="sort-foot">
        <button class="sort-btn" :disabled="!canAdd" @click="addKey">+ 添加排序维度</button>
        <button class="sort-btn" v-tip="'恢复为按表名升序'" @click="reset">重置</button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.sort-panel {
  position: fixed;
  z-index: 2000;
  width: 300px;
  box-sizing: border-box;
  padding: 8px;
  background: rgba(var(--ink-4-rgb), .98);
  border: 1px solid var(--line-strong, var(--line));
  border-radius: 8px;
  box-shadow: 0 12px 32px rgba(var(--shadow-rgb), .5);
}

.sort-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 2px 4px 8px;
  font-size: 11px;
  color: var(--text-dim);
  letter-spacing: .5px;
}

.sort-btn {
  flex: 0 0 auto;
  background: transparent;
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--text-dim);
  font-size: 11px;
  line-height: 1;
  padding: 4px 5px;
  white-space: nowrap;
  cursor: pointer;
  transition: all .15s ease;
}

.sort-btn:hover:not(:disabled) {
  color: var(--cyan);
  border-color: var(--line-strong, var(--line));
}

.sort-remove:hover {
  color: var(--magenta);
}

/* 文字按钮（升序/降序、上移）：统一宽度，避免文案变化时行内抖动 */
.sort-text {
  min-width: 36px;
  font-family: var(--font-cjk);
}

.sort-btn:disabled {
  opacity: .3;
  cursor: default;
}

.sort-list {
  max-height: 220px;
  overflow-y: auto;
}

.sort-empty {
  padding: 6px 4px;
  font-size: 11px;
  color: var(--text-dim);
}

.sort-row {
  display: flex;
  align-items: center;
  gap: 5px;
  margin-bottom: 5px;
}

.sort-priority {
  flex: 0 0 auto;
  width: 12px;
  text-align: center;
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-dim);
}

/* 排序维度选择：复用数据库选择器（IconSelect），压成面板行高 */
.sort-field {
  flex: 1 1 auto;
  min-width: 0;
}

.sort-field :deep(.icon-select-trigger) {
  gap: 4px;
  padding: 4px 6px;
  border-radius: 4px;
  font-size: 12px;
}

.sort-field :deep(.icon-select-caret) {
  color: var(--text-dim);
  opacity: 1;
}

.sort-foot {
  display: flex;
  align-items: center;
  gap: 6px;
  padding-top: 6px;
  border-top: 1px solid var(--line);
}
</style>
