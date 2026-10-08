<script setup>
import { ref, computed } from 'vue';
import { useConnectionStore } from '@/stores/connection';
import { useUiStore } from '@/stores/ui';
import ContextMenu from '@/components/common/ContextMenu.vue';
import ItemIcon from '@/components/common/ItemIcon.vue';

const props = defineProps({
  conn: { type: Object, required: true }
});

const connStore = useConnectionStore();
const ui = useUiStore();

const isActive = computed(() => props.conn.id === connStore.currentConnId);

const menuVisible = ref(false);
const menuX = ref(0);
const menuY = ref(0);

// 只有当前已连上的那条才给「断开连接」
const menuItems = computed(() => {
  const items = [{ label: '编辑', value: 'edit' }, { label: '删除', value: 'remove', danger: true }];
  if (props.conn.id === connStore.currentConnId) items.push({ label: '断开连接', value: 'disconnect', danger: true });
  return items;
});

function onContextMenu(e) {
  menuX.value = e.clientX;
  menuY.value = e.clientY;
  menuVisible.value = true;
}

function onMenuSelect(item) {
  menuVisible.value = false;
  const val = item && item.value;
  if (val === 'edit') {
    ui.openConnectionModal(props.conn);
  } else if (val === 'remove') {
    if (confirm('确定要删除这个连接吗？')) connStore.removeConnection(props.conn.id);
  } else if (val === 'disconnect') {
    connStore.disconnectFromUi();
  }
}
</script>

<template>
  <div
    class="connection-item"
    :class="{ active: isActive }"
    @click="connStore.selectConnection(conn.id)"
    @contextmenu.prevent="onContextMenu"
  >
    <div class="connection-head">
      <span class="connection-name">{{ conn.name }}</span>
      <span class="conn-tag">
        <ItemIcon :name="conn.type" />
        {{ conn.type.toUpperCase() }}
      </span>
    </div>
    <div class="connection-type">{{ conn.host }}:{{ conn.port }}</div>
    <div v-if="isActive" class="database-list show">
      <div
        v-for="db in connStore.currentDatabaseList"
        :key="db"
        class="database-item"
        :class="{ active: db === connStore.currentDatabase }"
        @click.stop="connStore.selectDatabase(db)"
      >
        <ItemIcon name="database" />
        <span class="item-label">{{ db }}</span>
      </div>
    </div>
    <ContextMenu
      v-if="menuVisible"
      :x="menuX"
      :y="menuY"
      :items="menuItems"
      @select="onMenuSelect"
      @close="menuVisible = false"
    />
  </div>
</template>
