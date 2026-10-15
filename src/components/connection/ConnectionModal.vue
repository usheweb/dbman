<script setup>
import { reactive, watch } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { isRelational, isFileBased } from '@/utils/dbType';
import * as api from '@/api';
import IconSelect from '@/components/common/IconSelect.vue';

const ui = useUiStore();
const connStore = useConnectionStore();

// 数据库类型选项（图标为自绘单色版，见 ItemIcon）
const DB_TYPES = [
  { value: 'mysql', label: 'MySQL', icon: 'mysql' },
  { value: 'mariadb', label: 'MariaDB', icon: 'mariadb' },
  { value: 'postgres', label: 'PostgreSQL', icon: 'postgres' },
  { value: 'sqlite', label: 'SQLite', icon: 'sqlite' },
  { value: 'sqlserver', label: 'SQL Server', icon: 'sqlserver' },
  { value: 'oracle', label: 'Oracle', icon: 'oracle' },
  { value: 'redis', label: 'Redis', icon: 'redis' }
];

// 各类型的默认端口：MySQL / MariaDB 3306，PostgreSQL 5432，
// SQL Server 1433，Oracle 1521，Redis 6379
const DEFAULT_PORTS = {
  mysql: 3306,
  mariadb: 3306,
  postgres: 5432,
  sqlserver: 1433,
  oracle: 1521,
  redis: 6379
};

// 需要用户名/密码认证的关系型库（SQLite 为本地文件，无认证）
function needsAuth(type) {
  return isRelational(type) && !isFileBased(type);
}

// 「数据库」字段的标签随类型变化：SQLite 填文件路径，Oracle 填服务名
function dbFieldLabel(type) {
  if (isFileBased(type)) return '数据库文件';
  if (type === 'oracle') return '服务名(Service Name)';
  return '数据库';
}

function defaultPort(type) {
  return DEFAULT_PORTS[type] || 3306;
}

const form = reactive({
  name: '',
  type: 'mysql',
  host: '',
  port: defaultPort('mysql'),
  user: '',
  password: '',
  database: '',
  db: 0,
  useSsh: false,
  sshHost: '',
  sshPort: 22,
  sshUser: '',
  sshPassword: '',
  sshKey: ''
});

// 用编辑目标（或新建时的默认值）填充表单
function fillForm() {
  const conn = ui.connModalTarget;
  form.name = conn ? conn.name : '';
  form.type = conn ? conn.type : 'mysql';
  form.host = conn ? conn.host : '';
  form.port = conn && conn.port ? conn.port : defaultPort(form.type);
  form.user = conn ? conn.user : '';
  form.password = conn ? conn.password : '';
  form.database = conn ? (conn.database || '') : '';
  form.db = conn ? (conn.db || 0) : 0;
  form.useSsh = conn ? !!conn.ssh : false;
  if (conn && conn.ssh) {
    form.sshHost = conn.ssh.host || '';
    form.sshPort = conn.ssh.port || 22;
    form.sshUser = conn.ssh.username || '';
    form.sshPassword = conn.ssh.password || '';
    form.sshKey = conn.ssh.privateKey || '';
  } else {
    form.sshHost = '';
    form.sshPort = 22;
    form.sshUser = '';
    form.sshPassword = '';
    form.sshKey = '';
  }
}

watch(
  () => [ui.modals.connectionModal, ui.connModalTarget],
  () => {
    if (ui.modals.connectionModal) fillForm();
  }
);

// 切换类型时同步端口：仍是上个类型的默认端口（说明用户没手填过）才跟着换，手填过的值保留
function onTypeChange(type) {
  const prevDefault = defaultPort(form.type);
  if (!form.port || Number(form.port) === prevDefault) form.port = defaultPort(type);
  form.type = type;
}

function buildSsh() {
  return {
    enabled: true,
    host: form.sshHost,
    port: Number(form.sshPort),
    username: form.sshUser,
    password: form.sshPassword,
    privateKey: form.sshKey
  };
}

// 与 getConnectionConfig 完全一致：关系型用 database，redis 用 db
function buildPayload() {
  const config = {
    host: form.host,
    port: Number(form.port),
    user: form.user,
    password: form.password
  };
  if (isRelational(form.type)) config.database = form.database;
  else config.db = Number(form.db);
  return {
    type: form.type,
    config,
    ssh: form.useSsh ? buildSsh() : { enabled: false }
  };
}

// SQLite：选择一个已有的数据库文件作为「数据库」
async function pickSqliteFile() {
  const picked = await api.openDialog({
    multiple: false,
    filters: [{ name: 'SQLite', extensions: ['db', 'sqlite', 'sqlite3', 'db3'] }]
  });
  if (picked) form.database = picked;
}

