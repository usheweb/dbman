<script setup>
import { ref, watch, onUnmounted } from 'vue';
import promoImg from '@/assets/tslclock.png';
import { useStoreStore } from '@/stores/store';
import { dayKeyAfter } from '@/utils/format';

// 跳过时长：勾选后 7 天内不再弹出，到期后（若仍勾着）再弹一次，等效「每周一次」
const SKIP_DAYS = 7;

// 启动推广弹窗：默认每周弹一次，倒计时结束后才允许关闭
const props = defineProps({
  visible: { type: Boolean, default: false }
});
const emit = defineEmits(['close']);

const store = useStoreStore();
// 「7 天内不再提示」：勾选偏好持久化，用户勾过一次以后每次打开都保持勾选，
// 于是自动变成「每周只弹一次」；主动取消勾选则恢复每次启动都弹
const skipWeek = ref(false);

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
  // 沿用上次的勾选偏好：勾过就一直勾上（也就是持续每周一次）
  skipWeek.value = store.get('dbman_promo_skip_enabled') === '1';
  startTimer();
});

document.addEventListener('visibilitychange', onResume);
window.addEventListener('focus', onResume);

onUnmounted(() => {
  stopTimer();
  document.removeEventListener('visibilitychange', onResume);
  window.removeEventListener('focus', onResume);
});

async function close() {
  if (remain.value > 0) return;
  // 勾选状态持久化：勾了就记下「到期日 = 今天 + 7 天」，启动时据此跳过弹窗；
  // 到下一周过期时再弹一次，因为仍然勾着，关闭后又顺延一周 —— 即每周一次。
  // 这里等写盘完成再关闭，避免用户紧接着重启时磁盘上还没有这些键
  await store.set('dbman_promo_skip_enabled', skipWeek.value ? '1' : '0');
  if (skipWeek.value) await store.set('dbman_promo_skip_until', dayKeyAfter(SKIP_DAYS));
  emit('close');
}
</script>

<template>
  <!-- 鼠标移回窗口时再兜底重算一次，防止系统未派发 focus/visibilitychange -->
  <div class="modal" :class="{ hidden: !visible }" @pointerenter="sync">
    <div class="modal-content promo-content">
      <div class="modal-scroll">
        <img class="promo-img" :src="promoImg" alt="唐三浪学习时钟">
        <div class="promo-actions">
          <label class="promo-skip">
            <input type="checkbox" v-model="skipWeek">
            <span>7 天内不再提示</span>
          </label>
          <button class="btn promo-btn" :disabled="remain > 0" @click="close">
            {{ remain > 0 ? `${remain} 秒后可关闭` : '关闭' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.promo-content {
  width: 640px;
}

.promo-content .modal-scroll {
  padding: 18px;
}

.promo-img {
  display: block;
  width: 100%;
  border-radius: 8px;
}

.promo-actions {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 18px;
  margin-top: 16px;
}

.promo-skip {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--text-dim);
  font-family: var(--font-cjk);
  font-size: 13px;
  letter-spacing: .5px;
  cursor: pointer;
  user-select: none;
}

.promo-skip input {
  width: 14px;
  height: 14px;
  margin: 0;
  accent-color: var(--cyan);
  cursor: pointer;
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
