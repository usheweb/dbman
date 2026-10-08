<script setup>
// 工作区根组件：AI 生成栏 + SQL 编辑器 + 结果区
import { useConnectionStore } from '@/stores/connection';
import SqlEditor from './SqlEditor.vue';
import ResultArea from './ResultArea.vue';
import AiGenerateBar from './AiGenerateBar.vue';

const conn = useConnectionStore();
</script>

<template>
  <div class="workspace">
    <AiGenerateBar v-if="conn.currentConnData && conn.currentConnData.type === 'mysql'" />
    <SqlEditor />
    <ResultArea />
  </div>
</template>
