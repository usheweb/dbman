<script setup>
// 视图设置：控制表列表里展示哪些信息（表注释 / 数据行数与大小）+ 查询结果历史条数上限
// 修改后立即生效并写入本地存储，不需要额外保存
import ModalShell from '@/components/common/ModalShell.vue';
import { useUiStore } from '@/stores/ui';
import { useWorkspaceStore } from '@/stores/workspace';

const ui = useUiStore();
const ws = useWorkspaceStore();

const OPTIONS = [
  { key: 'tableComment', label: '表注释', hint: '在表名下方显示表注释说明' },
  { key: 'tableMeta', label: '数据行数与大小', hint: '在表名下方显示「行数 · 总大小」' }
];

function toggle(key, checked) {
  ui.saveViewOpts({ [key]: checked });
}

// 历史条数：限制在 1~1000，越界时把输入框的值回写成生效值
function saveHistoryMax(ev) {
  const el = ev.target;
  let n = parseInt(el.value, 10);
  if (!Number.isFinite(n) || n < 1) n = 1;
  if (n > 1000) n = 1000;
  el.value = n;
  ui.saveViewOpts({ resultHistoryMax: n });
  ws.trimHistories();
}
</script>

<template>
  <ModalShell :visible="ui.modals.viewSettingsModal" width="460px" @close="ui.closeModal('viewSettingsModal')">
    <template #title>视图设置</template>

    <div class="vs-list">
      <label v-for="opt in OPTIONS" :key="opt.key" class="vs-row">
        <span class="vs-text">
          <span class="vs-label">{{ opt.label }}</span>
          <span class="vs-hint">{{ opt.hint }}</span>
        </span>
        <input
          type="checkbox"
          class="vs-check"
          :checked="ui.viewOpts[opt.key]"
          @change="toggle(opt.key, $event.target.checked)"
        >
      </label>
      <div class="vs-row vs-row-static">
        <span class="vs-text">
          <span class="vs-label">查询结果历史条数</span>
          <span class="vs-hint">结果区下拉可切换回看的最近查询结果，最多保留多少条</span>
        </span>
        <input
          type="number"
          class="vs-num"
          min="1"
          max="1000"
          step="1"
          :value="ui.viewOpts.resultHistoryMax"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
          @change="saveHistoryMax"
        >
      </div>
    </div>

    <div class="vs-actions">
      <span class="vs-tip">取消勾选即可隐藏，修改后立即生效。</span>
      <button type="button" class="btn btn-secondary" @click="ui.closeModal('viewSettingsModal')">关闭</button>
    </div>
  </ModalShell>
</template>

<style scoped>
.vs-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.vs-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 14px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: rgba(var(--void-rgb), .35);
  cursor: pointer;
  transition: all .2s ease;
}

.vs-row:hover {
  border-color: var(--line-strong);
  background: rgba(var(--void-rgb), .6);
  box-shadow: 0 0 14px -6px rgba(var(--cyan-rgb), .6);
}

.vs-text {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.vs-label {
  font-family: var(--font-cjk);
  font-size: 13px;
  color: var(--text);
}

.vs-hint {
  font-size: 12px;
  color: var(--text-dim);
}

.vs-check {
  flex: 0 0 auto;
  width: 16px;
  height: 16px;
  accent-color: var(--cyan);
  cursor: pointer;
}

/* 数字输入行不可点击整行，去掉 hand 光标 */
.vs-row-static {
  cursor: default;
}

.vs-num {
  flex: 0 0 auto;
  width: 88px;
  padding: 5px 8px;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: rgba(var(--void-rgb), .5);
  color: var(--text);
  font-family: var(--font-cjk);
  font-size: 13px;
  text-align: center;
  outline: none;
  transition: all .2s ease;
}

.vs-num:focus {
  border-color: var(--cyan);
  box-shadow: var(--glow-cyan);
}

.vs-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 18px;
}

.vs-tip {
  font-size: 12px;
  color: var(--text-dim);
}
</style>