// 用户名必填针对需要认证的关系型库：留空时 MySQL 会按匿名账号登录，只能看到 information_schema 等极少数库。
// SQLite 为本地文件，Redis 走密码认证（后端固定不带用户名），所以不校验用户名。
function validate() {
  if (needsAuth(form.type) && !form.user.trim()) {
    ui.showToast('请填写用户名（如 MySQL 一般填 root）', 'error');
    return false;
  }
  return true;
}

async function onTest() {
  if (!validate()) return;
  try {
    await connStore.testConnection(buildPayload());
    ui.showToast('连接成功！', 'success');
  } catch (err) {
    ui.showToast('连接失败：' + String(err), 'error');
  }
}

function onSave() {
  if (!validate()) return;
  const id = ui.connModalTarget ? ui.connModalTarget.id : connStore.newConnectionId();
  connStore.upsertConnection({
    id,
    name: form.name,
    type: form.type,
    host: form.host,
    port: Number(form.port),
    user: form.user,
    password: form.password,
    database: form.database,
    db: Number(form.db),
    ssh: form.useSsh ? buildSsh() : null
  });
  ui.closeModal('connectionModal');
  ui.showToast('连接已保存');
}
</script>

<template>
  <div class="modal" :class="{ hidden: !ui.modals.connectionModal }">
    <div class="modal-content" style="width: 500px;">
      <div class="modal-scroll">
      <div class="modal-header">
        <h3>{{ ui.connModalTarget ? '编辑连接' : '新建连接' }}</h3>
        <button class="close-btn" @click="ui.closeModal('connectionModal')">&times;</button>
      </div>
      <form @submit.prevent="onSave">
        <div class="form-group">
          <label>连接名称</label>
          <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.name" required>
        </div>
        <div class="form-group">
          <label>数据库类型</label>
          <IconSelect
            :model-value="form.type"
            :options="DB_TYPES"
            @update:model-value="onTypeChange"
          />
        </div>
        <div class="form-row" v-show="!isFileBased(form.type)">
          <div class="form-group">
            <label>主机</label>
            <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.host" :required="!isFileBased(form.type)" placeholder="localhost">
          </div>
          <div class="form-group">
            <label>端口</label>
            <input type="number" v-model.number="form.port" :required="!isFileBased(form.type)">
          </div>
        </div>
        <div class="form-row" v-show="!isFileBased(form.type)">
          <div class="form-group">
            <label>用户名</label>
            <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.user"
              :required="needsAuth(form.type)" :placeholder="needsAuth(form.type) ? '如：root' : '可留空'">
          </div>
          <div class="form-group">
            <label>密码</label>
            <input type="password" v-model="form.password">
          </div>
        </div>
        <div class="form-group" v-show="isRelational(form.type)">
          <label>{{ dbFieldLabel(form.type) }}</label>
          <div style="display: flex; gap: 8px;">
            <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.database"
              :placeholder="isFileBased(form.type) ? '点击右侧选择 .db / .sqlite 文件，或直接填写路径' : '可选'" style="flex: 1;">
            <button v-if="isFileBased(form.type)" type="button" class="btn btn-secondary" @click="pickSqliteFile">浏览…</button>
          </div>
        </div>
        <div class="form-group" v-show="form.type === 'redis'">
          <label>DB索引</label>
          <input type="number" v-model.number="form.db">
        </div>
        <div class="form-group" v-show="!isFileBased(form.type)">
          <label style="display: flex; align-items: center; gap: 8px;">
            <input type="checkbox" v-model="form.useSsh">
            使用SSH隧道
          </label>
        </div>
        <div v-show="form.useSsh && !isFileBased(form.type)">
          <div class="form-row">
            <div class="form-group">
              <label>SSH主机</label>
              <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.sshHost">
            </div>
            <div class="form-group">
              <label>SSH端口</label>
              <input type="number" v-model.number="form.sshPort">
            </div>
          </div>
          <div class="form-row">
            <div class="form-group">
              <label>SSH用户名</label>
              <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.sshUser">
            </div>
            <div class="form-group">
              <label>SSH密码</label>
              <input type="password" v-model="form.sshPassword">
            </div>
          </div>
          <div class="form-group">
            <label>SSH私钥（可选）</label>
            <textarea autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.sshKey" rows="4"></textarea>
          </div>
        </div>
        <div style="display: flex; gap: 10px; justify-content: flex-end;">
          <button type="button" class="btn btn-secondary" @click="ui.closeModal('connectionModal')">取消</button>
          <button type="button" class="btn" @click="onTest">测试连接</button>
          <button type="submit" class="btn">保存</button>
        </div>
      </form>
      </div>
    </div>
  </div>
</template>
