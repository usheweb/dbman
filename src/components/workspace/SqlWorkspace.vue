<script setup>
// 工作区根组件：查询 Tab 条 + AI 生成栏 + SQL 编辑器 + 结果区
// 编辑器与结果区之间的横条可上下拖动，用来调整结果区高度（编辑器高度持久化在 ui.viewOpts.editorHeight）
import { computed, onBeforeUnmount, ref } from 'vue';
import { useConnectionStore } from '@/stores/connection';
import { useUiStore } from '@/stores/ui';
import { isMysqlLike } from '@/utils/dbType';
import QueryTabs from '@/components/layout/QueryTabs.vue';
import SqlEditor from './SqlEditor.vue';
import ResultArea from './ResultArea.vue';
import AiGenerateBar from './AiGenerateBar.vue';

const conn = useConnectionStore();
const ui = useUiStore();

const rootEl = ref(null);
const editorRef = ref(null);
const rowResizing = ref(false);
// editorHeight 为 0 时不写变量，交给 theme.css 里的默认百分比（40%）
const rootStyle = computed(() => (ui.viewOpts.editorHeight
  ? { '--editor-h': ui.viewOpts.editorHeight + 'px' }
  : {}));

let rowFromY = 0;
let rowFromH = 0;
function startRowResize(e) {
  const el = editorRef.value?.$el;
  if (!el) return;
  rowFromY = e.clientY;
  rowFromH = el.getBoundingClientRect().height;
  rowResizing.value = true;
  document.addEventListener('mousemove', onRowResizeMove);
  document.addEventListener('mouseup', stopRowResize);
  e.preventDefault();
}
function onRowResizeMove(e) {
  const total = rootEl.value ? rootEl.value.clientHeight : window.innerHeight;
  // 上下都留余地：编辑器最小 150px（与 theme.css 的 min-height 一致），结果区至少留 160px
  const max = Math.max(150, total - 160);
  const h = Math.min(max, Math.max(150, rowFromH + (e.clientY - rowFromY)));
  ui.saveViewOpts({ editorHeight: Math.round(h) });
}
function stopRowResize() {
  rowResizing.value = false;
  document.removeEventListener('mousemove', onRowResizeMove);
  document.removeEventListener('mouseup', stopRowResize);
}
function resetRowHeight() {
  ui.saveViewOpts({ editorHeight: 0 });
}
onBeforeUnmount(stopRowResize);
</script>

<template>
  <div class="workspace" ref="rootEl" :style="rootStyle">
    <QueryTabs />
    <AiGenerateBar v-if="conn.currentConnData && isMysqlLike(conn.currentConnData.type)" />
    <SqlEditor ref="editorRef" />
    <div
      v-if="!ui.viewOpts.sqlCollapsed && !ui.resultFullscreen"
      class="splitter-h"
      :class="{ resizing: rowResizing }"
      v-tip="'拖动调整结果区高度；双击恢复默认'"
      @mousedown="startRowResize"
      @dblclick="resetRowHeight"
    ></div>
    <ResultArea />
  </div>
</template>
