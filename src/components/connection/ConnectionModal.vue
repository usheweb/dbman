<script setup>
import { reactive, watch } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import IconSelect from '@/components/common/IconSelect.vue';

const ui = useUiStore();
const connStore = useConnectionStore();

// 数据库类型选项（图标为自绘单色版，见 ItemIcon）
const DB_TYPES = [
  { value: 'mysql', label: 'MySQL', icon: 'mysql' },
  { value: 'redis', label: 'Redis', icon: 'redis' }
];

const form = reactive({
  name: '',
  type: 'mysql',
  host: '',
  port: 3306,
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
  form.port = conn ? conn.port : 3306;
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

// 切换类型时若端口为空，补默认端口（与原 toggleDBOptions 一致）
function onTypeChange(type) {
  form.type = type;
  if (!form.port) form.port = type === 'mysql' ? 3306 : 6379;
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

// 与 getConnectionConfig 完全一致：mysql 用 database，redis 用 db
function buildPayload() {
  const config = {
    host: form.host,
    port: Number(form.port),
    user: form.user,
    password: form.password
  };
  if (form.type === 'mysql') config.database = form.database;
  else config.db = Number(form.db);
  return {
    type: form.type,
    config,
    ssh: form.useSsh ? buildSsh() : { enabled: false }
  };
}

// 用户名必须填：留空时 MySQL 会按匿名账号登录，只能看到 information_schema 等极少数库
function validate() {
  if (!form.user.trim()) {
    ui.showToast('请填写用户名（MySQL 一般填 root）', 'error');
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
        <div class="form-row">
          <div class="form-group">
            <label>主机</label>
            <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.host" required placeholder="localhost">
          </div>
          <div class="form-group">
            <label>端口</label>
            <input type="number" v-model.number="form.port" required>
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label>用户名</label>
            <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.user" required placeholder="如：root">
          </div>
          <div class="form-group">
            <label>密码</label>
            <input type="password" v-model="form.password">
          </div>
        </div>
        <div class="form-group" v-show="form.type === 'mysql'">
          <label>数据库</label>
          <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.database" placeholder="可选">
        </div>
        <div class="form-group" v-show="form.type === 'redis'">
          <label>DB索引</label>
          <input type="number" v-model.number="form.db">
        </div>
        <div class="form-group">
          <label style="display: flex; align-items: center; gap: 8px;">
            <input type="checkbox" v-model="form.useSsh">
            使用SSH隧道
          </label>
        </div>
        <div v-show="form.useSsh">
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
</template>
