<script setup>
// AI 模型设置：接口地址 / API Key / 模型名称（第一期由用户自行配置）
import { reactive, ref, watch } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useAiStore } from '@/stores/ai';
import { useUiStore } from '@/stores/ui';

const ai = useAiStore();
const ui = useUiStore();

const form = reactive({ baseUrl: '', apiKey: '', model: '', timeoutSecs: 60, maxRounds: 6 });
const testing = ref(false);

watch(
  () => ui.modals.aiConfigModal,
  open => {
    if (open) Object.assign(form, ai.config);
  }
);

function collect() {
  return {
    baseUrl: form.baseUrl.trim(),
    apiKey: form.apiKey.trim(),
    model: form.model.trim(),
    timeoutSecs: Number(form.timeoutSecs) || 60,
    maxRounds: Number(form.maxRounds) || 6
  };
}

async function onTest() {
  testing.value = true;
  try {
    await ai.testConfig(collect());
    ui.showToast('连接成功，配置可用', 'success');
  } catch (err) {
    ui.showToast('测试失败：' + String((err && err.message) || err), 'error');
  } finally {
    testing.value = false;
  }
}

function onSave() {
  const data = collect();
  if (!data.baseUrl || !data.model) {
    ui.showToast('接口地址与模型名称为必填项', 'error');
    return;
  }
  ai.saveConfig(data);
  ui.closeModal('aiConfigModal');
  ui.showToast('AI 配置已保存', 'success');
}
</script>

<template>
  <ModalShell :visible="ui.modals.aiConfigModal" width="520px" @close="ui.closeModal('aiConfigModal')">
    <template #title>AI 模型设置</template>
    <form @submit.prevent="onSave">
      <div class="form-group">
        <label>接口地址</label>
        <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.baseUrl" placeholder="https://api.deepseek.com">
        <div class="form-hint">支持填根地址、/v1，或完整 /v1/chat/completions</div>
      </div>
      <div class="form-group">
        <label>API Key</label>
        <input type="password" v-model="form.apiKey" placeholder="sk-...">
      </div>
      <div class="form-group">
        <label>模型名称</label>
        <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.model" placeholder="deepseek-chat">
      </div>
      <div class="form-row">
        <div class="form-group">
          <label>超时（秒）</label>
          <input type="number" v-model.number="form.timeoutSecs" min="5">
        </div>
        <div class="form-group">
          <label>最大工具轮次</label>
          <input type="number" v-model.number="form.maxRounds" min="2">
        </div>
      </div>
      <div style="display: flex; gap: 10px; justify-content: flex-end;">
        <button type="button" class="btn btn-secondary" @click="ui.closeModal('aiConfigModal')">取消</button>
        <button type="button" class="btn btn-secondary" :disabled="testing" @click="onTest">
          {{ testing ? '测试中…' : '测试连接' }}
        </button>
        <button type="submit" class="btn">保存</button>
      </div>
    </form>
  </ModalShell>
</template>

<style scoped>
.form-hint {
  margin-top: 4px;
  font-size: 12px;
  color: var(--text-dim);
}
</style>
