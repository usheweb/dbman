// 「@表名」引用：输入框里打 @ 弹候选表，选中后把 @表名 插进需求文本，已引用的表用 chip 展示可点掉。
// 逻辑抽到这里，避免 AiGenerateBar 单文件过长。
import { computed, nextTick, ref } from 'vue';
import { useAiStore } from '@/stores/ai';
import { useConnectionStore } from '@/stores/connection';

// inputEl：需求输入框的模板 ref；onEnter：菜单未消费回车时的回调（触发生成）
export function useTableMention(inputEl, { onEnter } = {}) {
  const ai = useAiStore();
  const conn = useConnectionStore();

  const mentionRef = ref(null);
  const mentionOpen = ref(false);
  const mentionQuery = ref('');
  // prompt 中当前正在输入的 “@” 的绝对下标，以及查询词的结束位置
  const mentionStart = ref(0);
  const mentionEnd = ref(0);

  // 已引用的表：让用户确认模型会看到哪些表
  const mentionedTables = computed(() => {
    const out = [];
    const re = /(?:^|\s)@([^\s@]+)/g;
    let m;
    while ((m = re.exec(ai.prompt || ''))) {
      if (!out.includes(m[1])) out.push(m[1]);
    }
    return out;
  });

  function closeMention() {
    mentionOpen.value = false;
    mentionQuery.value = '';
  }

  // 输入框用 :value + @input 手动同步（避免 v-model 与 @input 同时挂在同一元素上）
  function onInput(ev) {
    ai.prompt = ev.target.value;
    syncMention();
  }

  // 光标前若存在未结束的 @xxx 就弹候选菜单
  function syncMention() {
    if (ai.generating) {
      closeMention();
      return;
    }
    const el = inputEl.value;
    if (!el) return;
    const caret = el.selectionStart ?? 0;
    const before = (ai.prompt || '').slice(0, caret);
    const m = /(?:^|\s)@([^\s@]*)$/.exec(before);
    if (!m) {
      closeMention();
      return;
    }
    mentionStart.value = caret - m[1].length - 1;
    mentionEnd.value = caret;
    mentionQuery.value = m[1];
    mentionOpen.value = true;
  }

  // 菜单开着时按键优先给菜单（上下选择 / 回车选中 / Esc 收起）
  function handleKeydown(ev) {
    if (mentionRef.value && mentionRef.value.handleKeydown(ev)) return;
    if (ev.key === 'Enter') {
      ev.preventDefault();
      if (onEnter) onEnter();
    }
  }

  // 把 [mentionStart, mentionEnd) 替换成 “@表名 ”，光标落在末尾空格之后。
  // 点菜单时输入框会失焦，所以这里只在菜单没开（「+ 当前表」按钮）时才读实时光标。
  function insertTable(name) {
    const el = inputEl.value;
    const text = ai.prompt || '';
    const focused = el && document.activeElement === el;
    const caret = focused ? (el.selectionStart ?? text.length) : text.length;
    const start = mentionOpen.value ? mentionStart.value : caret;
    const end = mentionOpen.value ? mentionEnd.value : caret;
    ai.prompt = text.slice(0, start) + '@' + name + ' ' + text.slice(end);
    closeMention();
    const pos = start + name.length + 2;
    nextTick(() => {
      if (!el) return;
      el.focus();
      el.setSelectionRange(pos, pos);
    });
  }

  function onAddCurrentTable() {
    if (!conn.currentTable) return;
    insertTable(conn.currentTable);
  }

  function removeTable(name) {
    const esc = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    ai.prompt = (ai.prompt || '')
      .replace(new RegExp('(^|\\s)@' + esc + '\\s?'), '$1')
      .trim();
  }

  function onClear() {
    ai.prompt = '';
    closeMention();
  }

  return {
    mentionRef, mentionOpen, mentionQuery, mentionedTables,
    onInput, syncMention, handleKeydown, insertTable, closeMention,
    onAddCurrentTable, removeTable, onClear
  };
}
