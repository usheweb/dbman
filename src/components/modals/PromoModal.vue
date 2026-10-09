<script setup>
import { ref, watch, onUnmounted } from 'vue';
import promoImg from '@/assets/tslclock.png';

// 启动推广弹窗：每次打开应用都弹一次，倒计时结束后才允许关闭
const props = defineProps({
  visible: { type: Boolean, default: false }
});
const emit = defineEmits(['close']);

const COUNTDOWN_SECONDS = 3;
const remain = ref(COUNTDOWN_SECONDS);
let timer = null;

function stopTimer() {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
}

watch(() => props.visible, (visible) => {
  stopTimer();
  if (!visible) return;
  // 每次弹出都从 3 重新开始倒数
  remain.value = COUNTDOWN_SECONDS;
  timer = setInterval(() => {
    remain.value -= 1;
    if (remain.value <= 0) {
      remain.value = 0;
      stopTimer();
    }
  }, 1000);
});

onUnmounted(stopTimer);

function close() {
  if (remain.value > 0) return;
  emit('close');
}
</script>

<template>
  <div class="modal" :class="{ hidden: !visible }">
    <div class="modal-content promo-content">
      <img class="promo-img" :src="promoImg" alt="唐三浪学习时钟">
      <div class="promo-actions">
        <button class="btn promo-btn" :disabled="remain > 0" @click="close">
          {{ remain > 0 ? `${remain} 秒后可关闭` : '关闭' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.promo-content {
  width: 640px;
  padding: 18px;
}

.promo-img {
  display: block;
  width: 100%;
  border-radius: 8px;
}

.promo-actions {
  display: flex;
  justify-content: center;
  margin-top: 16px;
}

.promo-btn {
  min-width: 150px;
  text-transform: none;
  font-family: var(--font-cjk);
}

.promo-btn:disabled {
  opacity: .5;
  cursor: not-allowed;
  transform: none;
  box-shadow: none;
}
</style>
