// AI：大模型配置（本地持久化）+ 生成状态
// 请求统一走 Rust（api.aiGenerateSql），密钥不进 js 之外的任何地方，也不直连上游。
import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import { listen } from '@tauri-apps/api/event';
import * as api from '@/api';
import { useStoreStore } from './store';
import { useUiStore } from './ui';
import { useConnectionStore } from './connection';
import { useWorkspaceStore } from './workspace';

export const DEFAULT_AI_CONFIG = {
  baseUrl: '',
  apiKey: '',
  model: '',
  timeoutSecs: 60,
  maxRounds: 6
};

export const useAiStore = defineStore('ai', () => {
  const store = useStoreStore();
  const ui = useUiStore();
  const conn = useConnectionStore();
  const ws = useWorkspaceStore();

  const config = ref({ ...DEFAULT_AI_CONFIG });
  const generating = ref(false);
  const status = ref('');
  const error = ref('');
  // 描述输入框内容：放 store 里，收藏当前 SQL 时可作为备注默认值
  const prompt = ref('');
  // 实时步骤：每完成一步由 Rust 推送后立刻追加，不等全部结束
  // { tool, summary, ms }（ms 为该步执行的毫秒数）
  const steps = ref([]);
  // '' 空闲 | 'thinking' 正在请求模型；phaseRound 为当前轮次
  const phase = ref('');
  const phaseRound = ref(0);
  const lastUsage = ref(null);
  // 后端是否留有本连接 + 本库的会话上下文（决定按钮显示「AI 生成」还是「继续微调」）
  const hasSession = ref(false);
  const turn = ref(0);

  // 换连接或换库就不该再沿用上一轮的上下文
  watch(
    () => [conn.currentConnId, conn.currentDatabase],
    () => { hasSession.value = false; turn.value = 0; }
  );

  function init() {
    Object.assign(config.value, DEFAULT_AI_CONFIG, store.get('dbman_ai_config') || {});
    bindProgress();
  }

  // 监听 Rust 侧 `ai:progress`：step 追加一条步骤，thinking 更新当前阶段
  let binding = null;
  function bindProgress() {
    if (binding) return;
    binding = listen('ai:progress', ev => {
      const p = ev.payload || {};
      if (p.phase === 'step') {
        steps.value.push({
          tool: p.tool || '',
          summary: p.summary || '',
          ms: Number(p.elapsed_ms) || 0
        });
      } else if (p.phase === 'thinking') {
        phase.value = 'thinking';
        phaseRound.value = p.round || 0;
      }
    });
  }

  function isConfigured() {
    return !!(config.value.baseUrl && config.value.apiKey && config.value.model);
  }

  function saveConfig(next) {
    config.value = { ...DEFAULT_AI_CONFIG, ...(next || config.value) };
    store.set('dbman_ai_config', { ...config.value });
  }

  async function testConfig(next) {
    await api.aiTestConfig({ ...DEFAULT_AI_CONFIG, ...(next || config.value) });
  }

  // 生成 SQL 并写入编辑器（不自动执行，用户审阅后自行 Ctrl+Enter）
  // keepSession 为 true 且后端留有上下文时，在上一次会话基础上继续（追问式微调）
  async function generate(prompt, keepSession = false) {
    const text = String(prompt || '').trim();
    if (!text) {
      ui.showToast('请先描述你的查询需求', 'error');
      return false;
    }
    if (!isConfigured()) {
      ui.showToast('请先完成 AI 模型设置', 'error');
      ui.openModal('aiConfigModal');
      return false;
    }
    if (!conn.currentDatabase) {
      ui.showToast('请先选择要操作的数据库', 'error');
      return false;
    }

    generating.value = true;
    error.value = '';
    steps.value = [];
    phase.value = '';
    phaseRound.value = 0;
    lastUsage.value = null;
    status.value = keepSession && hasSession.value
      ? '正在基于上一轮结果微调…'
      : '正在生成 SQL…（模型会先查询表结构）';
    try {
      const res = await conn.withRetry(id => api.aiGenerateSql(
        id, conn.currentDatabase, text, { ...config.value }, !!keepSession
      ));
      lastUsage.value = res.usage || null;
      hasSession.value = !!res.hasSession;
      turn.value = res.turn || 0;
      ws.setSql(res.sql);
      status.value = `已生成（第 ${res.turn || 1} 轮上下文，${res.rounds} 次模型调用）`;
      ui.showToast('SQL 已生成并写入编辑器', 'success');
      return true;
    } catch (err) {
      // 失败时保留上一轮可用上下文，用户可直接重试
      error.value = String((err && err.message) || err);
      status.value = '';
      ui.showToast('生成失败：' + error.value, 'error');
      return false;
    } finally {
      generating.value = false;
      phase.value = '';
    }
  }

  function reset() {
    status.value = '';
    error.value = '';
    steps.value = [];
    phase.value = '';
    phaseRound.value = 0;
    lastUsage.value = null;
    hasSession.value = false;
    turn.value = 0;
  }

  return {
    config, generating, status, error, prompt, steps, phase, phaseRound, lastUsage,
    hasSession, turn,
    init, isConfigured, saveConfig, testConfig, generate, reset
  };
});
