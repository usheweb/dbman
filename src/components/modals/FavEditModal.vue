<script setup>
import { ref, watch, nextTick } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useUiStore } from '@/stores/ui';
import { useSqlStore } from '@/stores/sql';

const ui = useUiStore();
const sqlStore = useSqlStore();
const nameInput = ref(null);

watch(() => ui.modals.favEditModal, async (v) => {
  if (v) {
    await nextTick();
    if (nameInput.value) nameInput.value.focus();
  }
});

function save() {
  const msg = sqlStore.saveFavDraft();
  if (msg) {
    ui.showToast(msg, 'success');
    ui.closeModal('favEditModal');
  }
}
</script>

<template>
  <ModalShell :visible="ui.modals.favEditModal" width="620px" @close="ui.closeModal('favEditModal')">
    <template #title>{{ sqlStore.favDraft.editId ? '编辑收藏' : '收藏 SQL' }}</template>
    <div class="form-group">
      <label>名称</label>
      <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" ref="nameInput" v-model="sqlStore.favDraft.name" placeholder="留空则自动取 SQL 首行">
    </div>
    <div class="form-group">
      <label>备注（可选）</label>
      <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="sqlStore.favDraft.note" placeholder="例如：查当日订单">
    </div>
    <div class="form-group">
      <label>SQL</label>
      <div class="filter-preview">{{ sqlStore.favDraft.sql }}</div>
    </div>
    <div class="filter-actions">
      <button class="btn btn-secondary" @click="ui.closeModal('favEditModal')">取消</button>
      <button class="btn" @click="save">保存</button>
    </div>
  </ModalShell>
</template>
