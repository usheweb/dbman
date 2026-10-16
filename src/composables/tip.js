// 全项目统一的悬浮提示（tooltip）：全局指令 v-tip + 挂在 body 上的单例宿主气泡。
//
// 为什么不用原生 title：系统灰气泡样式跟项目风格完全不搭（方圆角、无描边、无主题），
// 而且显示延迟不可控、长文本会被系统截断。这里全部换成自绘气泡，亮/暗主题都能跟随。
//
// 用法：
//   <button v-tip="'静态文案'">…</button>         静态文案（指令值是表达式，所以静态串要套引号）
//   <button v-tip="动态文案">…</button>           动态：变量 / 三元 / 拼接 / 模板串都行
//   setTip(el, '文案')                            程序化：给 DOM 里动态创建的元素挂提示（如 CodeMirror 的 gutter 按钮）
//
// 实现要点：
// 1) 监听走 document 级委托（mouseover / focusin），不给每个元素单独挂事件，109 处迁移零成本；
// 2) 定位/显隐由单例宿主管，position: fixed，所以溢出隐藏的容器（表树面板等）也压不住气泡；
// 3) 禁用态的 <button> 不派发鼠标事件，改用 document.elementFromPoint 命中测试找回真正的目标。

const SHOW_DELAY = 320; // 悬停多久才显示，太灵敏会一路闪气泡
const HIDE_DELAY = 60; // 移开后延迟消失，鼠标扫过时不至于立刻闪掉
const GAP = 9; // 气泡与目标的间距
const EDGE = 8; // 气泡与视口边缘的最小距离

let host = null;
let textEl = null;
let caretEl = null;
let inited = false;
let currentEl = null; // 当前挂着气泡的目标元素
let showTimer = 0;
let hideTimer = 0;

function ensureHost() {
  if (host) return host;
  host = document.createElement('div');
  host.className = 'dbman-tip';
  host.setAttribute('role', 'tooltip');
  caretEl = document.createElement('span');
  caretEl.className = 'dbman-tip-caret';
  textEl = document.createElement('span');
  textEl.className = 'dbman-tip-text';
  host.appendChild(caretEl);
  host.appendChild(textEl);
  document.body.appendChild(host);
  return host;
}

// 算位置：默认压在目标上方，上方放不下就翻到下方；左右贴边时收进视口内，
// 小箭头跟着目标的中点走（同时收在气泡两端以内，免得箭头跑出圆角）
function place(el) {
  const rect = el.getBoundingClientRect();
  const w = host.offsetWidth;
  const h = host.offsetHeight;
  const vw = document.documentElement.clientWidth;
  const vh = document.documentElement.clientHeight;

  const above = rect.top - h - GAP >= EDGE;
  let top = above ? rect.top - h - GAP : rect.bottom + GAP;
  top = Math.min(Math.max(top, EDGE), Math.max(vh - h - EDGE, EDGE));

  const anchorX = rect.left + rect.width / 2;
  const left = Math.min(Math.max(anchorX - w / 2, EDGE), Math.max(vw - w - EDGE, EDGE));

  host.style.left = `${Math.round(left)}px`;
  host.style.top = `${Math.round(top)}px`;
  host.classList.toggle('above', above);

  const caretLeft = Math.min(Math.max(anchorX - left, 12), Math.max(w - 12, 12));
  caretEl.style.left = `${Math.round(caretLeft - 4)}px`;
  caretEl.style.top = above ? 'auto' : '-4px';
  caretEl.style.bottom = above ? '-4px' : 'auto';
}

function showNow(el) {
  if (!el || !el.isConnected) return;
  const text = el.dataset.tip || '';
  if (!text) return;
  ensureHost();
  currentEl = el;
  textEl.textContent = text;
  host.classList.remove('show');
  place(el);
  // 位置先落好再淡入：下一帧加 class，让过渡真的跑起来（否则会从上一位置滑过来）
  requestAnimationFrame(() => {
    if (currentEl === el) host.classList.add('show');
  });
}

