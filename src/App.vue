<script setup>
import { onMounted, onBeforeUnmount, ref, computed } from 'vue';
import { useStoreStore } from '@/stores/store';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { useWorkspaceStore } from '@/stores/workspace';
import { useSqlStore } from '@/stores/sql';
import { useAiStore } from '@/stores/ai';
import { useThemeStore } from '@/stores/theme';
import { todayKey } from '@/utils/format';

import AppHeader from '@/components/layout/AppHeader.vue';
import ConnectionSidebar from '@/components/layout/ConnectionSidebar.vue';
import DatabasePanel from '@/components/database/DatabasePanel.vue';
import ConnectionModal from '@/components/connection/ConnectionModal.vue';
import ContentModal from '@/components/modals/ContentModal.vue';
import ConfirmModal from '@/components/modals/ConfirmModal.vue';
import ExecConfirmModal from '@/components/modals/ExecConfirmModal.vue';
import ExportModal from '@/components/modals/ExportModal.vue';
import FilterModal from '@/components/modals/FilterModal.vue';
import SqlLogModal from '@/components/modals/SqlLogModal.vue';
import ResultHistoryModal from '@/components/modals/ResultHistoryModal.vue';
import SqlFavModal from '@/components/modals/SqlFavModal.vue';
import FavEditModal from '@/components/modals/FavEditModal.vue';
import AiConfigModal from '@/components/modals/AiConfigModal.vue';
import ViewSettingsModal from '@/components/modals/ViewSettingsModal.vue';
import AboutModal from '@/components/modals/AboutModal.vue';
import PromptModal from '@/components/modals/PromptModal.vue';
import TableDdlModal from '@/components/modals/TableDdlModal.vue';
import ExportSqlModal from '@/components/modals/ExportSqlModal.vue';
import TableDesignerModal from '@/components/modals/TableDesignerModal.vue';
import CsvImportModal from '@/components/modals/CsvImportModal.vue';
import ResultConfirmModal from '@/components/modals/ResultConfirmModal.vue';
import IdentifierSuggestModal from '@/components/modals/IdentifierSuggestModal.vue';
import CellEditModal from '@/components/modals/CellEditModal.vue';
import ResultSqlPanel from '@/components/modals/ResultSqlPanel.vue';
import PromoModal from '@/components/modals/PromoModal.vue';
import ToastHost from '@/components/common/ToastHost.vue';
import ContextMenu from '@/components/common/ContextMenu.vue';

const store = useStoreStore();
const ui = useUiStore();
const conn = useConnectionStore();
const ws = useWorkspaceStore();
const sql = useSqlStore();
const ai = useAiStore();
const theme = useThemeStore();

// 全应用右键菜单：顶掉 WebView 自带的英文 Reload / Inspect Element
const ctxVisible = ref(false);
const ctxX = ref(0);
const ctxY = ref(0);
const ctxItems = computed(() => [
  { label: '刷新', value: 'reload' },
  {
    label: '设置',
    children: [
      { label: 'AI 设置', value: 'ai' },
      { label: '视图设置', value: 'view' }
    ]
  },
  { label: theme.theme === 'dark' ? '切换为亮色主题' : '切换为暗色主题', value: 'theme' }
]);

function onDocContextMenu(e) {
  // 表列表、结果集、连接列表等已有自己的右键菜单，不再接管
  if (e.defaultPrevented) return;
  // 输入框与 SQL 编辑器保留系统原生的复制/粘贴菜单
  const el = e.target;
  if (el instanceof Element && el.closest('input, textarea, [contenteditable="true"], .cm-editor')) return;
  e.preventDefault();
  ctxX.value = e.clientX;
  ctxY.value = e.clientY;
  ctxVisible.value = true;
}

function onCtxSelect(item) {
  ctxVisible.value = false;
  if (!item) return;
  switch (item.value) {
    case 'reload': window.location.reload(); break;
    case 'ai': ui.openModal('aiConfigModal'); break;
    case 'view': ui.openModal('viewSettingsModal'); break;
    case 'theme': theme.toggle(); break;
  }
}

// ===== 连接/数据库侧栏宽度：拖它与右侧内容区的边界调整，宽度持久化在 ui.viewOpts.sidebarWidth =====
const SIDEBAR_W_DEFAULT = 290; // 双击分隔条恢复到这个宽度
const SIDEBAR_W_MIN = 200;
const sidebarRef = ref(null);
const sidebarResizing = ref(false);
// 收起态下交给 .app-container.sidebar-collapsed 的 46px，不要用内联变量盖掉它
const appStyle = computed(() => (ui.panelState.sidebar
  ? {}
  : { '--sidebar-w': (ui.viewOpts.sidebarWidth || SIDEBAR_W_DEFAULT) + 'px' }));

