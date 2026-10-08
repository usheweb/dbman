<script setup>
import { useConnectionStore } from '@/stores/connection';
import TableTree from '@/components/database/TableTree.vue';
import RedisTree from '@/components/database/RedisTree.vue';
import RedisDetail from '@/components/redis/RedisDetail.vue';
import SqlWorkspace from '@/components/workspace/SqlWorkspace.vue';

const connStore = useConnectionStore();
</script>

<template>
  <div class="database-panel">
    <div v-if="!connStore.currentConnData" class="no-connection">请先创建并选择一个数据库连接</div>
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
