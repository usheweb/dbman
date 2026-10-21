<script setup>
// 创建数据库：按数据库类型给出对应的建库参数（字符集/排序规则、编码/模板），
// 底部实时预览生成的 CREATE DATABASE 语句，确认后直接在所选连接上执行
import { computed, nextTick, ref, watch } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import IconSelect from '@/components/common/IconSelect.vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';

const ui = useUiStore();
const connStore = useConnectionStore();

const nameEl = ref(null);
const name = ref('');
const charset = ref('utf8mb4');
const collation = ref('utf8mb4_general_ci');
const ifNotExists = ref(false);
const encoding = ref('UTF8');
const template = ref('template0');
const busy = ref(false);
const errorMsg = ref('');

const MYSQL_CHARSETS = [
  { value: 'utf8mb4', label: 'utf8mb4（推荐，完整 Unicode）' },
  { value: 'utf8', label: 'utf8（MySQL 旧版三字节）' },
  { value: 'gbk', label: 'gbk（简体中文）' },
  { value: 'big5', label: 'big5（繁体中文）' },
  { value: 'latin1', label: 'latin1（西欧字符）' }
];

// 排序规则随字符集联动，默认取每组第一个
const MYSQL_COLLATIONS = {
  utf8mb4: ['utf8mb4_general_ci', 'utf8mb4_unicode_ci', 'utf8mb4_bin', 'utf8mb4_0900_ai_ci'],
  utf8: ['utf8_general_ci', 'utf8_unicode_ci', 'utf8_bin'],
  gbk: ['gbk_chinese_ci', 'gbk_bin'],
  big5: ['big5_chinese_ci', 'big5_bin'],
  latin1: ['latin1_swedish_ci', 'latin1_general_ci', 'latin1_bin']
};

const PG_ENCODINGS = [
  { value: 'UTF8', label: 'UTF8（推荐）' },
  { value: 'GBK', label: 'GBK' },
  { value: 'BIG5', label: 'BIG5' },
  { value: 'LATIN1', label: 'LATIN1' },
  { value: 'SQL_ASCII', label: 'SQL_ASCII（不做编码转换）' }
];

const PG_TEMPLATES = [
  { value: 'template0', label: 'template0（可自定义编码）' },
  { value: 'template1', label: 'template1（沿用默认库设置）' }
];

const conn = computed(() => ui.createDbTarget || {});
const dbType = computed(() => String(conn.value.type || '').toLowerCase());
const isMysql = computed(() => dbType.value === 'mysql' || dbType.value === 'mariadb');
const isPg = computed(() => dbType.value === 'postgres' || dbType.value === 'postgresql');
const isMssql = computed(() => dbType.value === 'sqlserver' || dbType.value === 'mssql');

const collationOptions = computed(() =>
  (MYSQL_COLLATIONS[charset.value] || []).map(v => ({ value: v, label: v }))
);

// 换字符集时排序规则跟着换到该字符集的默认值，避免拼出字符集与规则不匹配的语句
watch(charset, (v) => {
  collation.value = (MYSQL_COLLATIONS[v] || [])[0] || '';
});

// 按当前参数拼出建库语句；预览与最终执行用同一个函数，避免两处逻辑走样
function buildSql(nm) {
  if (isMysql.value) {
    return 'CREATE DATABASE ' + (ifNotExists.value ? 'IF NOT EXISTS ' : '')
      + '`' + nm + '`\n  DEFAULT CHARACTER SET ' + charset.value
      + ' COLLATE ' + collation.value;
  }
  if (isPg.value) {
    return 'CREATE DATABASE "' + nm + '"\n  WITH ENCODING \'' + encoding.value
      + '\' TEMPLATE ' + template.value;
  }
  return 'CREATE DATABASE [' + nm + ']';
}

// 语句预览：名字还没填时用占位，让用户先看清语句结构
const sqlPreview = computed(() => buildSql(String(name.value).trim() || '数据库名'));

watch(() => ui.modals.createDbModal, async (v) => {
  if (!v) return;
  name.value = '';
  charset.value = 'utf8mb4';
  collation.value = 'utf8mb4_general_ci';
  ifNotExists.value = false;
  encoding.value = 'UTF8';
  template.value = 'template0';
  busy.value = false;
  errorMsg.value = '';
  await nextTick();
  if (nameEl.value) nameEl.value.focus();
});

