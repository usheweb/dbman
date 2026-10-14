<script setup>
// 带图标的下拉选择：原生 <select> 的 option 无法显示图片，故自绘。
// options: [{ value, label, icon }]，icon 对应 ItemIcon 的 name（不传则不显示图标）
// searchable：下拉顶部带搜索框，按 fuzzyScore 模糊筛选 label（数据库/表上百个时用）
//
// 菜单 Teleport 到 body + fixed 定位：侧栏 .db-tree 有 overflow-y:auto，
// 就地渲染的绝对定位菜单会被裁掉（与 ContextMenu 的处理方式一致）。
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import ItemIcon from './ItemIcon.vue';
import { fuzzyScore } from '@/utils/sql';

const props = defineProps({
  modelValue: { type: String, default: '' },
  options: { type: Array, default: () => [] },
  searchable: { type: Boolean, default: false },
  placeholder: { type: String, default: '请选择' }
});

const emit = defineEmits(['update:modelValue']);

const rootEl = ref(null);
const triggerEl = ref(null);
const menuEl = ref(null);
const searchEl = ref(null);
const open = ref(false);
const keyword = ref('');
const menuStyle = ref({});

const current = computed(
  () => props.options.find(o => o.value === props.modelValue) || null
);

// 命中项按得分降序：前缀 > 子串 > 顺序子序列（fuzzyScore 不匹配返回 -1）
const visibleOptions = computed(() => {
  const q = keyword.value.trim();
  if (!props.searchable || !q) return props.options;
  return props.options
    .map(opt => ({ opt, score: fuzzyScore(opt.label, q) }))
    .filter(item => item.score >= 0)
    .sort((a, b) => b.score - a.score)
    .map(item => item.opt);
});

// 菜单贴着触发器展开，宽度与其一致
function syncMenuPos() {
  const rect = triggerEl.value && triggerEl.value.getBoundingClientRect();
  if (!rect) return;
  menuStyle.value = {
    position: 'fixed',
    left: rect.left + 'px',
    top: rect.bottom + 4 + 'px',
    width: rect.width + 'px'
  };
}

function addFloatingListeners() {
  window.addEventListener('resize', syncMenuPos);
  // 捕获阶段：祖先容器（侧栏、.db-tree）滚动时菜单要跟着走
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
    if (props.searchable && searchEl.value) searchEl.value.focus();
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

function choose(opt) {
  if (!opt) return;
  emit('update:modelValue', opt.value);
  close();
}

// 回车选中第一个命中项，Esc 收起
function onSearchKeydown(ev) {
  if (ev.key === 'Enter') {
    ev.preventDefault();
    choose(visibleOptions.value[0]);
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
  <div ref="rootEl" class="icon-select" :class="{ open }">
    <button ref="triggerEl" type="button" class="icon-select-trigger" @click="toggle">
      <ItemIcon v-if="current && current.icon" :name="current.icon" />
      <span class="icon-select-label">{{ current ? current.label : placeholder }}</span>
      <span class="icon-select-caret">▾</span>
    </button>

    <Teleport to="body">
      <div v-if="open" ref="menuEl" class="icon-select-menu" :style="menuStyle">
        <input
          v-if="searchable"
          ref="searchEl"
          v-model="keyword"
          type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          class="icon-select-search"
          placeholder="输入关键字筛选"
          @keydown="onSearchKeydown"
        >
        <div class="icon-select-list">
          <div
            v-for="opt in visibleOptions"
            :key="opt.value"
            class="icon-select-option"
            :class="{ active: opt.value === modelValue }"
            @click="choose(opt)"
          >
            <ItemIcon v-if="opt.icon" :name="opt.icon" />
            <span class="icon-select-label">{{ opt.label }}</span>
          </div>
          <div v-if="!visibleOptions.length" class="icon-select-empty">无匹配项</div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.icon-select {
  position: relative;
}

.icon-select-trigger {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  background: rgba(var(--void-rgb), .7);
  border: 1px solid var(--line);
  border-radius: 6px;
  color: var(--text);
  font-size: 14px;
  font-family: var(--font-ui);
  text-align: left;
  cursor: pointer;
  transition: all .2s;
}

.icon-select.open .icon-select-trigger,
.icon-select-trigger:hover {
  border-color: var(--cyan);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .25), 0 0 16px -3px rgba(var(--cyan-rgb), .6);
}

.icon-select-caret {
  margin-left: auto;
  padding-left: 4px;
  color: var(--text-dim);
  font-size: 13px;
  line-height: 1;
  transition: transform .2s;
}

.icon-select.open .icon-select-caret {
  transform: rotate(180deg);
}

/* 位置与宽度由 syncMenuPos 以内联 fixed 定位给出；
   z-index 需高于排序面板等浮层（2000），否则菜单会被盖住 */
.icon-select-menu {
  z-index: 2100;
  padding: 4px;
  background: var(--panel-solid);
  border: 1px solid var(--line-strong);
  border-radius: 6px;
  box-shadow: 0 14px 34px -14px rgba(var(--cyan-rgb), .55);
}

.icon-select-search {
  width: 100%;
  margin-bottom: 4px;
  padding: 7px 9px;
  background: rgba(var(--void-rgb), .7);
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 12px;
  outline: none;
  transition: all .2s;
}

.icon-select-search::placeholder {
  color: var(--text-dim);
}

.icon-select-search:focus {
  border-color: var(--cyan);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .25), 0 0 12px -3px rgba(var(--cyan-rgb), .6);
}

/* 选项上百个时内部滚动，不撑破窗口 */
.icon-select-list {
  max-height: 264px;
  overflow-y: auto;
}

.icon-select-option {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 10px;
  border-radius: 4px;
  color: var(--text);
  font-size: 14px;
  font-family: var(--font-ui);
  cursor: pointer;
  transition: background .15s, color .15s;
}

.icon-select-option:hover {
  background: rgba(var(--cyan-rgb), .08);
}

.icon-select-option.active {
  color: var(--cyan);
  background: rgba(var(--cyan-rgb), .12);
}

.icon-select-label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.icon-select-empty {
  padding: 10px;
  color: var(--text-dim);
  font-size: 12px;
  text-align: center;
}

/* 下拉里图标比列表稍大，更醒目 */
.icon-select :deep(.item-svg) {
  width: 15px;
  height: 15px;
}
</style>
