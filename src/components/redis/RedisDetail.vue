<script setup>
import { ref, computed, watch, nextTick } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { useRedis } from '@/composables/useRedis';
import { escapeHtml } from '@/utils/escape';
import RedisValueEditor from '@/components/redis/RedisValueEditor.vue';
import * as api from '@/api';

const ui = useUiStore();
const connStore = useConnectionStore();
const { selectedKey } = useRedis();

// 支持在此修改的类型
const EDIT_TYPES = ['string', 'list', 'set', 'zset', 'hash'];

const data = ref(null);
const loading = ref(false);
const saving = ref(false);
const editing = ref(false);
const ttlEditing = ref(false);
// 请求令牌：快速连点不同 key 时，只认最后一次的结果，避免旧结果覆盖新选中
let reqToken = 0;

async function loadValue(key) {
  const token = ++reqToken;
  if (!key) {
    data.value = null;
    loading.value = false;
    return;
  }
  // 立刻进入加载态并清掉上一个 key 的内容，让点击马上有反馈
  data.value = null;
  loading.value = true;
  try {
    const d = await connStore.withRetry(id => api.redisGet(id, key));
    if (token !== reqToken) return;
    data.value = d;
    // 可修改的内容直接进入编辑态，省去再点一次「编辑内容」
    editing.value = valueEditable.value;
  } catch (err) {
    if (token !== reqToken) return;
    ui.showToast(String(err), 'error');
  } finally {
    if (token === reqToken) loading.value = false;
  }
}

watch(selectedKey, key => {
  // 换 key 时退出编辑态，避免把 A 的草稿保存到 B
  editing.value = false;
  ttlEditing.value = false;
  loadValue(key);
}, { immediate: true });

// JSON 语法高亮：与原始单文件实现保持一致
function syntaxHighlightJson(json) {
  json = json.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  return json.replace(/("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g, function (match) {
    let cls = 'json-number';
    if (/^"/.test(match)) {
      cls = /:$/.test(match) ? 'json-key' : 'json-string';
    } else if (/true|false/.test(match)) {
      cls = 'json-boolean';
    } else if (/null/.test(match)) {
      cls = 'json-null';
    }
    return '<span class="' + cls + '">' + match + '</span>';
  });
}

const ttlText = computed(() => (data.value ? (data.value.ttl === -1 ? '永久' : data.value.ttl + 's') : ''));

// 把值整理成待展示的纯文本，是 JSON 就美化后展示
const rendered = computed(() => {
  const d = data.value;
  if (!d) return { text: '', isJson: false };
  if (typeof d.value === 'string') {
    try {
      return { text: JSON.stringify(JSON.parse(d.value), null, 2), isJson: true };
    } catch { /* 不是 JSON，按原文展示 */ }
  }
  try {
    return { text: JSON.stringify(d.value, null, 2), isJson: true };
  } catch {
    return { text: String(d.value), isJson: false };
  }
});

const displayHtml = computed(() => {
  const t = rendered.value.text;
  if (!t) return '';
  return rendered.value.isJson ? syntaxHighlightJson(t) : '<pre>' + escapeHtml(t) + '</pre>';
});

// 是否允许在此修改内容：只按类型判断，内容再大也不再限制
const valueEditable = computed(() => !!data.value && EDIT_TYPES.includes(data.value.type));

const readonlyReason = computed(() => {
  const d = data.value;
  if (!d || EDIT_TYPES.includes(d.type)) return '';
  return d.type === 'stream' ? '暂不支持在此修改 stream 类型' : '暂不支持在此修改该类型';
});

// ===== 修改内容 =====
async function onSaveValue(value) {
  // 清空集合等于删除该 key，先明确确认
  const emptied = Array.isArray(value) ? value.length === 0 : (value && typeof value === 'object' ? Object.keys(value).length === 0 : false);
  if (emptied && !confirm('保存后该 Key 将不再有内容（等于删除），确定继续吗？')) return;
  saving.value = true;
  try {
    await connStore.withRetry(id => api.redisSetValue(id, selectedKey.value, value));
    ui.showToast('已保存', 'success');
    // 重载后由 loadValue 统一决定是否继续处于编辑态
    await loadValue(selectedKey.value);
  } catch (err) {
    ui.showToast(String(err), 'error');
  } finally {
    saving.value = false;
  }
}

// ===== 修改 TTL =====
const ttlDraft = ref('');
const ttlInputEl = ref(null);

function startTtlEdit() {
  if (!data.value) return;
  ttlDraft.value = String(data.value.ttl);
  ttlEditing.value = true;
  nextTick(() => {
    if (ttlInputEl.value) {
      ttlInputEl.value.focus();
      ttlInputEl.value.select();
    }
  });
}

