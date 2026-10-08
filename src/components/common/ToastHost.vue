<script setup>
import { useUiStore } from '@/stores/ui';

const ui = useUiStore();
</script>

<template>
  <div class="toast" :class="[ui.toast.type, { hidden: !ui.toast.visible }]">{{ ui.toast.msg }}</div>
</template>
