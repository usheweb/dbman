<script setup>
// 工作区根组件：查询 Tab 条 + AI 生成栏 + SQL 编辑器 + 结果区
import { useConnectionStore } from '@/stores/connection';
import QueryTabs from '@/components/layout/QueryTabs.vue';
import SqlEditor from './SqlEditor.vue';
import ResultArea from './ResultArea.vue';
import AiGenerateBar from './AiGenerateBar.vue';

const conn = useConnectionStore();
</script>

<template>
  <div class="workspace">
    <QueryTabs />
    <AiGenerateBar v-if="conn.currentConnData && conn.currentConnData.type === 'mysql'" />
    <SqlEditor />
    <ResultArea />
  </div>
</template>

