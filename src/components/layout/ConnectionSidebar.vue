<script setup>
import { ref } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import ConnectionItem from '@/components/connection/ConnectionItem.vue';

const ui = useUiStore();
const connStore = useConnectionStore();

// ===== 连接卡片拖拽排序 =====
// 用 pointer 事件直接移动 DOM（与 tslclock 首页排序同一套思路），不用 HTML5 拖放：
// WKWebView 下 HTML5 drag 的 drop 经常不触发，表现就是松手后卡片弹回原位。
// 拖动时被拖卡片跟手，其余卡片实时让位；松手后把 DOM 顺序写回 store 并持久化
const listEl = ref(null);

// 位移超过这个阈值才算拖动，否则当普通点击（展开 / 切换连接）
const DRAG_THRESHOLD = 5;

let dragEl = null;
let pointerId = null;
let startY = 0;
let dragging = false;
// 跟随鼠标的半透明副本：挂在 body 上，松手即删；原卡片留在列表里当占位
let ghostEl = null;
let grabStartX = 0;
let grabStartY = 0;
// 拖动结束后浏览器还会补发一次 click，用它吞掉，避免误触发展开 / 切库
let swallowClick = false;

function onPointerDown(e) {
  if (e.button !== 0) return;
  // 库列表项上的按下是切库，不参与拖动
  if (e.target.closest && e.target.closest('.database-item')) return;
  dragEl = e.currentTarget;
  pointerId = e.pointerId;
  startY = e.clientY;
  grabStartX = e.clientX;
  grabStartY = e.clientY;
  dragging = false;
  // 没达到拖动阈值就松手时指针可能已经在卡片外，兜底清理状态
  window.addEventListener('pointerup', onPointerUp, { once: true });
}

function onPointerMove(e) {
  if (!dragEl || e.pointerId !== pointerId) return;
  if (!dragging) {
    if (Math.abs(e.clientY - startY) < DRAG_THRESHOLD) return;
    dragging = true;
    dragEl.setPointerCapture(e.pointerId);
    dragEl.classList.add('dragging');
    document.body.style.userSelect = 'none';
    startGhost();
  }
  e.preventDefault();
  // 副本跟手：位置只改 transform，不触发列表重排
  if (ghostEl) {
    ghostEl.style.transform =
      `translate3d(${e.clientX - grabStartX}px, ${e.clientY - grabStartY}px, 0)`;
  }
  // 按鼠标位置算出插到第几个：其余卡片越过中线就让位，拖动过程中就能看到顺序变化
  const others = Array.from(listEl.value.children).filter(el => el !== dragEl);
  let insertAt = others.length;
  for (let i = 0; i < others.length; i++) {
    const rect = others[i].getBoundingClientRect();
    if (e.clientY < rect.top + rect.height / 2) {
      insertAt = i;
      break;
    }
  }
  const ref = others[insertAt] || null;
  // 位置没变（已经插在该插的地方）就不动 DOM，避免每帧重复重排
  if (ref ? ref === dragEl.nextElementSibling : dragEl === listEl.value.lastElementChild) return;

  // FLIP：先记下换位前每张卡的视觉位置，换位后再用 translate 补回差值并过渡到 0，
  // 这样两张卡是「平移换位」而不是瞬间跳到新位置
  const before = new Map();
  Array.from(listEl.value.children).forEach(el => before.set(el, el.getBoundingClientRect()));
  if (ref) listEl.value.insertBefore(dragEl, ref);
  else listEl.value.appendChild(dragEl);
  playFlip(before);
}

// 复制一份卡片挂到 body 上跟随鼠标；原卡片留在列表里压暗当占位
function startGhost() {
  const rect = dragEl.getBoundingClientRect();
  ghostEl = dragEl.cloneNode(true);
  ghostEl.classList.add('conn-drag-ghost');
  ghostEl.classList.remove('dragging');
  ghostEl.style.left = rect.left + 'px';
  ghostEl.style.top = rect.top + 'px';
  ghostEl.style.width = rect.width + 'px';
  document.body.appendChild(ghostEl);
}

function stopGhost() {
  if (!ghostEl) return;
  ghostEl.remove();
  ghostEl = null;
}

function playFlip(before) {
  Array.from(listEl.value.children).forEach(el => {
    const from = before.get(el);
    // 清掉上一轮可能没跑完的位移，读取真实布局位置
    el.style.transition = 'none';
    el.style.transform = '';
    const to = el.getBoundingClientRect();
    if (!from) return;
    const delta = from.top - to.top;
    if (Math.abs(delta) < 1) return;
    el.style.transform = `translateY(${delta}px)`;
    requestAnimationFrame(() => {
      el.style.transition = 'transform 160ms ease';
      el.style.transform = '';
    });
  });
}

function onPointerUp(e) {
  if (!dragEl || e.pointerId !== pointerId) return;
  const el = dragEl;
  const moved = dragging;
  dragEl = null;
  pointerId = null;
  dragging = false;
  // 松手：删掉跟随鼠标的副本，原卡片恢复正常
  stopGhost();
  el.style.transition = '';
  el.style.transform = '';
  el.classList.remove('dragging');
  document.body.style.userSelect = '';
  if (!moved) return;
  swallowClick = true;
  setTimeout(() => { swallowClick = false; }, 0);
  const ids = Array.from(listEl.value.children)
    .map(x => x.dataset.connId)
    .filter(Boolean);
  connStore.applyConnectionOrder(ids);
}

function onClickCapture(e) {
  if (!swallowClick) return;
  swallowClick = false;
  e.stopPropagation();
  e.preventDefault();
}
</script>

<template>
  <div class="sidebar" :class="{ collapsed: ui.panelState.sidebar }">
    <div class="sidebar-header">
      <span>连接</span>
      <div class="sidebar-header-actions">
        <button id="addConnBtn" class="btn-line" @click="ui.openConnectionModal(null)">+ 新建</button>
        <button
          class="panel-toggle"
          v-tip="(ui.panelState.sidebar ? '展开' : '收起') + ui.panelTitle('sidebar') + '列'"
          @click="ui.togglePanel('sidebar')"
        >
          <span class="toggle-glyph">{{ ui.panelState.sidebar ? '›' : '‹' }}</span>
        </button>
      </div>
    </div>
    <div ref="listEl" class="connection-list">
      <div
        v-for="conn in connStore.connections"
        :key="conn.id"
        :data-conn-id="conn.id"
        class="conn-drag-wrap"
        @pointerdown="onPointerDown"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @pointercancel="onPointerUp"
        @click.capture="onClickCapture"
      >
        <ConnectionItem :conn="conn" />
      </div>
    </div>
  </div>
</template>