function hideNow() {
  clearTimeout(showTimer);
  clearTimeout(hideTimer);
  showTimer = hideTimer = 0;
  currentEl = null;
  if (host) host.classList.remove('show');
}

function hideSoon() {
  clearTimeout(showTimer);
  showTimer = 0;
  if (!currentEl) return;
  clearTimeout(hideTimer);
  hideTimer = setTimeout(() => {
    if (currentEl) {
      currentEl = null;
      host.classList.remove('show');
    }
  }, HIDE_DELAY);
}

function showSoon(el) {
  if (el === currentEl) {
    clearTimeout(hideTimer);
    hideTimer = 0;
    return;
  }
  hideNow();
  hideTimer = 0;
  showTimer = setTimeout(() => {
    showTimer = 0;
    showNow(el);
  }, SHOW_DELAY);
}

// 命中的提示元素：优先用命中测试（能取回被父元素吞掉事件的禁用按钮），取不到再从事件目标往上找
function resolveTarget(e) {
  const real = document.elementFromPoint(e.clientX, e.clientY);
  const hit = real instanceof Element ? real.closest('[data-tip]') : null;
  if (hit) return hit;
  const t = e.target;
  return t instanceof Element ? t.closest('[data-tip]') : null;
}

function onOver(e) {
  const el = resolveTarget(e);
  if (!el) return hideSoon();
  showSoon(el);
}

function onOut(e) {
  if (!currentEl && !showTimer) return;
  // 指针只是移到了目标内部的子元素上，不算离开
  const to = e.relatedTarget;
  if (to instanceof Node && currentEl && currentEl.contains(to)) return;
  hideSoon();
}

function onFocusIn(e) {
  const t = e.target;
  const el = t instanceof Element ? t.closest('[data-tip]') : null;
  if (!el) return;
  clearTimeout(hideTimer);
  clearTimeout(showTimer);
  showNow(el); // 键盘 Tab 过来时立即显示，不等待
}

// 初始化：建宿主 + 挂全局监听（只在 main.js 调一次）
export function initTip() {
  if (inited) return;
  inited = true;
  ensureHost();
  document.addEventListener('mouseover', onOver);
  document.addEventListener('mouseout', onOut);
  document.addEventListener('focusin', onFocusIn);
  document.addEventListener('focusout', hideNow);
  // 按下鼠标（点击）或滚动时立刻收起：此时目标多半已变化或已随内容滚走
  document.addEventListener('mousedown', hideNow, true);
  document.addEventListener('keydown', e => { if (e.key === 'Escape') hideNow(); }, true);
  window.addEventListener('scroll', hideNow, true);
  window.addEventListener('resize', hideNow);
  window.addEventListener('blur', hideNow);
}

// 程序化挂提示：给 JS 里动态创建的 DOM 用（模板里请直接用 v-tip）
export function setTip(el, text) {
  if (!el) return;
  const t = String(text == null ? '' : text);
  if (el.getAttribute && el.getAttribute('title') != null) el.removeAttribute('title');
  if (t) el.dataset.tip = t;
  else delete el.dataset.tip;
}

export function removeTip(el) {
  if (el) delete el.dataset.tip;
}

// 指令值 → 文案。值写成 title 属性时（兼容写法）也认，同时把原生 title 摘掉
function applyTip(el, binding) {
  const fromTitle = el.getAttribute ? el.getAttribute('title') : null;
  if (fromTitle != null) el.removeAttribute('title'); // 摘掉原生灰气泡
  const value = binding && binding.value;
  const own = typeof value === 'number' ? String(value) : (typeof value === 'string' ? value : '');
  const text = own || fromTitle || '';
  if (text) el.dataset.tip = text;
  else delete el.dataset.tip;
}

export const vTip = {
  mounted: applyTip,
  updated: applyTip,
  unmounted(el) {
    delete el.dataset.tip;
    if (currentEl === el) hideNow();
  }
};
