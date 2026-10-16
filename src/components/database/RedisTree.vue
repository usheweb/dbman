<script setup>
import { watch, computed, onBeforeUnmount } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import {
  useRedis, loadRedisInfo, loadRedisKeys, resetRedis,
  redisKeysNextPage, redisKeysPrevPage, toGlob, globMatch
} from '@/composables/useRedis';
import ItemIcon from '@/components/common/ItemIcon.vue';
import IconSelect from '@/components/common/IconSelect.vue';

const ui = useUiStore();
const connStore = useConnectionStore();
const { keys, keyword, globalSearch, selectedKey, keyTotal, pageIndex, hasPrev, hasNext } = useRedis();

// 「本页」模式只过滤已经取回的这一页，零请求；「全库」模式把关键词交给服务端 SCAN 匹配
const visibleKeys = computed(() => {
  if (globalSearch.value) return keys.value;
  const k = keyword.value.trim();
  if (!k) return keys.value;
  const glob = toGlob(k);
  return keys.value.filter(item => globMatch(glob, item.name));
});

// 分页文案：keyTotal 是 DBSIZE（整个库），带关键词时不能当命中数用
const pagerText = computed(() => {
  const base = '第 ' + pageIndex.value + ' 页';
  if (!keyword.value.trim()) return base + ' · 共 ' + keyTotal.value + ' 个 key';
  if (globalSearch.value) return base;
  return base + ' · 本页命中 ' + visibleKeys.value.length + ' 个';
});

// 空列表文案：全库模式下命中稀疏时，一页可能扫不到任何匹配，但后面还有
const emptyText = computed(() => {
  if (!keyword.value.trim()) return '当前页没有 key';
  if (globalSearch.value && hasNext.value) return '本页无匹配，可翻页继续找';
  return '没有匹配的 key';
});

// 逻辑库下拉：与左侧连接卡片里的库列表同源（connection store 的 databaseList）
const dbOptions = computed(() => connStore.currentDatabaseList.map(db => ({
  value: db,
  label: 'DB ' + db
})));

// Redis 类型 → 图标名；未知类型回退到钥匙图形
const KEY_ICONS = {
  string: 'rkey-string',
  list: 'rkey-list',
  set: 'rkey-set',
  zset: 'rkey-zset',
  hash: 'rkey-hash',
  stream: 'rkey-stream'
};
function keyIcon(type) {
  return KEY_ICONS[type] || 'rkey-none';
}

// 连接是异步建立的：currentConnData 先就绪、后端句柄 serverConnId 后赋值，
// 本组件挂载时句柄往往还是 null（会以 null 句柄调用后端报 connid 类型错误）。
// 这里同时监听「活跃连接」与「连接是否就绪」：切换 tab 或重连后都会重新拉一次。
// info 与 keys 各自 try/catch：info 失败不应连累 key 列表，否则会出现「首次打开列表空白」。
async function loadAll() {
  try {
    await loadRedisInfo(connStore);
  } catch (err) {
    ui.showToast(String(err), 'error');
  }
  try {
    await loadRedisKeys(connStore);
  } catch (err) {
    ui.showToast(String(err), 'error');
  }
}

// 换连接清空上一条的 key / 选中项；换库只重拉列表。
// 建连时 isConnected 与 currentDatabase 会先后各触发一次，用 cid#db 去重避免重复请求
let loadedKey = '';
watch(
  [() => connStore.currentConnId, () => connStore.isConnected, () => connStore.currentDatabase],
  async ([cid, ok, db], [prevCid]) => {
    if (cid !== prevCid) { resetRedis(); loadedKey = ''; }
    if (!ok || !db) return;
    const k = cid + '#' + db;
    if (k === loadedKey) return;
    loadedKey = k;
    selectedKey.value = null;
    await loadAll();
  },
  { immediate: true, flush: 'post' }
);

// ===== 关键词筛选 =====
// 搜索输入：本页模式纯前端过滤（零请求），全库模式 300ms 防抖后交给服务端 SCAN
let searchTimer = null;

async function searchNow() {
  if (searchTimer) {
    clearTimeout(searchTimer);
    searchTimer = null;
  }
  try {
    await loadRedisKeys(connStore);
  } catch (err) {
    ui.showToast(String(err), 'error');
  }
}

function onKeywordInput() {
  if (!globalSearch.value) return;
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(searchNow, 300);
}

// 切换搜索范围：本页 ↔ 全库。两种 range 的 pattern 不同，必须重取第一页
async function toggleScope() {
  globalSearch.value = !globalSearch.value;
  await searchNow();
}

onBeforeUnmount(() => {
  if (searchTimer) clearTimeout(searchTimer);
});

