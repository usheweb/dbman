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

// ===== 左侧列表面板宽度：拖右边缘调整，宽度持久化在 ui.viewOpts 里 =====
// 表列表与 Redis key 列表共用同一套拖拽逻辑，只在默认宽度 / 最小宽度 / 上限 / 双击恢复值上有差别。
// 宽度写进 CSS 变量 --tree-w 由 theme.css 消费；值为 0 表示「用户没拖过」，
// 这时不写变量，交给 CSS 走内容自适应（Redis 的旧行为）
const TREE_W_DEFAULT = 300; // 表列表双击分隔条恢复到这个宽度
const TREE_W_MIN = 180;

function usePanelWidth({ optKey, minW, maxRatio, maxCap, resetTo }) {
  const wrapEl = ref(null);
  const resizing = ref(false);
  let fromX = 0;
  let fromW = 0;
  // 没拖过（0）就不下发变量，保持 CSS 里的默认值 / 自适应
  const style = computed(() => {
    const w = ui.viewOpts[optKey] || 0;
    return w > 0 ? { '--tree-w': w + 'px' } : {};
  });
  const isAuto = computed(() => !(ui.viewOpts[optKey] > 0));
  function onMove(e) {
    // 上限按窗口宽度的比例取，再宽就把右侧内容挤没了
    const max = Math.max(minW + 60, Math.min(window.innerWidth * maxRatio, maxCap));
    const w = Math.min(max, Math.max(minW, fromW + (e.clientX - fromX)));
    ui.saveViewOpts({ [optKey]: Math.round(w) });
  }
  function stop() {
    resizing.value = false;
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', stop);
  }
  function start(e) {
    if (ui.panelState.tree) return; // 收起态只有 46px，没有可拖的余地
    // 自适应态下也以当前实际宽度为起点，避免一按下就跳到默认宽度
    fromW = wrapEl.value
      ? wrapEl.value.getBoundingClientRect().width
      : (ui.viewOpts[optKey] || 0);
    fromX = e.clientX;
    resizing.value = true;
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', stop);
    e.preventDefault();
  }
  function reset() {
    ui.saveViewOpts({ [optKey]: resetTo });
  }
  onBeforeUnmount(stop);
  return { wrapEl, resizing, style, isAuto, start, reset };
}

const {
  wrapEl: treeWrapEl, resizing: treeResizing, style: treeWrapStyle,
  start: startTreeResize, reset: resetTreeWidth
} = usePanelWidth({ optKey: 'treeWidth', minW: TREE_W_MIN, maxRatio: 0.6, maxCap: 720, resetTo: TREE_W_DEFAULT });

// Redis：没拖过时按内容自适应，一旦拖过就固定为用户宽度，不再自动调节
const {
  wrapEl: redisWrapEl, resizing: redisResizing, style: redisWrapStyle, isAuto: redisAuto,
  start: startRedisResize, reset: resetRedisWidth
} = usePanelWidth({ optKey: 'redisTreeWidth', minW: TREE_W_MIN, maxRatio: 0.6, maxCap: 720, resetTo: 0 });

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
      <!-- key 列表面板：没拖过时 .auto 走内容自适应，拖过之后固定为用户宽度 -->
      <div
        ref="redisWrapEl"
        class="db-tree-wrap"
        :class="{
          collapsed: ui.panelState.tree,
          resizing: redisResizing,
          auto: redisAuto && !ui.panelState.tree
        }"
        :style="redisWrapStyle"
      >
        <RedisTree />
        <div
          v-if="!ui.panelState.tree"
          class="splitter-v"
          v-tip="'拖动调整 key 列表宽度；双击恢复自适应'"
          @mousedown="startRedisResize"
          @dblclick="resetRedisWidth"
        ></div>
      </div>
      <RedisDetail />
    </template>
  </div>
</template>
