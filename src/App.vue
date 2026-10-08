<script setup>
import { onMounted } from 'vue';
import { useStoreStore } from '@/stores/store';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { useWorkspaceStore } from '@/stores/workspace';
import { useSqlStore } from '@/stores/sql';
import { useAiStore } from '@/stores/ai';

import AppHeader from '@/components/layout/AppHeader.vue';
import ConnectionSidebar from '@/components/layout/ConnectionSidebar.vue';
import DatabasePanel from '@/components/database/DatabasePanel.vue';
import ConnectionModal from '@/components/connection/ConnectionModal.vue';
import ContentModal from '@/components/modals/ContentModal.vue';
import ExecConfirmModal from '@/components/modals/ExecConfirmModal.vue';
import ExportModal from '@/components/modals/ExportModal.vue';
import FilterModal from '@/components/modals/FilterModal.vue';
import SqlLogModal from '@/components/modals/SqlLogModal.vue';
import SqlFavModal from '@/components/modals/SqlFavModal.vue';
import FavEditModal from '@/components/modals/FavEditModal.vue';
import AiConfigModal from '@/components/modals/AiConfigModal.vue';
import PromptModal from '@/components/modals/PromptModal.vue';
import TableDdlModal from '@/components/modals/TableDdlModal.vue';
import ExportSqlModal from '@/components/modals/ExportSqlModal.vue';
import TableDesignerModal from '@/components/modals/TableDesignerModal.vue';
import CsvImportModal from '@/components/modals/CsvImportModal.vue';
import ToastHost from '@/components/common/ToastHost.vue';

const store = useStoreStore();
const ui = useUiStore();
const conn = useConnectionStore();
const ws = useWorkspaceStore();
const sql = useSqlStore();
const ai = useAiStore();

// 启动引导：先把本地文件里的数据读进内存，后面的渲染/绑定才有正确的初始状态
onMounted(async () => {
  await store.load();
  ui.init();
  conn.init();
  ws.init();
  sql.init();
  ai.init();
  // 启动时没有任何连接中的数据库：展开「连接」面板，方便先选一个连接
  if (!conn.isConnected) ui.setPanel('sidebar', false);
  // 工作区执行完语句后要写 SQL 日志，这里做一次注入，避免 store 之间循环依赖
  ws.bindSqlStore(sql);
});
</script>

<template>
  <div class="app-container">
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
  <ExecConfirmModal />
  <ExportModal />
  <FilterModal />
  <SqlLogModal />
  <SqlFavModal />
  <FavEditModal />
  <AiConfigModal />
  <PromptModal />
  <TableDdlModal />
  <ExportSqlModal />
  <TableDesignerModal />
  <CsvImportModal />
  <ToastHost />
</template>
