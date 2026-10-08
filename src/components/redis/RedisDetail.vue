<script setup>
import { ref, computed, watch } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { useRedis } from '@/composables/useRedis';
import { escapeHtml } from '@/utils/escape';
import * as api from '@/api';

const ui = useUiStore();
const connStore = useConnectionStore();
const { selectedKey } = useRedis();

const data = ref(null);

watch(selectedKey, async key => {
  if (!key) {
    data.value = null;
    return;
  }
  try {
    data.value = await connStore.withRetry(id => api.redisGet(id, key));
  } catch (err) {
    ui.showToast(String(err), 'error');
  }
}, { immediate: true });

// JSON 语法高亮：与原始单文件实现保持一致
function syntaxHighlightJson(json) {
  json = json.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  return json.replace(/("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g, function (match) {
    let cls = 'json-number';
    if (/^"/.test(match)) {
      cls = /:$/.test(match) ? 'json-key' : 'json-string';
    } else if (/true|false/.test(match)) {
      cls = 'json-boolean';
    } else if (/null/.test(match)) {
      cls = 'json-null';
    }
    return '<span class="' + cls + '">' + match + '</span>';
  });
}

const ttlText = computed(() => (data.value.ttl === -1 ? '永久' : data.value.ttl + 's'));

const displayHtml = computed(() => {
  const d = data.value;
  try {
    if (typeof d.value === 'string') {
      const parsed = JSON.parse(d.value);
      return syntaxHighlightJson(JSON.stringify(parsed, null, 2));
    }
    return '<pre>' + escapeHtml(JSON.stringify(d.value, null, 2)) + '</pre>';
  } catch {
    return escapeHtml(String(d.value));
  }
});
</script>

<template>
  <div class="workspace">
    <div v-if="!data" style="padding:20px;color:var(--text-dim);">请从左侧选择一个 Key</div>
    <div v-else style="padding:20px;">
      <h3 style="margin-bottom:15px;">{{ selectedKey }}</h3>
      <p style="margin-bottom:10px;">类型: <strong>{{ data.type }}</strong> | TTL: {{ ttlText }}</p>
      <div class="large-textarea" v-html="displayHtml"></div>
    </div>
  </div>
</template>
