<script setup>
// 多选列选择器：表结构设计器里的「列」不再手填，改为从当前表的字段里点选，避免字段名写错；
// 下拉顶部带搜索框，按 fuzzyScore 模糊定位（字段多的表也能快速找到）。
// 点击顺序即列在索引里的先后顺序：已选列在下拉里置顶并标出顺位，再点一次即移出。
//
// 菜单 Teleport 到 body + fixed 定位：设计器弹窗内部有滚动容器，
// 就地渲染的绝对定位菜单会被裁掉（与 IconSelect / ContextMenu 的处理一致）。
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { fuzzyScore } from '@/utils/sql';

const props = defineProps({
  modelValue: { type: Array, default: () => [] },
  // 候选字段名（当前表的所有列）
  options: { type: Array, default: () => [] },
  placeholder: { type: String, default: '选择列' }
});
const emit = defineEmits(['update:modelValue']);

const rootEl = ref(null);
const triggerEl = ref(null);
const menuEl = ref(null);
const searchEl = ref(null);
const open = ref(false);
const keyword = ref('');
const menuStyle = ref({});

const selected = computed(() => props.modelValue || []);
const selectedText = computed(() => selected.value.join(', '));

// 已选在前（按其在索引里的顺序），未选在后；有关键字时两边都按模糊得分过滤/排序，
// 已选项始终保留，方便直接点掉改顺序
const visibleOptions = computed(() => {
  const q = keyword.value.trim();
  const picked = [];
  const rest = [];
  props.options.forEach((name, i) => {
    const rank = selected.value.indexOf(name);
    if (rank >= 0) {
      if (!q || fuzzyScore(name, q) >= 0) picked.push({ name, rank: rank + 1 });
      return;
    }
    rest.push({ name, i, score: q ? fuzzyScore(name, q) : 0 });
  });
  picked.sort((a, b) => a.rank - b.rank);
  const tail = rest
    .filter(it => it.score >= 0)
    .sort((a, b) => b.score - a.score || a.i - b.i);
  return picked.concat(tail);
});

function syncMenuPos() {
  const rect = triggerEl.value && triggerEl.value.getBoundingClientRect();
  if (!rect) return;
  menuStyle.value = {
    position: 'fixed',
    left: rect.left + 'px',
    top: rect.bottom + 4 + 'px',
    width: Math.max(rect.width, 216) + 'px'
  };
}

function addFloatingListeners() {
  window.addEventListener('resize', syncMenuPos);
  // 捕获阶段：设计器弹窗内部滚动时菜单要跟着触发器走
  window.addEventListener('scroll', syncMenuPos, true);
}

function removeFloatingListeners() {
  window.removeEventListener('resize', syncMenuPos);
  window.removeEventListener('scroll', syncMenuPos, true);
}

function openMenu() {
  open.value = true;
  keyword.value = '';
  syncMenuPos();
  addFloatingListeners();
  nextTick(() => {
    syncMenuPos();
    if (searchEl.value) searchEl.value.focus();
  });
}

function close() {
  if (!open.value) return;
  open.value = false;
  removeFloatingListeners();
}

function toggle() {
  if (open.value) close();
  else openMenu();
}

// 点一下加入（追加到末尾，即排在索引列的最后一位），再点一下移出
function pick(name) {
  const list = selected.value.slice();
  const i = list.indexOf(name);
  if (i >= 0) list.splice(i, 1);
  else list.push(name);
  emit('update:modelValue', list);
}

function clearAll() {
  if (!selected.value.length) return;
  emit('update:modelValue', []);
}

// 回车选第一个未选中的命中项（已选项排在最前，直接回车会误删），Esc 收起
function onSearchKeydown(ev) {
  if (ev.key === 'Enter') {
    ev.preventDefault();
    const first = visibleOptions.value.find(o => !o.rank);
    if (first) pick(first.name);
  } else if (ev.key === 'Escape') {
    ev.preventDefault();
    close();
  }
}

// 点组件或菜单以外的地方收起（菜单已 Teleport，需单独判断）
function onDocMousedown(ev) {
  if (rootEl.value && rootEl.value.contains(ev.target)) return;
  if (menuEl.value && menuEl.value.contains(ev.target)) return;
  close();
}

onMounted(() => {
  document.addEventListener('mousedown', onDocMousedown);
});
onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocMousedown);
  removeFloatingListeners();
});
</script>

