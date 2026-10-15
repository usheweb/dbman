<script setup>
// 表名 / 字段名写错时的纠错弹窗：按相似度列出候选，用户点选即替换 SQL 里的名字后重跑
import { ref, computed, watch, nextTick } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useWorkspaceStore } from '@/stores/workspace';

const ws = useWorkspaceStore();
const active = ref(0);
const itemRefs = ref([]);

const info = computed(() => ws.identSuggest);
const kindLabel = computed(() => (info.value.kind === 'column' ? '字段' : '表'));
const title = computed(() => `${kindLabel.value}名可能有误`);
const candidates = computed(() => info.value.candidates || []);
const subject = computed(() => (info.value.db ? `${info.value.db}.${info.value.name}` : info.value.name));

function pct(ratio) {
  return Math.round((ratio || 0) * 100);
}

function pick(name) {
  ws.resolveIdentSuggest(name || null);
}

// 「确定」：应用当前选中的候选项
function confirmPick() {
  const c = candidates.value[active.value];
  pick(c ? c.name : null);
}

watch(() => info.value.visible, async (visible) => {
  if (!visible) return;
  active.value = 0;
  await nextTick();
  const el = itemRefs.value[0];
  if (el) el.focus();
});

// 上下键在候选之间移动焦点，回车由按钮自身触发
function onKeydown(e) {
  const list = candidates.value;
  if (!list.length) return;
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    active.value = Math.min(active.value + 1, list.length - 1);
    const el = itemRefs.value[active.value];
    if (el) el.focus();
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    active.value = Math.max(active.value - 1, 0);
    const el = itemRefs.value[active.value];
    if (el) el.focus();
  }
}
</script>

<template>
  <ModalShell :visible="info.visible" width="520px" @close="pick(null)">
    <template #title>{{ title }}</template>
    <div class="confirm-body">
      <div class="confirm-tip">
        未找到{{ kindLabel }} <code class="idf-name">{{ subject }}</code>，你是否想改用下列名称？
      </div>
      <div v-if="candidates.length" class="idf-list" @keydown="onKeydown">
        <button
          v-for="(c, i) in candidates"
          :key="c.name"
          :ref="el => { itemRefs[i] = el; }"
          class="idf-item"
          :class="{ 'idf-item-active': active === i }"
          @click="active = i"
          @dblclick="pick(c.name)"
          @keydown.enter.prevent="pick(c.name)"
          @focus="active = i"
        >
          <span class="idf-cand">{{ c.name }}</span>
          <span class="idf-ratio">
            <span class="idf-bar"><span class="idf-bar-fill" :style="{ width: pct(c.ratio) + '%' }"></span></span>
            <span class="idf-pct">{{ pct(c.ratio) }}%</span>
          </span>
        </button>
      </div>
      <div v-else class="idf-empty">未找到相似的{{ kindLabel }}名，请手动修改 SQL。</div>
    </div>
    <div class="confirm-actions">
      <button class="btn btn-secondary" @click="pick(null)">取消</button>
      <button class="btn" :disabled="!candidates.length" @click="confirmPick">确定</button>
    </div>
  </ModalShell>
</template>

<style scoped>
/* 没有候选时「确定」不可用，需要视觉上能看出来 */
.confirm-actions .btn:disabled {
  opacity: .4;
  cursor: not-allowed;
}

.idf-name {
  padding: 1px 6px;
  border-radius: 4px;
  background: rgba(var(--void-2-rgb), .7);
  border: 1px solid var(--line);
  color: var(--magenta);
  font-family: var(--font-mono);
  font-size: 12px;
}

.idf-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 280px;
  overflow: auto;
}

.idf-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 12px;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: rgba(var(--void-2-rgb), .5);
  color: var(--text);
  cursor: pointer;
  text-align: left;
  transition: border-color .15s, background .15s;
}

.idf-item:hover,
.idf-item-active,
.idf-item:focus-visible {
  outline: none;
  border-color: var(--cyan);
  background: rgba(var(--cyan-rgb), .1);
}

.idf-cand {
  font-family: var(--font-mono);
  font-size: 13px;
  color: var(--text);
  word-break: break-all;
}

.idf-ratio {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
}

.idf-bar {
  display: block;
  width: 72px;
  height: 5px;
  border-radius: 3px;
  background: rgba(var(--void-2-rgb), .9);
  overflow: hidden;
}

.idf-bar-fill {
  display: block;
  height: 100%;
  border-radius: 3px;
  background: var(--cyan);
}

.idf-pct {
  min-width: 34px;
  text-align: right;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-dim);
}

.idf-empty {
  color: var(--text-dim);
  font-size: 13px;
  font-family: var(--font-cjk);
  letter-spacing: .5px;
}
</style>
