<script setup>
import { onMounted } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { useRedis, loadRedisInfo, loadRedisKeys } from '@/composables/useRedis';

const ui = useUiStore();
const connStore = useConnectionStore();
const { keys, pattern, selectedKey } = useRedis();

onMounted(async () => {
  try {
    await loadRedisInfo(connStore);
    await loadRedisKeys(connStore, pattern.value);
  } catch (err) {
    ui.showToast(String(err), 'error');
  }
});

async function reload() {
  try {
    await loadRedisKeys(connStore, pattern.value || '*');
  } catch (err) {
    ui.showToast(String(err), 'error');
  }
}
</script>

<template>
  <div class="db-tree" :class="{ collapsed: ui.panelState.tree }">
    <div class="tree-filter-row">
      <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" class="filter-input" v-model="pattern" placeholder="搜索key..." @input="reload">
      <button
        class="panel-toggle"
        @click="ui.togglePanel('tree')"
        :title="(ui.panelState.tree ? '展开' : '收起') + ui.panelTitle('tree') + '列'"
      >{{ ui.panelState.tree ? '›' : '‹' }}</button>
    </div>
    <div id="keyList">
      <div
        v-for="k in keys"
        :key="k"
        class="tree-item"
        :class="{ active: selectedKey === k }"
        @click="selectedKey = k"
      >{{ k }}</div>
    </div>
  </div>
</template>
