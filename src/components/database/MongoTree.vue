<script setup>
// MongoDB 左侧面板：库下拉 + 集合列表。
// 库的列表来自连接卡片（connection store 的 databaseList），
// 集合列表由本组件按当前库自行拉取（RedisTree 同款做法）。
import { ref, computed, watch } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { useMongo, loadMongoCollections, resetMongo } from '@/composables/useMongo';
import ItemIcon from '@/components/common/ItemIcon.vue';
import IconSelect from '@/components/common/IconSelect.vue';

const ui = useUiStore();
const connStore = useConnectionStore();
const { collections, selectedCollection } = useMongo();

const filterText = ref('');

const dbOptions = computed(() => connStore.currentDatabaseList.map(db => ({ value: db, label: db })));

const filtered = computed(() => {
  const k = filterText.value.trim().toLowerCase();
  if (!k) return collections.value;
  return collections.value.filter(c => (c.name || '').toLowerCase().includes(k));
});

const countText = computed(() => {
  const total = collections.value.length;
  if (!filterText.value.trim()) return '共 ' + total + ' 个集合';
  return filtered.value.length + ' / ' + total + ' 个集合';
});

// 连接是异步建立的：currentConnData 先就绪、后端句柄 serverConnId 后赋值，
// 本组件挂载时句柄往往还是 null。同时监听活跃连接 / 连接就绪 / 当前库：
// 切 tab、重连、换库都会重新拉一次。用 cid#db 去重避免建连时的重复请求
let loadedKey = '';
watch(
  [() => connStore.currentConnId, () => connStore.isConnected, () => connStore.currentDatabase],
  async ([cid, ok, db], [prevCid]) => {
    if (cid !== prevCid) { resetMongo(); loadedKey = ''; }
    if (!ok || !db) return;
    const k = cid + '#' + db;
    if (k === loadedKey) return;
    loadedKey = k;
    // 换库后旧集合名在新库里未必存在，先清空选中项，避免右侧工作区挂着上一个库的结果
    selectedCollection.value = null;
    try {
      await loadMongoCollections(connStore);
    } catch (err) {
      ui.showToast(String(err), 'error');
    }
  },
  { immediate: true, flush: 'post' }
);
</script>

<template>
  <div class="db-tree" :class="{ collapsed: ui.panelState.tree }">
    <div class="tree-header-row">
      <IconSelect
        class="db-picker"
        title="切换数据库"
        :model-value="connStore.currentDatabase"
        :options="dbOptions"
        searchable
        @update:model-value="connStore.selectDatabase"
      />
    </div>
    <div class="tree-filter-row">
      <input
        type="text"
        autocapitalize="off"
        autocorrect="off"
        spellcheck="false"
        class="filter-input"
        v-model="filterText"
        placeholder="过滤集合名..."
      >
      <button
        class="panel-toggle"
        @click="ui.togglePanel('tree')"
        :title="(ui.panelState.tree ? '展开' : '收起') + ui.panelTitle('tree') + '列'"
      >{{ ui.panelState.tree ? '›' : '‹' }}</button>
    </div>
    <div v-if="connStore.currentDatabase" class="tree-count-row" :title="countText">{{ countText }}</div>
    <div id="collectionList">
      <div
        v-for="c in filtered"
        :key="c.name"
        class="tree-item tree-table"
        :class="{ active: selectedCollection === c.name }"
        @click="selectedCollection = c.name"
      >
        <ItemIcon name="collection" />
        <span class="tree-table-body">
          <span class="tree-table-name">{{ c.name }}</span>
        </span>
      </div>
      <div v-if="!filtered.length" class="mongo-tree-empty">
        {{ collections.length ? '没有匹配的集合' : '当前库没有集合' }}
      </div>
    </div>
  </div>
</template>

<style scoped>
.mongo-tree-empty {
  padding: 10px 4px;
  color: var(--text-dim);
  font-size: 12px;
  text-align: center;
}

/* 收起后输入框被压扁，按钮回到固定小尺寸，否则会跟着变形或撑破 46px 宽的竖条 */
.db-tree.collapsed .tree-filter-row .panel-toggle {
  align-self: center;
  height: 22px;
  width: 22px;
}
</style>
