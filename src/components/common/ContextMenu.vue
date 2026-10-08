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

function onSelect(item) {
  emit('select', item);
}

// 点菜单以外的地方关闭
function onDocMousedown(ev) {
  if (menuEl.value && menuEl.value.contains(ev.target)) return;
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
          class="rs-menu-item"
          :class="{ 'rs-danger': it.danger }"
          @click="onSelect(it)"
        >{{ it.label }}</div>
      </template>
    </div>
  </Teleport>
</template>
