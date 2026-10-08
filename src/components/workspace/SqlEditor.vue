<script setup>
// SQL 编辑器：CodeMirror 5 实例 + 编辑器工具栏
import { onMounted, onBeforeUnmount, ref } from 'vue';
import CodeMirror from 'codemirror';
import 'codemirror/mode/sql/sql.js';
import 'codemirror/addon/hint/show-hint.js';
import 'codemirror/addon/hint/sql-hint.js';
import { useWorkspaceStore } from '@/stores/workspace';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { useSqlStore } from '@/stores/sql';
import { useAiStore } from '@/stores/ai';
import * as api from '@/api';
import { formatSql } from '@/utils/sql';
import { showSqlHint } from '@/composables/useSqlHint';

const ws = useWorkspaceStore();
const ui = useUiStore();
const conn = useConnectionStore();
const sqlStore = useSqlStore();
const ai = useAiStore();

const ta = ref(null);
let cm = null;

onMounted(() => {
  cm = CodeMirror.fromTextArea(ta.value, {
    mode: 'text/x-mysql',
    theme: 'monokai',
    lineNumbers: true,
    indentWithTabs: false,
    indentUnit: 4,
    tabSize: 4,
    lineWrapping: true,
    extraKeys: {
      'Ctrl-Space': () => showSqlHint(cm, conn),
      // 这里不能直接传 executeQuery：CodeMirror 会把 cm 实例当第一个参数传入，会被误当作「已确认」
      'Ctrl-Enter': () => ws.executeQuery()
    }
  });
  ws.setEditor(cm);
  // inputRead 只在「插入」文本时触发，退格/Delete 不会触发，
  // 所以这里统一监听 change：插入看 text，删除看 removed，两种都要刷新下拉
  cm.on('change', (instance, change) => {
    const typed = change.origin === '+delete'
      ? change.removed[0]
      : (change.origin === '+input' ? change.text[0] : '');
    if (!typed || typed.length !== 1 || !/[\w.]/.test(typed)) return;
    showSqlHint(instance, conn);
  });
  cm.setValue('');
  ws.resetPageState();
});

onBeforeUnmount(() => {
  if (!cm) return;
  cm.getWrapperElement()?.remove();
  cm = null;
});

function openFavFromEditor() {
  const sql = ws.getSql();
  if (!sql.trim()) {
    ui.showToast('编辑器里还没有 SQL', 'error');
    return;
  }
  sqlStore.openFavDraft({
    sql,
    db: conn.currentDatabase || '',
    conn: conn.currentConnData ? conn.currentConnData.name : '',
    // AI 描述框里的文字带进收藏面板当备注，方便日后辨认这条 SQL
    note: ai.prompt.trim()
  });
  ui.openModal('favEditModal');
}

function onFormat() {
  ws.setSql(formatSql(ws.getSql()));
}

// 选择外部 SQL 文件，把内容载入编辑器（不自动执行，用户确认后再跑）
async function openSqlFile() {
  const picked = await api.openDialog({
    multiple: false,
    filters: [{ name: 'SQL', extensions: ['sql', 'txt'] }]
  });
  const file = Array.isArray(picked) ? picked[0] : picked;
  if (!file) return;
  try {
    const text = await api.readTextFile(file);
    if (!text.trim()) {
      ui.showToast('文件内容为空', 'error');
      return;
    }
    if (ws.getSql().trim() && !confirm('编辑器里已有 SQL，载入文件会覆盖当前内容，确定继续吗？')) return;
    ws.setSql(text);
    ui.showToast('已载入 SQL 文件', 'success');
  } catch (e) {
    ui.showToast(String(e.message || e), 'error');
  }
}
</script>

<template>
  <div class="query-editor">
    <div class="editor-header">
      <span>SQL编辑器</span>
      <div class="editor-toolbar">
        <button class="btn btn-secondary" @click="openSqlFile">打开 SQL 文件</button>
        <button class="btn btn-secondary" @click="ui.openModal('sqlFavModal')">SQL 收藏列表</button>
        <button class="btn btn-secondary" @click="ui.openModal('sqlLogModal')">SQL 日志列表</button>
        <button class="btn btn-secondary" @click="openFavFromEditor">☆ 收藏</button>
        <button class="btn btn-secondary" @click="onFormat">格式化</button>
        <button class="btn" @click="ws.executeQuery()">执行 (Ctrl+Enter)</button>
      </div>
    </div>
    <textarea autocapitalize="off" autocorrect="off" spellcheck="false" ref="ta"></textarea>
  </div>
</template>