async function submit() {
  if (busy.value) return;
  errorMsg.value = '';
  const target = ui.createDbTarget;
  if (!target) return;
  const nm = String(name.value).trim();
  if (!nm) {
    errorMsg.value = '请输入数据库名';
    return;
  }
  // 这些字符在库名里要么非法、要么需要额外的转义规则，直接拦下更稳妥
  if (/[\s`"'\\/.]/.test(nm)) {
    errorMsg.value = '数据库名不能包含空格、引号、反引号、斜杠或点';
    return;
  }
  if (isMysql.value && nm.length > 64) {
    errorMsg.value = 'MySQL 数据库名最长 64 个字符';
    return;
  }
  busy.value = true;
  try {
    await connStore.createDatabaseFor(target.id, buildSql(nm));
    ui.showToast(`数据库「${nm}」已创建`);
    ui.closeModal('createDbModal');
  } catch (e) {
    errorMsg.value = String(e && e.message ? e.message : e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ModalShell :visible="ui.modals.createDbModal" width="520px" @close="ui.closeModal('createDbModal')">
    <template #title>创建数据库</template>
    <div class="export-body">
      <div class="export-row">
        <div class="export-label">连接</div>
        <div class="export-opts">
          <span class="export-hint">{{ conn.name || conn.host }} · {{ dbType.toUpperCase() }}</span>
        </div>
      </div>
      <div class="export-row">
        <div class="export-label">数据库名</div>
        <div class="export-opts">
          <input
            ref="nameEl"
            type="text"
            autocapitalize="off" autocorrect="off" spellcheck="false"
            class="filter-input"
            style="width: 100%;"
            v-model="name"
            placeholder="例如：shop_prod"
            @keyup.enter="submit"
          >
        </div>
      </div>

      <template v-if="isMysql">
        <div class="export-row">
          <div class="export-label">字符集</div>
          <div class="export-opts">
            <IconSelect v-model="charset" :options="MYSQL_CHARSETS" class="create-db-select" />
          </div>
        </div>
        <div class="export-row">
          <div class="export-label">排序规则</div>
          <div class="export-opts">
            <IconSelect v-model="collation" :options="collationOptions" class="create-db-select" />
          </div>
        </div>
        <div class="export-row">
          <div class="export-label">已存在时</div>
          <div class="export-opts">
            <label class="export-opt">
              <input type="checkbox" v-model="ifNotExists">
              加上 IF NOT EXISTS（库已存在时跳过，不报错）
            </label>
          </div>
        </div>
      </template>

      <template v-else-if="isPg">
        <div class="export-row">
          <div class="export-label">编码</div>
          <div class="export-opts">
            <IconSelect v-model="encoding" :options="PG_ENCODINGS" class="create-db-select" />
          </div>
        </div>
        <div class="export-row">
          <div class="export-label">模板库</div>
          <div class="export-opts">
            <IconSelect v-model="template" :options="PG_TEMPLATES" class="create-db-select" />
          </div>
        </div>
        <div class="export-hint">指定非默认编码时需使用 template0，否则可能因模板库编码不兼容而失败。</div>
      </template>

      <div class="form-group" style="margin-top: 6px;">
        <label>语句预览</label>
        <pre class="create-db-sql">{{ sqlPreview }}</pre>
      </div>

      <div v-if="errorMsg" class="create-db-error">{{ errorMsg }}</div>
    </div>
    <div class="confirm-actions">
      <button class="btn btn-secondary" @click="ui.closeModal('createDbModal')">取消</button>
      <button class="btn" :disabled="busy" @click="submit">{{ busy ? '创建中…' : '创建' }}</button>
    </div>
  </ModalShell>
</template>

<style scoped>
/* 下拉本身宽度 100%，在 flex 行里要靠 flex 撑满整行 */
.create-db-select {
  flex: 1;
  min-width: 0;
}

.create-db-sql {
  margin: 0;
  padding: 10px 12px;
  background: rgba(var(--void-rgb), .7);
  border: 1px solid var(--line);
  border-radius: 6px;
  color: var(--cyan);
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-all;
}

.create-db-error {
  margin-top: 8px;
  color: var(--magenta);
  font-size: 12px;
  line-height: 1.5;
}
</style>