async function saveTtl() {
  const ttl = Number(ttlDraft.value);
  if (!Number.isInteger(ttl) || ttl === 0 || ttl < -1) {
    ui.showToast('TTL 需为正整数秒数，或填 -1 表示永久', 'error');
    return;
  }
  try {
    await connStore.withRetry(id => api.redisSetTtl(id, selectedKey.value, ttl));
    // 本地同步，避免为一次 TTL 变更重拉整个大值
    if (data.value) data.value.ttl = ttl;
    ttlEditing.value = false;
    ui.showToast('TTL 已更新', 'success');
  } catch (err) {
    ui.showToast(String(err), 'error');
  }
}
</script>

<template>
  <div class="workspace redis-detail">
    <div v-if="!selectedKey" class="redis-empty">请从左侧选择一个 Key</div>
    <template v-else>
      <!-- Key 名先显示出来，让点击立刻有反馈 -->
      <div class="redis-head">
        <h3>{{ selectedKey }}</h3>
        <div v-if="data" class="redis-meta">
          <span class="redis-badge">{{ data.type }}</span>
          <span class="redis-ttl">
            <template v-if="!ttlEditing">
              TTL：{{ ttlText }}
              <button class="row-btn" v-tip="'修改 TTL'" @click="startTtlEdit">改</button>
            </template>
            <template v-else>
              TTL：
              <input
                ref="ttlInputEl"
                v-model="ttlDraft"
                type="number"
                class="ttl-input"
                min="-1"
                @keydown.enter="saveTtl"
                @keydown.esc="ttlEditing = false"
              >
              <span class="ttl-unit">秒</span>
              <button class="row-btn" v-tip="'保存 TTL'" @click="saveTtl">√</button>
              <button class="row-btn" v-tip="'取消'" @click="ttlEditing = false">×</button>
              <span class="ttl-hint">-1 = 永久</span>
            </template>
          </span>
          <span class="redis-head-spacer"></span>
          <button v-if="valueEditable && !editing" class="btn btn-secondary" @click="editing = true">编辑内容</button>
          <span v-else-if="!valueEditable" class="redis-readonly" v-tip="readonlyReason">{{ readonlyReason }}</span>
        </div>
        <p v-else>读取中…</p>
      </div>
      <div v-if="loading" class="redis-loading">
        <span class="spinner"></span>正在读取内容…
      </div>
      <template v-else-if="data">
        <RedisValueEditor
          v-if="editing"
          :type="data.type"
          :value="data.value"
          :saving="saving"
          @save="onSaveValue"
          @cancel="editing = false"
        />
        <template v-else>
          <!-- 值可能很长：盒子内部滚动，头部信息保持可见 -->
          <div class="large-textarea redis-value" v-html="displayHtml"></div>
        </template>
      </template>
    </template>
  </div>
</template>

<style scoped>
.redis-detail {
  padding: 20px;
}

.redis-empty {
  color: var(--text-dim);
}

.redis-head h3 {
  margin-bottom: 10px;
}

.redis-meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  margin-bottom: 12px;
  font-size: 13px;
  color: var(--text-dim);
}

.redis-badge {
  padding: 2px 8px;
  border: 1px solid rgba(var(--cyan-rgb), .45);
  border-radius: 10px;
  color: var(--cyan);
  font-family: var(--font-mono);
  font-size: 11px;
  letter-spacing: .5px;
}

.redis-ttl {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-family: var(--font-mono);
  font-size: 12px;
}

.ttl-input {
  width: 90px;
  padding: 4px 8px;
  background: rgba(var(--void-rgb), .6);
  border: 1px solid var(--line);
  border-radius: 5px;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 12px;
  outline: none;
}

.ttl-input:focus {
  border-color: var(--cyan);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .25);
}

.ttl-unit,
.ttl-hint {
  color: var(--text-dim);
  font-size: 11px;
}

.redis-head-spacer {
  flex: 1;
}

/* 类型不支持修改时的只读提示 */
.redis-readonly {
  max-width: 320px;
  color: var(--magenta);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.redis-loading {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--text-dim);
  font-size: 13px;
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid var(--line);
  border-top-color: var(--cyan);
  border-radius: 50%;
  animation: redis-spin .7s linear infinite;
}

@keyframes redis-spin {
  to { transform: rotate(360deg); }
}

/* 覆盖 .large-textarea 的 min-height：撑满剩余高度并在内部滚动，避免超长值被裁掉 */
.redis-value {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
</style>