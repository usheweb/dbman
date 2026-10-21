<script setup>
import { ref, computed } from 'vue';
import { useConnectionStore } from '@/stores/connection';
import { useUiStore } from '@/stores/ui';
import ContextMenu from '@/components/common/ContextMenu.vue';
import ItemIcon from '@/components/common/ItemIcon.vue';
import { canCreateDatabase, canDropDatabase } from '@/utils/dbType';

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
// 右键点在哪个库上：删库时直接用它，弹窗里不用再选一次
const contextDb = ref('');

// 只有已打开（有 tab）的那条才给「断开连接」；支持建库/删库的类型才给对应菜单
const menuItems = computed(() => {
  const items = [{ label: '编辑', value: 'edit' }];
  if (canCreateDatabase(props.conn.type)) items.push({ label: '创建数据库', value: 'create-db' });
  if (canDropDatabase(props.conn.type)) items.push({ label: '删除数据库', value: 'drop-db' });
  items.push({ label: '删除连接', value: 'remove', danger: true });
  if (connStore.isOpen(props.conn.id)) items.push({ label: '断开连接', value: 'disconnect', danger: true });
  return items;
});

function onContextMenu(e) {
  // 在连接卡片空白处右键：没有指定库，删库时仍由弹窗选择
  contextDb.value = '';
  menuX.value = e.clientX;
  menuY.value = e.clientY;
  menuVisible.value = true;
}

function onDbContextMenu(e, db) {
  contextDb.value = db;
  menuX.value = e.clientX;
  menuY.value = e.clientY;
  menuVisible.value = true;
}

async function onMenuSelect(item) {
  menuVisible.value = false;
  const val = item && item.value;
  if (val === 'edit') {
    ui.openConnectionModal(props.conn);
  } else if (val === 'create-db') {
    // 建库要落在已建立的连接上：还没打开就按「点击卡片」的方式先连上（失败时 openConnection 已提示）
    try {
      if (!connStore.isOpen(props.conn.id)) await connStore.openConnection(props.conn.id);
    } catch (e) {
      return;
    }
    collapsed.value = false;
    ui.openCreateDbModal(props.conn);
  } else if (val === 'drop-db') {
    // 删库同样要先有连接与库列表，弹窗里的候选就来自这条连接的 databaseList
    try {
      if (!connStore.isOpen(props.conn.id)) await connStore.openConnection(props.conn.id);
    } catch (e) {
      return;
    }
    collapsed.value = false;
    ui.openDropDbModal(props.conn, contextDb.value);
  } else if (val === 'remove') {
    const ok = await ui.askConfirm({
      title: '删除连接',
      message: `确定要删除连接「${props.conn.name || props.conn.host}」吗？`,
      confirmText: '删除',
      danger: true
    });
    if (ok) connStore.removeConnection(props.conn.id);
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
      <span class="conn-drag-handle" v-tip="'按住拖动可调整连接顺序'">⠿</span>
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
        @contextmenu.stop.prevent="onDbContextMenu($event, db)"
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
