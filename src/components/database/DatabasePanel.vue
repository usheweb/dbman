<script setup>
import { computed } from 'vue';
import { useConnectionStore } from '@/stores/connection';
import TableTree from '@/components/database/TableTree.vue';
import RedisTree from '@/components/database/RedisTree.vue';
import RedisDetail from '@/components/redis/RedisDetail.vue';
import SqlWorkspace from '@/components/workspace/SqlWorkspace.vue';

const connStore = useConnectionStore();

// 建连期间（尤其是被防火墙/安全组丢包时）给出明确反馈，避免界面看起来卡死
const connectingTarget = computed(() => {
  const c = connStore.currentConnData;
  if (!c) return '';
  return c.port ? c.host + ':' + c.port : c.host;
});
</script>

<template>
  <div class="database-panel">
    <div v-if="!connStore.currentConnData" class="no-connection">请先创建并选择一个数据库连接</div>
    <div v-else-if="!connStore.isConnected" class="no-connection">正在连接 {{ connectingTarget }} …</div>
    <template v-else-if="connStore.currentConnData.type === 'mysql'">
      <TableTree />
      <SqlWorkspace />
    </template>
    <template v-else>
      <RedisTree />
      <RedisDetail />
    </template>
  </div>
</template>