let sidebarFromX = 0;
let sidebarFromW = 0;
function startSidebarResize(e) {
  if (ui.panelState.sidebar) return; // 收起态只有 46px，没有可拖的余地
  const el = sidebarRef.value && sidebarRef.value.$el;
  sidebarFromX = e.clientX;
  sidebarFromW = el ? el.getBoundingClientRect().width : (ui.viewOpts.sidebarWidth || SIDEBAR_W_DEFAULT);
  sidebarResizing.value = true;
  document.addEventListener('mousemove', onSidebarResizeMove);
  document.addEventListener('mouseup', stopSidebarResize);
  e.preventDefault();
}
function onSidebarResizeMove(e) {
  // 上限取窗口宽度的 40%，再宽就把库表/查询区挤没了
  const max = Math.max(SIDEBAR_W_MIN + 60, Math.min(window.innerWidth * 0.4, 560));
  const w = Math.min(max, Math.max(SIDEBAR_W_MIN, sidebarFromW + (e.clientX - sidebarFromX)));
  ui.saveViewOpts({ sidebarWidth: Math.round(w) });
}
function stopSidebarResize() {
  sidebarResizing.value = false;
  document.removeEventListener('mousemove', onSidebarResizeMove);
  document.removeEventListener('mouseup', stopSidebarResize);
}
function resetSidebarWidth() {
  ui.saveViewOpts({ sidebarWidth: SIDEBAR_W_DEFAULT });
}

// 启动引导：先把本地文件里的数据读进内存，后面的渲染/绑定才有正确的初始状态
onMounted(async () => {
  await store.load();
  // 「唐三浪学习时钟」推广弹窗：勾选过「7 天内不再提示」且未到期就跳过，
  // 到期后再弹一次（勾选偏好仍在，关闭后又顺延一周）—— 等效每周一次
  const promoSkipEnabled = store.get('dbman_promo_skip_enabled') === '1';
  const promoUntil = store.get('dbman_promo_skip_until') || '';
  if (!(promoSkipEnabled && todayKey() <= promoUntil)) ui.openModal('promoModal');
  ui.init();
  conn.init();
  ws.init();
  sql.init();
  ai.init();
  // 启动时没有任何连接中的数据库：展开「连接」面板，方便先选一个连接
  if (!conn.isConnected) ui.setPanel('sidebar', false);
  // 工作区执行完语句后要写 SQL 日志，这里做一次注入，避免 store 之间循环依赖
  ws.bindSqlStore(sql);
  // 查询 Tab 换入换出时要同步换 AI 栏状态，同样由这里注入（workspace → ai 会循环依赖）
  ws.bindAiStore(ai);
  document.addEventListener('contextmenu', onDocContextMenu);
});

onBeforeUnmount(() => {
  document.removeEventListener('contextmenu', onDocContextMenu);
  stopSidebarResize();
});
</script>

<template>
  <div
    class="app-container"
    :class="{ 'result-fs': ui.resultFullscreen, 'sidebar-collapsed': ui.panelState.sidebar }"
    :style="appStyle"
  >
    <AppHeader />
    <div class="main-content">
      <ConnectionSidebar ref="sidebarRef" />
      <div v-if="!ui.panelState.sidebar" class="sidebar-splitter">
        <div
          class="splitter-v"
          :class="{ resizing: sidebarResizing }"
          v-tip="'拖动调整连接/数据库列表宽度；双击恢复默认'"
          @mousedown="startSidebarResize"
          @dblclick="resetSidebarWidth"
        ></div>
      </div>
      <div class="content-area">
        <DatabasePanel />
      </div>
    </div>
  </div>

  <ConnectionModal />
  <ContentModal />
  <ConfirmModal />
  <ExecConfirmModal />
  <ExportModal />
  <FilterModal />
  <SqlLogModal />
  <ResultHistoryModal />
  <SqlFavModal />
  <FavEditModal />
  <AiConfigModal />
  <ViewSettingsModal />
  <AboutModal />
  <PromptModal />
  <TableDdlModal />
  <ExportSqlModal />
  <TableDesignerModal />
  <CsvImportModal />
  <ResultConfirmModal />
  <IdentifierSuggestModal />
  <CellEditModal />
  <ResultSqlPanel />
  <PromoModal :visible="ui.modals.promoModal" @close="ui.closeModal('promoModal')" />
  <ContextMenu
    v-if="ctxVisible"
    :x="ctxX"
    :y="ctxY"
    :items="ctxItems"
    @select="onCtxSelect"
    @close="ctxVisible = false"
  />
  <ToastHost />
</template>
