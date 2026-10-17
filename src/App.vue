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

onBeforeUnmount(() => document.removeEventListener('contextmenu', onDocContextMenu));
</script>

<template>
  <div class="app-container" :class="{ 'result-fs': ui.resultFullscreen, 'sidebar-collapsed': ui.panelState.sidebar }">
    <AppHeader />
    <div class="main-content">
      <ConnectionSidebar />
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