// 翻页：游标分页拿不到总页数，只能按「有没有下一个游标」来判断能否往后翻
async function onPrevPage() {
  try {
    await redisKeysPrevPage(connStore);
  } catch (err) {
    ui.showToast(String(err), 'error');
  }
}

async function onNextPage() {
  try {
    await redisKeysNextPage(connStore);
  } catch (err) {
    ui.showToast(String(err), 'error');
  }
}
</script>

<template>
  <div class="db-tree" :class="{ collapsed: ui.panelState.tree }">
    <div class="tree-header-row">
      <IconSelect
        class="db-picker"
        v-tip="'切换逻辑库'"
        :model-value="connStore.currentDatabase"
        :options="dbOptions"
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
        v-model="keyword"
        placeholder="搜索key..."
        @input="onKeywordInput"
        @keydown.enter="searchNow"
      >
      <button
        class="key-scope-btn"
        :class="{ on: globalSearch }"
        v-tip="globalSearch ? '当前在全部 key 中搜索，点击改为只筛当前页' : '当前只筛当前页，点击改为在全部 key 中搜索'"
        @click="toggleScope"
      >{{ globalSearch ? '全库' : '本页' }}</button>
      <button
        class="panel-toggle"
        @click="ui.togglePanel('tree')"
        v-tip="(ui.panelState.tree ? '展开' : '收起') + ui.panelTitle('tree') + '列'"
      >{{ ui.panelState.tree ? '›' : '‹' }}</button>
    </div>
    <div id="keyList">
      <div
        v-for="k in visibleKeys"
        :key="k.name"
        class="tree-item key-item"
        :class="{ active: selectedKey === k.name }"
        @click="selectedKey = k.name"
      >
        <ItemIcon :name="keyIcon(k.type)" v-tip="k.type" />
        <span class="key-name">{{ k.name }}</span>
      </div>
      <div v-if="!visibleKeys.length" class="key-empty">{{ emptyText }}</div>
    </div>
    <!-- 游标分页：SCAN 每次只取一页，避免大库上 KEYS 全量阻塞 -->
    <div class="tree-pager">
      <button class="row-btn" v-tip="'上一页'" :disabled="!hasPrev" @click="onPrevPage">‹</button>
      <span class="tree-pager-info" v-tip="pagerText">{{ pagerText }}</span>
      <button class="row-btn" v-tip="'下一页'" :disabled="!hasNext" @click="onNextPage">›</button>
    </div>
  </div>
</template>

<style scoped>
/* 与同行输入框等高：行高由输入框撑开，按钮拉伸填满，避免视觉上矮一截 */
.tree-filter-row .key-scope-btn,
.tree-filter-row .panel-toggle {
  align-self: stretch;
  height: auto;
  box-sizing: border-box;
}

/* 收起后输入框被压扁，按钮回到固定小尺寸，否则会跟着变形或撑破 46px 宽的竖条 */
.db-tree.collapsed .tree-filter-row .panel-toggle {
  align-self: center;
  height: 22px;
  width: 22px;
}

/* 搜索范围切换：全库=服务端 SCAN 匹配，本页=只筛当前已取回的一页。
   宽度给最小值，避免两个字的文案把按钮挤成高瘦竖条；字号、圆角与输入框一致 */
.key-scope-btn {
  flex: 0 0 auto;
  min-width: 46px;
  padding: 0 10px;
  font-size: 12px;
  line-height: 1;
  text-align: center;
  white-space: nowrap;
  color: var(--text-dim);
  background: transparent;
  border: 1px solid var(--line);
  border-radius: 6px;
  cursor: pointer;
}

/* 箭头按钮原来是 22px 方块，行高变高后显得细长，放宽到接近正方形 */
.tree-filter-row .panel-toggle {
  width: 34px;
}

.key-scope-btn:hover {
  color: var(--text);
}

.key-scope-btn.on {
  color: var(--cyan);
  border-color: var(--cyan);
}

.key-empty {
  padding: 10px 4px;
  color: var(--text-dim);
  font-size: 12px;
  text-align: center;
}

/* 面板收起时一并隐藏，避免 46px 宽里挤出来 */
.db-tree.collapsed .key-scope-btn {
  max-width: 0;
  padding: 0;
  border-width: 0;
  opacity: 0;
  overflow: hidden;
  pointer-events: none;
}

/* 分页条吸在面板底部，key 列表滚动时始终可见 */
.tree-pager {
  position: sticky;
  bottom: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 8px -12px -12px;
  padding: 8px 12px;
  background: rgba(var(--ink-4-rgb), .96);
  border-top: 1px solid var(--line);
}

.tree-pager-info {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 11px;
  text-align: center;
}

/* 面板收起时一并隐藏，避免 46px 宽里挤出来 */
.db-tree.collapsed .tree-pager {
  opacity: 0;
  pointer-events: none;
}
</style>
