<script setup>
import { ref, onMounted, onBeforeUnmount, nextTick } from 'vue';

const props = defineProps({
  x: { type: Number, required: true },
  y: { type: Number, required: true },
  items: { type: Array, default: () => [] }
});

const emit = defineEmits(['select', 'close']);

const menuEl = ref(null);
const left = ref(props.x);
const top = ref(props.y);

// 子菜单：items 中带 children 的项（如右键菜单里的「设置」），悬停时在右侧展开
const subEl = ref(null);
const openIndex = ref(-1);
const subLeft = ref(0);
const subTop = ref(0);
const rowEls = [];

function hasChildren(it) {
  return !!(it && it.children && it.children.length);
}

function openSub(i) {
  const it = props.items[i];
  if (!hasChildren(it) || it.disabled) {
    openIndex.value = -1;
    return;
  }
  openIndex.value = i;
  nextTick(() => {
    const row = rowEls[i];
    const host = menuEl.value;
    if (!row || !host) return;
    const r = row.getBoundingClientRect();
    const hr = host.getBoundingClientRect();
    subTop.value = Math.max(4, r.top - 5);
    // 默认贴在父菜单右侧，右边放不下就翻到左侧
    const w = subEl.value ? subEl.value.getBoundingClientRect().width : 176;
    const right = hr.right - 4;
    subLeft.value = right + w > window.innerWidth ? Math.max(4, hr.left - w + 4) : right;
  });
}

function onItemClick(i, it) {
  // 置灰项不响应点击
  if (it.disabled) return;
  if (hasChildren(it)) {
    // 有子菜单的项本身不触发动作，点一下切换展开状态
    if (openIndex.value === i) openIndex.value = -1;
    else openSub(i);
    return;
  }
  emit('select', it);
}

// 点菜单以外的地方关闭（子菜单算菜单内部）
function onDocMousedown(ev) {
  const t = ev.target;
  if (menuEl.value && menuEl.value.contains(t)) return;
  if (subEl.value && subEl.value.contains(t)) return;
  emit('close');
}

onMounted(async () => {
  await nextTick();
  if (!menuEl.value) return;
  // 贴边时向左 / 向上翻转，避免菜单溢出视口
  const rect = menuEl.value.getBoundingClientRect();
  const l = props.x + rect.width > window.innerWidth ? props.x - rect.width : props.x;
  const t = props.y + rect.height > window.innerHeight ? props.y - rect.height : props.y;
  left.value = Math.max(4, l);
  top.value = Math.max(4, t);
  document.addEventListener('mousedown', onDocMousedown);
});

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocMousedown);
});
</script>

<template>
  <Teleport to="body">
    <div ref="menuEl" class="rs-menu" :style="{ left: left + 'px', top: top + 'px' }">
      <template v-for="(it, index) in items" :key="index">
        <div v-if="it.separator" class="rs-menu-sep"></div>
        <div
          v-else
          :ref="el => (rowEls[index] = el)"
          class="rs-menu-item"
          :class="{ 'rs-danger': it.danger, 'rs-open': openIndex === index, 'rs-disabled': it.disabled }"
          @click="onItemClick(index, it)"
          @mouseenter="openSub(index)"
        >
          <span class="rs-item-label">{{ it.label }}</span>
          <span v-if="hasChildren(it)" class="rs-sub-arrow">&#9656;</span>
        </div>
      </template>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="openIndex >= 0 && hasChildren(items[openIndex])"
      ref="subEl"
      class="rs-menu"
      :style="{ left: subLeft + 'px', top: subTop + 'px' }"
    >
      <div
        v-for="(c, ci) in items[openIndex].children"
        :key="ci"
        class="rs-menu-item"
        :class="{ 'rs-danger': c.danger }"
        @click="emit('select', c)"
      >{{ c.label }}</div>
    </div>
  </Teleport>
</template>