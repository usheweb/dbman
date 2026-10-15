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
// Redis 的「库」是逻辑库序号：列表里显示成 DB 0 / DB 1，避免光看数字不知所云
const isRedis = computed(() => props.conn.type === 'redis');

// 库列表的展开/收起：点击已选中的连接卡片即切换，点别的连接则选中它并展开
const collapsed = ref(false);

function onItemClick() {
  if (isActive.value) {
    collapsed.value = !collapsed.value;
    return;
  }
  collapsed.value = false;
  connStore.selectConnection(props.conn.id);
}

const menuVisible = ref(false);
const menuX = ref(0);
const menuY = ref(0);

// 只有已打开（有 tab）的那条才给「断开连接」
const menuItems = computed(() => {
  const items = [{ label: '编辑', value: 'edit' }, { label: '删除', value: 'remove', danger: true }];
  if (connStore.isOpen(props.conn.id)) items.push({ label: '断开连接', value: 'disconnect', danger: true });
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
    connStore.closeTab(props.conn.id);
  }
}
</script>

<template>
  <div
    class="connection-item"
    :class="{ active: isActive }"
    @click="onItemClick"
    @contextmenu.prevent="onContextMenu"
  >
    <div class="connection-head">
      <span class="connection-name">{{ conn.name }}</span>
      <span class="conn-tag">
        <ItemIcon :name="conn.type" />
        {{ conn.type.toUpperCase() }}
      </span>
      <span v-if="isActive" class="conn-caret" :class="{ collapsed }">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M6 9l6 6 6-6" />
        </svg>
      </span>
    </div>
    <div class="connection-type">{{ conn.host }}:{{ conn.port }}</div>
    <div v-if="isActive" class="database-list" :class="{ show: !collapsed }">
      <div
        v-for="db in connStore.currentDatabaseList"
        :key="db"
        class="database-item"
        :class="{ active: db === connStore.currentDatabase }"
        @click.stop="connStore.selectDatabase(db)"
      >
        <ItemIcon name="database" />
        <span class="item-label">{{ isRedis ? 'DB ' + db : db }}</span>
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
