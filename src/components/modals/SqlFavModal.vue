<script setup>
import { ref, computed, watch, nextTick } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useUiStore } from '@/stores/ui';
import { useWorkspaceStore } from '@/stores/workspace';
import { useSqlStore } from '@/stores/sql';
import { escapeHtml } from '@/utils/escape';
import { highlightSqlSyntax } from '@/utils/sql';
import { formatDateTime } from '@/utils/format';

const ui = useUiStore();
const ws = useWorkspaceStore();
const sqlStore = useSqlStore();

const keyword = ref('');
const searchInput = ref(null);

const items = computed(() => sqlStore.filterFavs(keyword.value));

watch(() => ui.modals.sqlFavModal, async (v) => {
  if (v) {
    keyword.value = '';
    await nextTick();
    if (searchInput.value) searchInput.value.focus();
  }
});

// 名称 / 备注不是 SQL，只做检索命中高亮
function highlightHit(text, kw) {
  const k = String(kw).toLowerCase();
  if (!k) return escapeHtml(text);
  const lower = String(text).toLowerCase();
  let out = '';
  let pos = 0;
  let i = lower.indexOf(k, pos);
  while (i !== -1) {
    out += escapeHtml(text.slice(pos, i)) + '<span class="log-hit">' + escapeHtml(text.slice(i, i + k.length)) + '</span>';
    pos = i + k.length;
    i = lower.indexOf(k, pos);
  }
  return out + escapeHtml(text.slice(pos));
}

function useFav(id) {
  const f = sqlStore.sqlFavs.find(x => x.id === id);
  if (!f) return;
  ws.setSql(f.sql);
  ui.closeModal('sqlFavModal');
  ui.showToast('已载入 SQL 到编辑器', 'success');
}

function editFav(f) {
  sqlStore.openFavDraft({ sql: f.sql, name: f.name, note: f.note, db: f.db, conn: f.conn, editId: f.id });
  ui.openModal('favEditModal');
}

function deleteFav(f) {
  sqlStore.removeFav(f.id);
  ui.showToast('已删除该条收藏', 'success');
}

function clearFavs() {
  if (!confirm('确定清空全部 SQL 收藏吗？')) return;
  sqlStore.clearFavs();
  ui.showToast('SQL 收藏已清空', 'success');
}
</script>

<template>
  <ModalShell :visible="ui.modals.sqlFavModal" width="900px" @close="ui.closeModal('sqlFavModal')">
    <template #title>SQL 收藏</template>
    <div class="log-tools">
      <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" class="find-input" ref="searchInput" v-model="keyword" placeholder="搜索名称 / 备注 / SQL..." title="支持包含匹配与模糊匹配">
      <span class="log-count">{{ items.length }} / {{ sqlStore.sqlFavs.length }} 条</span>
      <button class="btn btn-secondary" @click="clearFavs">清空收藏</button>
    </div>
    <div class="log-list">
      <div class="log-item" v-for="x in items" :key="x.f.id" @click="useFav(x.f.id)" title="点击载入编辑器">
        <div class="log-meta">
          <span class="log-type" :class="sqlStore.sqlLogClass(x.f.type)">{{ x.f.type }}</span>
          <span class="fav-name" v-html="keyword ? highlightHit(sqlStore.favDisplayName(x.f), keyword) : escapeHtml(sqlStore.favDisplayName(x.f))"></span>
          <span>{{ formatDateTime(new Date(x.f.t)) }}</span>
          <span v-if="x.f.db">库: {{ x.f.db }}</span>
          <span v-if="x.f.conn">连接: {{ x.f.conn }}</span>
          <button class="fav-act" title="编辑名称 / 备注" @click.stop="editFav(x.f)">&#9998;</button>
          <button class="log-del" title="删除这条收藏" @click.stop="deleteFav(x.f)">&times;</button>
        </div>
        <div class="fav-note" v-if="x.f.note" v-html="keyword ? highlightHit(x.f.note, keyword) : escapeHtml(x.f.note)"></div>
        <div class="log-sql" v-html="highlightSqlSyntax(x.f.sql, keyword)"></div>
      </div>
      <div class="log-empty" v-if="!items.length">{{ sqlStore.sqlFavs.length ? '没有匹配的收藏' : '暂无收藏，可在编辑器或 SQL 日志里点 ☆ 收藏' }}</div>
    </div>
  </ModalShell>
</template>
