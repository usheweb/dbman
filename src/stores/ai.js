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

  // ===== 查询 Tab 隔离 =====
  // 每个查询 Tab 各自的 AI 栏状态（prompt / 状态 / 步骤），切 Tab 时换入换出
  const tabStates = {};
  // 是否有生成请求在途（全局同一时刻只允许一个）；genTabId 为它所属的 Tab
  let inflight = false;
  let genTabId = null;
  let genRound = 0;
  let bgSteps = [];

  const BLANK_TAB = () => ({ prompt: '', status: '', error: '', steps: [] });

  // 把当前 live 的 AI 栏状态存回它所属的 Tab（切 Tab / 换连接前调用）
  function saveTabState(tabId) {
    if (!tabId) return;
    tabStates[tabId] = {
      prompt: prompt.value,
      status: status.value,
      error: error.value,
      steps: steps.value
    };
  }

  // 换入某个 Tab 的 AI 栏状态；tabId 为空表示当前没有查询 Tab
  function loadTabState(tabId) {
    if (!tabId) {
      prompt.value = ''; status.value = ''; error.value = ''; steps.value = [];
      generating.value = false; phase.value = ''; phaseRound.value = 0; lastUsage.value = null;
      hasSession.value = false; turn.value = 0;
      return;
    }
    const st = tabStates[tabId] || (tabStates[tabId] = BLANK_TAB());
    prompt.value = st.prompt;
    status.value = st.status;
    error.value = st.error;
    steps.value = st.steps;
    // 该 Tab 自己的生成任务还在跑：恢复生成中的界面
    const mine = inflight && genTabId === tabId;
    generating.value = mine;
    phase.value = mine ? 'thinking' : '';
    phaseRound.value = mine ? genRound : 0;
    lastUsage.value = null;
    // 后端会话按「连接 + 库」共享，不跨 Tab 还原上下文按钮状态，避免串上下文
    hasSession.value = false;
    turn.value = 0;
  }

  function dropTabState(tabId) {
    if (tabId) delete tabStates[tabId];
  }

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
        const step = { tool: p.tool || '', summary: p.summary || '', ms: Number(p.elapsed_ms) || 0 };
        // 生成所属的 Tab 已切走：先攒进缓冲，结束时整体写回它，不打扰当前 Tab
        if (inflight && genTabId !== ws.activeTabId) bgSteps.push(step);
        else steps.value.push(step);
      } else if (p.phase === 'thinking') {
        genRound = p.round || 0;
        if (inflight && genTabId !== ws.activeTabId) return;
        phase.value = 'thinking';
        phaseRound.value = genRound;
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
    // 生成是全局单任务（后端会话不并发）：切到别的 Tab 后按钮虽可用，也不许再起一个
    if (inflight) {
      ui.showToast('已有生成任务正在进行中，请稍候', 'error');
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
    // 记下发起时的 Tab：结果回来时用户可能已经切走
    const tabId = ws.activeTabId;
    inflight = true;
    genTabId = tabId;
    genRound = 0;
    bgSteps = [];
    const backgrounded = () => genTabId !== ws.activeTabId;
    const bgState = () => tabStates[genTabId] || (tabStates[genTabId] = BLANK_TAB());
    try {
      const res = await conn.withRetry(id => api.aiGenerateSql(
        id, conn.currentDatabase, text, { ...config.value }, !!keepSession
      ));
      const done = `已生成（第 ${res.turn || 1} 轮上下文，${res.rounds} 次模型调用）`;
      if (backgrounded()) {
        // 已切走：SQL 与状态写回原 Tab，不打扰当前 Tab
        ws.setSqlForTab(genTabId, res.sql);
        const st = bgState();
        st.status = done;
        st.error = '';
        st.steps = bgSteps;
      } else {
        lastUsage.value = res.usage || null;
        hasSession.value = !!res.hasSession;
        turn.value = res.turn || 0;
        ws.setSql(res.sql);
        status.value = done;
      }
      ui.showToast('SQL 已生成并写入编辑器', 'success');
      return true;
    } catch (err) {
      // 失败时保留上一轮可用上下文，用户可直接重试
      const msg = String((err && err.message) || err);
      if (backgrounded()) {
        const st = bgState();
        st.error = msg;
        st.status = '';
        st.steps = bgSteps;
      } else {
        error.value = msg;
        status.value = '';
      }
      ui.showToast('生成失败：' + msg, 'error');
      return false;
    } finally {
      inflight = false;
      genTabId = null;
      genRound = 0;
      bgSteps = [];
      generating.value = false;
      phase.value = '';
      phaseRound.value = 0;
    }
  }

  // 只清掉步骤明细，状态文案保留（步骤多时用来收拾界面）
  function clearSteps() {
    steps.value = [];
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
    init, isConfigured, saveConfig, testConfig, generate, clearSteps, reset,
    saveTabState, loadTabState, dropTabState
  };
});
