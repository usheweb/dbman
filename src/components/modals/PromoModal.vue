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
// 截止时间戳（墙上时钟）。不能用「每秒减 1」：窗口切到后台时
// WKWebView 会挂起/节流定时器，相对计数会停住，回到前台后弹窗关不掉
let endTime = 0;

function stopTimer() {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
}

// 按截止时间重算剩余秒数，而不是依赖定时器触发次数
function sync() {
  if (!endTime) return;
  remain.value = Math.max(0, Math.ceil((endTime - Date.now()) / 1000));
  if (remain.value <= 0) stopTimer();
}

function startTimer() {
  stopTimer();
  endTime = Date.now() + COUNTDOWN_SECONDS * 1000;
  remain.value = COUNTDOWN_SECONDS;
  timer = setInterval(sync, 1000);
}

// 从后台回到前台时立即重算：后台期间定时器可能一次都没触发
function onResume() {
  if (document.visibilityState === 'hidden') return;
  sync();
}

watch(() => props.visible, (visible) => {
  if (!visible) {
    stopTimer();
    endTime = 0;
    return;
  }
  startTimer();
});

document.addEventListener('visibilitychange', onResume);
window.addEventListener('focus', onResume);

onUnmounted(() => {
  stopTimer();
  document.removeEventListener('visibilitychange', onResume);
  window.removeEventListener('focus', onResume);
});

function close() {
  if (remain.value > 0) return;
  emit('close');
}
</script>

<template>
  <!-- 鼠标移回窗口时再兜底重算一次，防止系统未派发 focus/visibilitychange -->
  <div class="modal" :class="{ hidden: !visible }" @pointerenter="sync">
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
