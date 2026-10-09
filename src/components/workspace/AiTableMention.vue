<script setup>
// 输入框里打「@」弹出的表名候选菜单。
// 菜单 Teleport 到 body + fixed 定位（与 IconSelect 一致），键盘事件由父组件转发进来。
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { fuzzyScore } from '@/utils/sql';

const props = defineProps({
  open: { type: Boolean, default: false },
  query: { type: String, default: '' },
  // [{ tableName, tableComment }]，取 connStore.allTables
  tables: { type: Array, default: () => [] },
  // 输入框 DOM，菜单贴着它展开
  anchorEl: { type: Object, default: null }
});

const emit = defineEmits(['pick', 'close']);

const menuEl = ref(null);
const menuStyle = ref({});
const active = ref(0);

// 命中项按得分降序：前缀 > 子串 > 顺序子序列（fuzzyScore 不匹配返回 -1）
const visibleTables = computed(() => {
  const q = props.query.trim();
  if (!q) return props.tables.slice(0, 50);
  return props.tables
    .map(t => ({ t, score: fuzzyScore(t.tableName, q) }))
    .filter(item => item.score >= 0)
    .sort((a, b) => b.score - a.score)
    .map(item => item.t)
    .slice(0, 50);
});

function syncMenuPos() {
  const rect = props.anchorEl && props.anchorEl.getBoundingClientRect();
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
  // 捕获阶段：祖先容器滚动时菜单要跟着走
  window.addEventListener('scroll', syncMenuPos, true);
}

function removeFloatingListeners() {
  window.removeEventListener('resize', syncMenuPos);
  window.removeEventListener('scroll', syncMenuPos, true);
}

function scrollActiveIntoView() {
  nextTick(() => {
    const el = menuEl.value && menuEl.value.querySelector('.mention-item.active');
    if (el) el.scrollIntoView({ block: 'nearest' });
  });
}

// 由父组件在输入框 keydown 时调用；返回 true 表示该按键已被菜单消费
function handleKeydown(ev) {
  if (!props.open) return false;
  const list = visibleTables.value;
  if (ev.key === 'Escape') {
    emit('close');
    ev.preventDefault();
    return true;
  }
  if (!list.length) return false;
  if (ev.key === 'ArrowDown') {
    active.value = (active.value + 1) % list.length;
    scrollActiveIntoView();
    ev.preventDefault();
    return true;
  }
  if (ev.key === 'ArrowUp') {
    active.value = (active.value - 1 + list.length) % list.length;
    scrollActiveIntoView();
    ev.preventDefault();
    return true;
  }
  if (ev.key === 'Enter') {
    const t = list[active.value] || list[0];
    if (t) emit('pick', t.tableName);
    ev.preventDefault();
    return true;
  }
  return false;
}

function choose(t) {
  emit('pick', t.tableName);
}

// 点菜单或输入框以外的地方收起
function onDocMousedown(ev) {
  if (!props.open) return;
  if (menuEl.value && menuEl.value.contains(ev.target)) return;
  if (props.anchorEl && props.anchorEl.contains(ev.target)) return;
  emit('close');
}

watch(
  () => props.open,
  open => {
    if (open) {
      active.value = 0;
      syncMenuPos();
      addFloatingListeners();
      nextTick(syncMenuPos);
      document.addEventListener('mousedown', onDocMousedown);
    } else {
      removeFloatingListeners();
      document.removeEventListener('mousedown', onDocMousedown);
    }
  }
);

watch([() => props.query, visibleTables], () => { active.value = 0; });

onBeforeUnmount(() => {
  removeFloatingListeners();
  document.removeEventListener('mousedown', onDocMousedown);
});

defineExpose({ handleKeydown });
</script>

<template>
  <Teleport to="body">
    <div v-if="open" ref="menuEl" class="mention-menu" :style="menuStyle">
      <div class="mention-list">
        <div
          v-for="(t, i) in visibleTables"
          :key="t.tableName"
          class="mention-item"
          :class="{ active: i === active }"
          @mousedown.prevent
          @click="choose(t)"
          @mouseenter="active = i"
        >
          <span class="mention-name">{{ t.tableName }}</span>
          <span v-if="t.tableComment" class="mention-comment">{{ t.tableComment }}</span>
        </div>
        <div v-if="!visibleTables.length" class="mention-empty">无匹配项</div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.mention-menu {
  z-index: 1300;
  padding: 4px;
  background: var(--panel-solid);
  border: 1px solid var(--line-strong);
  border-radius: 6px;
  box-shadow: 0 14px 34px -14px rgba(0, 229, 255, .55);
}

.mention-list {
  max-height: 240px;
  overflow-y: auto;
}

.mention-item {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 6px 9px;
  border-radius: 4px;
  cursor: pointer;
  transition: background .15s, color .15s;
}

.mention-item:hover,
.mention-item.active {
  background: rgba(0, 229, 255, .08);
}

.mention-item.active {
  color: var(--cyan);
  background: rgba(0, 229, 255, .12);
}

.mention-name {
  font-family: var(--font-mono);
  font-size: 12px;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mention-comment {
  font-size: 11px;
  color: var(--text-dim);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mention-item.active .mention-comment {
  color: var(--cyan-soft);
}

.mention-empty {
  padding: 10px;
  color: var(--text-dim);
  font-size: 12px;
  text-align: center;
}
</style>
