<script setup>
// AI 生成栏：一句话描述 → 大模型按需查询表结构 → SQL 写入编辑器（不自动执行）
import { computed } from 'vue';
import { useAiStore } from '@/stores/ai';
import { useUiStore } from '@/stores/ui';

const ai = useAiStore();
const ui = useUiStore();

// 生成中优先显示当前阶段（正在请求模型第几轮），否则显示状态文案
const statusText = computed(() => {
  if (ai.error) return ai.error;
  if (ai.generating && ai.phase === 'thinking' && ai.phaseRound) {
    return `正在请求模型（第 ${ai.phaseRound} 轮）…`;
  }
  return ai.status;
});

async function onGenerate() {
  // 不清空输入框：结果不满意时可直接改提示词重试
  await ai.generate(ai.prompt);
}

// 本地元数据查询常在几十毫秒内：不足 1 秒显示毫秒，否则显示秒（保留 1 位小数）
function formatElapsed(ms) {
  const v = Number(ms) || 0;
  return v < 1000 ? `${Math.round(v)}ms` : `${(v / 1000).toFixed(1)}s`;
}
</script>

<template>
  <div class="ai-bar">
    <div class="ai-bar-row">
      <div class="ai-bar-field">
        <input
          v-model="ai.prompt"
          type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
          class="filter-input ai-bar-input"
          placeholder="用一句话描述需求，例如：查出最近 7 天注册的用户"
          :disabled="ai.generating"
          @keyup.enter="onGenerate"
        >
        <button v-if="ai.prompt" class="ai-bar-clear" title="清空" @click="ai.prompt = ''">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8"/></svg>
        </button>
      </div>
      <button class="btn" :disabled="ai.generating" @click="onGenerate">
        {{ ai.generating ? '生成中…' : 'AI 生成' }}
      </button>
      <button class="btn btn-secondary" @click="ui.openModal('aiConfigModal')">AI 设置</button>
    </div>

    <div v-if="ai.generating || ai.error || ai.status" class="ai-bar-status" :class="{ 'is-error': !!ai.error }">
      <span v-if="ai.generating" class="ai-spinner"></span>
      <span>{{ statusText }}</span>
    </div>

    <!-- 每完成一步立刻出现一条，带该步耗时 -->
    <div v-if="ai.steps.length" class="ai-bar-steps">
      <span v-for="(step, i) in ai.steps" :key="i" class="ai-step">
        <span class="ai-step-tool">{{ step.tool }}</span>{{ step.summary }}
        <span class="ai-step-time">{{ formatElapsed(step.ms) }}</span>
      </span>
    </div>
  </div>
</template>

<style scoped>
.ai-bar {
  padding: 8px 12px;
  border-bottom: 1px solid var(--line);
  background: var(--panel);
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.ai-bar-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* 输入框外层：负责占满剩余宽度，并给右侧清空图标做定位参照 */
.ai-bar-field {
  position: relative;
  flex: 1;
  min-width: 0;
  display: flex;
}

/* 复用 theme.css 的 .filter-input 外观（与表名筛选框一致），这里只覆盖布局 */
.ai-bar-input {
  width: 100%;
  margin-bottom: 0;
  padding-right: 30px; /* 给清空图标留位 */
}

.ai-bar-clear {
  position: absolute;
  right: 8px;
  top: 50%;
  transform: translateY(-50%);
  width: 18px;
  height: 18px;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  background: none;
  border: none;
  color: var(--text-dim);
  cursor: pointer;
  transition: color .15s;
}

.ai-bar-clear:hover {
  color: var(--cyan);
}

.ai-bar-clear svg {
  width: 13px;
  height: 13px;
}

/* 按钮不许被压扁换行 */
.ai-bar-row .btn {
  flex: 0 0 auto;
  white-space: nowrap;
}

.ai-bar-status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--cyan-soft);
}

.ai-bar-status.is-error {
  color: var(--magenta);
}

.ai-bar-steps {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.ai-step {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-dim);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  padding: 1px 6px;
}

.ai-step-tool {
  color: var(--cyan);
  margin-right: 4px;
}

/* 该步耗时：与文案用分隔线隔开 */
.ai-step-time {
  margin-left: 6px;
  padding-left: 6px;
  border-left: 1px solid var(--line);
  color: var(--cyan-soft);
}

.ai-spinner {
  width: 12px;
  height: 12px;
  border: 2px solid var(--line-strong);
  border-top-color: var(--cyan);
  border-radius: 50%;
  animation: ai-spin 0.8s linear infinite;
}

@keyframes ai-spin {
  to { transform: rotate(360deg); }
}
</style>
