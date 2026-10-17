<script setup>
// SQL 编辑器：CodeMirror 5 实例 + 编辑器工具栏
import { onMounted, onBeforeUnmount, ref, watch, nextTick } from 'vue';
import CodeMirror from 'codemirror';
import 'codemirror/mode/sql/sql.js';
import 'codemirror/addon/hint/show-hint.js';
import 'codemirror/addon/hint/sql-hint.js';
// 光标所在行加 class（wrap / background / gutter），「执行这一行」按钮据此显隐
import 'codemirror/addon/selection/active-line.js';
import { useWorkspaceStore } from '@/stores/workspace';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { useSqlStore } from '@/stores/sql';
import { useAiStore } from '@/stores/ai';
import { useThemeStore } from '@/stores/theme';
import * as api from '@/api';
import { formatSql } from '@/utils/sql';
import { showSqlHint } from '@/composables/useSqlHint';
import { setTip } from '@/composables/tip';

const ws = useWorkspaceStore();
const ui = useUiStore();
const conn = useConnectionStore();
const sqlStore = useSqlStore();
const ai = useAiStore();
const theme = useThemeStore();

const ta = ref(null);
let cm = null;

// 行内「执行这一行」用的自定义 gutter，排在行号左侧
const RUN_GUTTER = 'run-gutter';

function runMarker() {
  const el = document.createElement('span');
  el.className = 'run-gutter-btn';
  // gutter marker 是 JS 现造的 DOM，指令用不上，走 setTip 挂同款自定义气泡
  setTip(el, '执行这一行 SQL');
  el.innerHTML = '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>';
  return el;
}

// 给每一行补上播放图标；已有 marker 的行跳过，避免重复创建 DOM
function fillRunGutters() {
  if (!cm) return;
  cm.operation(() => {
    const total = cm.lineCount();
    for (let i = 0; i < total; i++) {
      const handle = cm.getLineHandle(i);
      if (!handle.gutterMarkers || !handle.gutterMarkers[RUN_GUTTER]) {
        cm.setGutterMarker(handle, RUN_GUTTER, runMarker());
      }
    }
  });
  syncGutterWidth();
}

// 「执行这一行」按钮要落在整条行号栏（执行列 + 行号列）的水平正中，
// 而栏宽随行号位数变化，纯 CSS 量不到，这里把实测总宽写进 CSS 变量交给 theme.css 定位
function syncGutterWidth() {
  if (!cm) return;
  const gutters = cm.getWrapperElement().querySelector('.CodeMirror-gutters');
  if (!gutters) return;
  cm.getWrapperElement().style.setProperty('--cm-gutter-w', gutters.offsetWidth + 'px');
}

// 光标停在空行（没有可执行的 SQL）时不显示「执行这一行」按钮
function syncCursorLineState() {
  if (!cm) return;
  const empty = !(cm.getLine(cm.getCursor().line) || '').trim();
  cm.getWrapperElement().classList.toggle('cursor-line-empty', empty);
}

// 点击是否落在「执行这一行」按钮的圆环范围内。
// 按钮被 CSS 居中铺在整条行号栏上，横向会越过 run-gutter 列，
// 此时 CodeMirror 会按 x 坐标把点击归给行号列，光看 gutter 名会漏判
function clickOnRunBtn(instance, e) {
  if (e && e.target instanceof Element && e.target.closest('.run-gutter-btn')) return true;
  const gutters = instance.getWrapperElement().querySelector('.CodeMirror-gutters');
  if (!gutters || !e) return false;
  const box = gutters.getBoundingClientRect();
  // 按钮直径 17px，居中在整条行号栏，两侧各留 3px 容差
  return Math.abs((e.clientX - box.left) - box.width / 2) <= 11.5;
}