<template>
  <div ref="rootEl" class="col-picker" :class="{ open }">
    <button
      ref="triggerEl"
      type="button"
      class="col-picker-trigger"
      v-tip="selected.length ? selectedText : placeholder"
      @click="toggle"
    >
      <span class="col-picker-value" :class="{ empty: !selected.length }">
        {{ selected.length ? selectedText : placeholder }}
      </span>
      <span class="col-picker-caret">▾</span>
    </button>

    <Teleport to="body">
      <div v-if="open" ref="menuEl" class="col-picker-menu" :style="menuStyle">
        <input
          ref="searchEl"
          v-model="keyword"
          type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          class="col-picker-search"
          placeholder="输入关键字定位字段"
          @keydown="onSearchKeydown"
        >
        <div class="col-picker-list">
          <div
            v-for="opt in visibleOptions"
            :key="opt.name"
            class="col-picker-option"
            :class="{ picked: opt.rank }"
            @click="pick(opt.name)"
          >
            <span class="col-picker-check">{{ opt.rank ? '✓' : '' }}</span>
            <span class="col-picker-name">{{ opt.name }}</span>
            <span v-if="opt.rank" class="col-picker-rank">{{ opt.rank }}</span>
          </div>
          <div v-if="!visibleOptions.length" class="col-picker-empty">无匹配字段</div>
        </div>
        <div class="col-picker-foot">
          <span class="col-picker-foot-text">
            已选 {{ selected.length }} 列<span v-if="selected.length"> · 点击顺序即索引顺序</span>
          </span>
          <button v-if="selected.length" type="button" class="col-picker-clear" @click="clearAll">清空</button>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.col-picker {
  position: relative;
  min-width: 0;
}

/* 紧凑尺寸对齐同排的输入框 / 下拉（.dz-row 里的控件都是 6px 10px） */
.col-picker-trigger {
  width: 100%;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  background: rgba(var(--void-rgb), .7);
  border: 1px solid var(--line);
  border-radius: 6px;
  color: var(--text);
  font-family: var(--font-ui);
  font-size: 12.5px;
  letter-spacing: .3px;
  text-align: left;
  cursor: pointer;
  transition: border-color .18s, box-shadow .18s;
}

.col-picker.open .col-picker-trigger,
.col-picker-trigger:hover {
  border-color: var(--line-strong);
}

.col-picker.open .col-picker-trigger,
.col-picker-trigger:focus {
  border-color: var(--cyan);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .25), 0 0 16px -4px rgba(var(--cyan-rgb), .6);
  outline: none;
}

.col-picker-value {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.col-picker-value.empty {
  color: var(--text-dim);
  opacity: .75;
}

.col-picker-caret {
  margin-left: auto;
  padding-left: 4px;
  color: var(--text-dim);
  font-size: 11px;
  line-height: 1;
  transition: transform .2s;
}

.col-picker.open .col-picker-caret {
  transform: rotate(180deg);
}

/* z-index 需高于设计器弹窗（含遮罩），否则菜单会被盖住 */
.col-picker-menu {
  z-index: 2100;
  padding: 4px;
  background: var(--panel-solid);
  border: 1px solid var(--line-strong);
  border-radius: 6px;
  box-shadow: 0 14px 34px -14px rgba(var(--cyan-rgb), .55);
}

.col-picker-search {
  width: 100%;
  margin-bottom: 4px;
  padding: 6px 9px;
  background: rgba(var(--void-rgb), .7);
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 12px;
  outline: none;
  transition: border-color .18s, box-shadow .18s;
}

.col-picker-search::placeholder {
  color: var(--text-dim);
  opacity: .75;
}

.col-picker-search:focus {
  border-color: var(--cyan);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .25), 0 0 12px -3px rgba(var(--cyan-rgb), .6);
}

.col-picker-list {
  max-height: 240px;
  overflow-y: auto;
}

.col-picker-option {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  border-radius: 4px;
  color: var(--text);
  font-size: 12.5px;
  letter-spacing: .3px;
  cursor: pointer;
  transition: background .15s, color .15s;
}

.col-picker-option:hover {
  background: rgba(var(--cyan-rgb), .08);
}

.col-picker-option.picked {
  color: var(--cyan);
}

.col-picker-check {
  flex: 0 0 12px;
  font-size: 11px;
  line-height: 1;
  text-align: center;
}

.col-picker-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
}

/* 顺位数字：1 就是索引里的第一列 */
.col-picker-rank {
  margin-left: auto;
  padding: 0 5px;
  border-radius: 999px;
  background: rgba(var(--cyan-rgb), .12);
  color: var(--cyan-soft);
  font-family: var(--font-mono);
  font-size: 10px;
  line-height: 15px;
}

.col-picker-empty {
  padding: 12px 8px;
  color: var(--text-dim);
  font-size: 12px;
  text-align: center;
}

.col-picker-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 4px;
  padding: 6px 8px 2px;
  border-top: 1px solid var(--line);
}

.col-picker-foot-text {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-dim);
  font-size: 11px;
}

.col-picker-clear {
  margin-left: auto;
  padding: 2px 8px;
  background: rgba(var(--overlay-rgb), .03);
  border: 1px solid var(--line);
  border-radius: 999px;
  color: var(--text-dim);
  font-family: var(--font-ui);
  font-size: 11px;
  cursor: pointer;
  transition: all .18s;
}

.col-picker-clear:hover {
  border-color: var(--magenta);
  color: var(--magenta);
  background: rgba(var(--magenta-rgb), .1);
}
</style>
