<script setup>
import { computed } from 'vue';
import { useConnectionStore } from '@/stores/connection';
import { isRelational, isFileBased } from '@/utils/dbType';
import TableTree from '@/components/database/TableTree.vue';
import RedisTree from '@/components/database/RedisTree.vue';
import RedisDetail from '@/components/redis/RedisDetail.vue';
import SqlWorkspace from '@/components/workspace/SqlWorkspace.vue';

const connStore = useConnectionStore();

// 建连期间（尤其是被防火墙/安全组丢包时）给出明确反馈，避免界面看起来卡死
const connectingTarget = computed(() => {
  const c = connStore.currentConnData;
  if (!c) return '';
  // 本地文件型（如 SQLite）没有主机端口，展示文件路径
  if (isFileBased(c.type)) return c.database || '';
  return c.port ? c.host + ':' + c.port : c.host;
});
</script>

<template>
  <div class="database-panel">
    <div v-if="!connStore.currentConnData" class="no-connection">请先创建并选择一个数据库连接</div>
    <div v-else-if="!connStore.isConnected" class="no-connection">正在连接 {{ connectingTarget }} …</div>
    <template v-else-if="isRelational(connStore.currentConnData.type)">
      <TableTree />
      <SqlWorkspace />
    </template>
    <template v-else>
      <RedisTree />
      <RedisDetail />
    </template>
  </div>
</template>
