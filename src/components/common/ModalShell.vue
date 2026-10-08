<script setup>
defineProps({
  visible: { type: Boolean, default: false },
  width: { type: String, default: '500px' }
});

defineEmits(['close']);
</script>

<template>
  <div class="modal" :class="{ hidden: !visible }">
    <div class="modal-content" :style="{ width }">
      <div class="modal-header">
        <h3><slot name="title" /></h3>
        <button class="close-btn" @click="$emit('close')">&times;</button>
      </div>
      <slot />
    </div>
  </div>
</template>
