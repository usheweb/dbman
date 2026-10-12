<script setup>
// SQL 编辑器：CodeMirror 5 实例 + 编辑器工具栏
import { onMounted, onBeforeUnmount, ref, watch, nextTick } from 'vue';
import CodeMirror from 'codemirror';
import 'codemirror/mode/sql/sql.js';
import 'codemirror/addon/hint/show-hint.js';
import 'codemirror/addon/hint/sql-hint.js';
import { useWorkspaceStore } from '@/stores/workspace';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { useSqlStore } from '@/stores/sql';
import { useAiStore } from '@/stores/ai';
import { useThemeStore } from '@/stores/theme';
import * as api from '@/api';
import { formatSql } from '@/utils/sql';
import { showSqlHint } from '@/composables/useSqlHint';

const ws = useWorkspaceStore();
const ui = useUiStore();
const conn = useConnectionStore();
const sqlStore = useSqlStore();
const ai = useAiStore();
const theme = useThemeStore();

const ta = ref(null);
let cm = null;

onMounted(() => {
  cm = CodeMirror.fromTextArea(ta.value, {
    mode: 'text/x-mysql',
    // 跟随全局主题：亮色 eclipse / 暗色 monokai，主题切换时由下方 watch 同步
    theme: theme.editorTheme,
    lineNumbers: true,
    indentWithTabs: false,
    // 与 formatSql 的缩进保持一致（2 空格），否则回车续行与格式化结果缩进不同
    indentUnit: 2,
    tabSize: 2,
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
  // 编辑器是全局单实例：切连接会重建，这里把当前连接的 SQL 草稿灌回去
  ws.applyDraft();
});

// 主题切换时同步 CodeMirror 皮肤（实例只初始化一次，这里改 option 即可）
watch(() => theme.editorTheme, (name) => {
  if (cm) cm.setOption('theme', name);
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
  const sql = ws.getSql();
  if (!sql.trim()) return;
  try {
    ws.setSql(formatSql(sql));
  } catch (e) {
    ui.showToast('SQL 不完整，无法格式化', 'error');
  }
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

// 折叠编辑器 / 结果区全屏都会让 CodeMirror 容器消失再出现，恢复后刷新一下，避免内容空白或光标错位
watch([() => ui.viewOpts.sqlCollapsed, () => ui.resultFullscreen], () => {
  nextTick(() => { if (cm) cm.refresh(); });
});
</script>

<template>
  <div class="query-editor" :class="{ collapsed: ui.viewOpts.sqlCollapsed }">
    <div class="editor-header">
      <span>SQL编辑器</span>
      <div class="editor-toolbar">
        <button class="icon-btn" title="打开 SQL 文件" @click="openSqlFile">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
          </svg>
        </button>
        <button class="icon-btn" title="SQL 收藏列表" @click="ui.openModal('sqlFavModal')">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M7 4h10a1 1 0 0 1 1 1v15l-6-4-6 4V5a1 1 0 0 1 1-1z" />
          </svg>
        </button>
        <button class="icon-btn" title="SQL 日志列表" @click="ui.openModal('sqlLogModal')">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3.5 12a8.5 8.5 0 1 0 2.6-6.1" />
            <path d="M3 4v4h4" />
            <path d="M12 7.5V12l3 1.8" />
          </svg>
        </button>
        <button class="icon-btn" title="收藏当前 SQL" @click="openFavFromEditor">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 3.6l2.6 5.3 5.8.9-4.2 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8L3.6 9.8l5.8-.9z" />
          </svg>
        </button>
        <button class="icon-btn" title="格式化 SQL" @click="onFormat">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 6h16" />
            <path d="M4 12h9" />
            <path d="M4 18h14" />
          </svg>
        </button>
        <button class="btn" @click="ws.executeQuery()">执行 (Ctrl+Enter)</button>
        <button
          class="icon-btn"
          :title="ui.viewOpts.sqlCollapsed ? '展开 SQL 编辑器' : '收起 SQL 编辑器，把空间让给结果区'"
          @click="ui.toggleSqlCollapsed()"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path :d="ui.viewOpts.sqlCollapsed ? 'M6 9l6 6 6-6' : 'M6 15l6-6 6 6'" />
          </svg>
        </button>
      </div>
    </div>
    <textarea autocapitalize="off" autocorrect="off" spellcheck="false" ref="ta"></textarea>
  </div>
</template>
