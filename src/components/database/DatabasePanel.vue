<script setup>
import { computed, onBeforeUnmount, ref } from 'vue';
import { useConnectionStore } from '@/stores/connection';
import { useUiStore } from '@/stores/ui';
import { isRelational, isFileBased, isMongoLike } from '@/utils/dbType';
import TableTree from '@/components/database/TableTree.vue';
import RedisTree from '@/components/database/RedisTree.vue';
import MongoTree from '@/components/database/MongoTree.vue';
import RedisDetail from '@/components/redis/RedisDetail.vue';
import MongoWorkspace from '@/components/mongo/MongoWorkspace.vue';
import SqlWorkspace from '@/components/workspace/SqlWorkspace.vue';

const connStore = useConnectionStore();
const ui = useUiStore();

// ===== 表列表面板宽度：拖右边缘调整，宽度持久化在 ui.viewOpts.treeWidth =====
const TREE_W_DEFAULT = 300; // 双击分隔条恢复到这个宽度
const TREE_W_MIN = 180;
const treeWrapEl = ref(null);
const treeResizing = ref(false);
// 宽度走 CSS 变量交给 theme.css，避免在这里拼一长串内联样式
const treeWrapStyle = computed(() => ({ '--tree-w': (ui.viewOpts.treeWidth || TREE_W_DEFAULT) + 'px' }));

let treeFromX = 0;
let treeFromW = 0;
function startTreeResize(e) {
  if (ui.panelState.tree) return; // 收起态只有 46px，没有可拖的余地
  treeFromX = e.clientX;
  treeFromW = treeWrapEl.value
    ? treeWrapEl.value.getBoundingClientRect().width
    : (ui.viewOpts.treeWidth || TREE_W_DEFAULT);
  treeResizing.value = true;
  document.addEventListener('mousemove', onTreeResizeMove);
  document.addEventListener('mouseup', stopTreeResize);
  e.preventDefault();
}
function onTreeResizeMove(e) {
  // 上限取窗口宽度的 60%，再宽就把查询区挤没了
  const max = Math.max(TREE_W_MIN + 60, Math.min(window.innerWidth * 0.6, 720));
  const w = Math.min(max, Math.max(TREE_W_MIN, treeFromW + (e.clientX - treeFromX)));
  ui.saveViewOpts({ treeWidth: Math.round(w) });
}
function stopTreeResize() {
  treeResizing.value = false;
  document.removeEventListener('mousemove', onTreeResizeMove);
  document.removeEventListener('mouseup', stopTreeResize);
}
function resetTreeWidth() {
  ui.saveViewOpts({ treeWidth: TREE_W_DEFAULT });
}
onBeforeUnmount(stopTreeResize);

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
      <div
        ref="treeWrapEl"
        class="db-tree-wrap"
        :class="{ collapsed: ui.panelState.tree, resizing: treeResizing }"
        :style="treeWrapStyle"
      >
        <TableTree />
        <div
          v-if="!ui.panelState.tree"
          class="splitter-v"
          v-tip="'拖动调整表列表宽度；双击恢复默认'"
          @mousedown="startTreeResize"
          @dblclick="resetTreeWidth"
        ></div>
      </div>
      <SqlWorkspace />
    </template>
    <template v-else-if="isMongoLike(connStore.currentConnData.type)">
      <MongoTree />
      <MongoWorkspace />
    </template>
    <template v-else>
      <RedisTree />
      <RedisDetail />
    </template>
  </div>
</template>
