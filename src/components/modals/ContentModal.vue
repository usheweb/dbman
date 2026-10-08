<script setup>
import { ref, computed } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useUiStore } from '@/stores/ui';
import { isTimestamp, isPossibleDate, formatDateTime } from '@/utils/format';
import { escapeHtml } from '@/utils/escape';

const ui = useUiStore();
const showTimestamp = ref(false);

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

const html = computed(() => {
  const content = ui.contentValue;
  if (!showTimestamp.value && isTimestamp(content)) {
    const date = new Date(parseInt(content) * (content.length === 10 ? 1000 : 1));
    return `<div>${escapeHtml(formatDateTime(date))}</div><br><div style="color:var(--text-dim);">原始值: ${escapeHtml(content)}</div>`;
  }
  // 只对对象/数组做 JSON 美化：像 "0.00"、"1e5"、"true" 同样是合法 JSON，
  // 但 JSON.parse 后再 stringify 会把 0.00 变成 0，所以必须原样展示
  if (/^[\[{]/.test(String(content == null ? '' : content).trim())) {
    try {
      const parsed = JSON.parse(content);
      return syntaxHighlightJson(JSON.stringify(parsed, null, 2));
    } catch (e) {
      // 不是合法 JSON，继续按普通文本 / 日期处理
    }
  }
  if (!showTimestamp.value && isPossibleDate(content)) {
    const date = new Date(content);
    if (!isNaN(date.getTime())) {
      return `<div>${escapeHtml(content)}</div><br><div style="color:var(--text-dim);">时间戳: ${Math.floor(date.getTime() / 1000)}</div>`;
    }
  }
  return escapeHtml(content);
});

function copyContent() {
  navigator.clipboard.writeText(ui.contentValue);
  ui.showToast('已复制到剪贴板', 'success');
}
</script>

<template>
  <ModalShell :visible="ui.modals.contentModal" width="800px" @close="ui.closeModal('contentModal')">
    <template #title>内容详情</template>
    <div style="margin-bottom: 10px;">
      <button class="btn btn-secondary" @click="copyContent">复制内容</button>
      <label style="margin-left: 10px;">
        <input type="checkbox" v-model="showTimestamp" style="margin-right: 5px;">
        显示为时间戳
      </label>
    </div>
    <div id="modalContent" class="large-textarea" v-html="html"></div>
  </ModalShell>
</template>