onMounted(() => {
  cm = CodeMirror.fromTextArea(ta.value, {
    mode: 'text/x-mysql',
    // 跟随全局主题：亮色 eclipse / 暗色 monokai，主题切换时由下方 watch 同步
    theme: theme.editorTheme,
    lineNumbers: true,
    gutters: ['run-gutter', 'CodeMirror-linenumbers'],
    // 光标所在行标记：nonEmpty 为 false 让「选中单行」时也保持标记，多行选择则不标记
    styleActiveLine: { nonEmpty: false },
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
  // 行内「执行这一行」：点击行号左侧的播放图标只跑该行 SQL，不覆盖编辑器内容
  cm.on('gutterClick', (instance, line, gutter, e) => {
    if (gutter !== RUN_GUTTER && !clickOnRunBtn(instance, e)) return;
    const text = (instance.getLine(line) || '').trim();
    if (!text) {
      ui.showToast('这一行没有可执行的 SQL', 'error');
      return;
    }
    ws.executeSqlText(text);
  });
  // 光标移动 / 行内容变化时同步「光标行是否为空」，决定要不要显示执行按钮
  cm.on('cursorActivity', syncCursorLineState);
  // 增删行后给新行补上播放图标（顺带同步行号栏宽度，行号位数变化会改变栏宽）
  // 顺带让结果表头的排序箭头跟着 SQL 里的 ORDER BY 走：手改 / 删掉排序子句时箭头同步变化
  cm.on('changes', () => { fillRunGutters(); syncCursorLineState(); ws.syncResultSort(); });
  // 刷新（窗口尺寸/折叠状态变化等）后重新量一次行号栏宽度
  cm.on('refresh', syncGutterWidth);
  // 编辑器是全局单实例：切连接会重建，这里把当前连接的 SQL 草稿灌回去
  ws.applyDraft();
  fillRunGutters();
  syncCursorLineState();
  // CodeMirror 5 对 gutter marker 是懒渲染，初次挂载后强制刷新一次，否则播放图标不出 DOM
  cm.refresh();
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
    if (ws.getSql().trim()) {
      const ok = await ui.askConfirm({
        title: '载入 SQL 文件',
        message: '编辑器里已有 SQL，载入文件会覆盖当前内容，确定继续吗？',
        confirmText: '覆盖载入',
        danger: true
      });
      if (!ok) return;
    }
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
        <button class="icon-btn" v-tip="'打开 SQL 文件'" @click="openSqlFile">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
          </svg>
          <span class="btn-label">打开文件</span>
        </button>
        <button class="icon-btn" v-tip="'SQL 收藏列表'" @click="ui.openModal('sqlFavModal')">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M7 4h10a1 1 0 0 1 1 1v15l-6-4-6 4V5a1 1 0 0 1 1-1z" />
          </svg>
          <span class="btn-label">收藏列表</span>
        </button>
        <button class="icon-btn" v-tip="'SQL 日志列表'" @click="ui.openModal('sqlLogModal')">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3.5 12a8.5 8.5 0 1 0 2.6-6.1" />
            <path d="M3 4v4h4" />
            <path d="M12 7.5V12l3 1.8" />
          </svg>
          <span class="btn-label">SQL 日志</span>
        </button>
        <button class="icon-btn" v-tip="'收藏当前 SQL'" @click="openFavFromEditor">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 3.6l2.6 5.3 5.8.9-4.2 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8L3.6 9.8l5.8-.9z" />
          </svg>
          <span class="btn-label">收藏</span>
        </button>
        <button class="icon-btn" v-tip="'格式化 SQL'" @click="onFormat">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 6h16" />
            <path d="M4 12h9" />
            <path d="M4 18h14" />
          </svg>
          <span class="btn-label">格式化</span>
        </button>
        <button class="icon-btn exec-btn" v-tip="'执行 SQL（Ctrl+Enter）'" @click="ws.executeQuery()">
          <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M8 5v14l11-7z" />
          </svg>
          <span class="btn-label">执行</span>
        </button>
        <button
          class="icon-btn"
          v-tip="ui.viewOpts.sqlCollapsed ? '展开 SQL 编辑器' : '收起 SQL 编辑器，把空间让给结果区'"
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
