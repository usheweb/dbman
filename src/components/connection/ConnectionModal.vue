<script setup>
import { reactive, ref, watch } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import { isRelational, isFileBased, isMongoLike } from '@/utils/dbType';
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
  { value: 'redis', label: 'Redis', icon: 'redis' },
  { value: 'mongodb', label: 'MongoDB', icon: 'mongodb' }
];

// 各类型的默认端口：MySQL / MariaDB 3306，PostgreSQL 5432，
// SQL Server 1433，Oracle 1521，Redis 6379，MongoDB 27017
const DEFAULT_PORTS = {
  mysql: 3306,
  mariadb: 3306,
  postgres: 5432,
  sqlserver: 1433,
  oracle: 1521,
  redis: 6379,
  mongodb: 27017
};

// 需要用户名/密码认证的关系型库（SQLite 为本地文件，无认证）
function needsAuth(type) {
  return isRelational(type) && !isFileBased(type);
}

// 各类型常见的管理员账号：用户名提示必须随类型变化，写死 MySQL/root 会让选了
// MariaDB / PostgreSQL / SQL Server / Oracle 的用户以为填错地方了
const DEFAULT_USERS = {
  mysql: 'root',
  mariadb: 'root',
  postgres: 'postgres',
  postgresql: 'postgres',
  sqlserver: 'sa',
  mssql: 'sa',
  oracle: 'system'
};

function typeLabel(type) {
  const found = DB_TYPES.find(t => t.value === type);
  return found ? found.label : type;
}

// 用户名输入框的占位提示
function userPlaceholder(type) {
  const example = DEFAULT_USERS[type];
  return example ? `如：${example}` : '可留空';
}

// MongoDB 的 config.database 传的是「认证库」（authSource），与关系型的「数据库」同字段
function hasDbField(type) {
  return isRelational(type) || isMongoLike(type);
}

// 「数据库」字段的标签随类型变化：SQLite 填文件路径，Oracle 填服务名，MongoDB 填认证库
function dbFieldLabel(type) {
  if (isFileBased(type)) return '数据库文件';
  if (type === 'oracle') return '服务名(Service Name)';
  if (isMongoLike(type)) return '认证库(Auth Source)';
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

// 已保存的秘密在这里用占位符表示。编辑连接时刻意不去读系统钥匙串（读别人的条目会弹授权框），
// 只按连接上的 secretFields 标记显示占位符；保存时若占位符原样保留，就完全不碰钥匙串
const SECRET_KEEP = '••••••••••';
const SECRET_KEYS = ['password', 'sshPassword', 'sshKey'];

// 用编辑目标（或新建时的默认值）填充表单。
// 密码 / SSH 密码 / SSH 私钥不随连接对象落盘；已进钥匙串的用占位符提示，钥匙串不可用时是明文残留
function fillForm() {
  const conn = ui.connModalTarget;
  form.name = conn ? conn.name : '';
  form.type = conn ? conn.type : 'mysql';
  // 新建时默认 localhost：输入框的占位提示看起来就像已填好，实际是空值，
  // 用户直接点「测试连接」会拿到驱动的英文报错（如 MongoDB 的 the hostname cannot be empty）
  form.host = conn ? conn.host : 'localhost';
  form.port = conn && conn.port ? conn.port : defaultPort(form.type);
  form.user = conn ? conn.user : '';
  form.database = conn ? (conn.database || '') : '';
  form.db = conn ? (conn.db || 0) : 0;
  form.useSsh = conn ? !!conn.ssh : false;
  form.sshHost = (conn && conn.ssh && conn.ssh.host) || '';
  form.sshPort = (conn && conn.ssh && conn.ssh.port) || 22;
  form.sshUser = (conn && conn.ssh && conn.ssh.username) || '';
  // 秘密字段：降级模式下连接里还留着明文，直接回填真值；反之按标记填占位符
  const fields = (conn && conn.secretFields) || [];
  const keep = name => (fields.includes(name) ? SECRET_KEEP : '');
  form.password = (conn && conn.password) || keep('password');
  form.sshPassword = (conn && conn.ssh && conn.ssh.password) || keep('sshPassword');
  form.sshKey = (conn && conn.ssh && conn.ssh.privateKey) || keep('sshKey');
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

// 表单三处秘密的原始值；SSH 关掉时两个 SSH 字段视为「清空」
function rawSecrets() {
  return {
    password: form.password,
    sshPassword: form.useSsh ? form.sshPassword : '',
    sshKey: form.useSsh ? form.sshKey : ''
  };
}

// 解析秘密字段：占位符表示「沿用已保存的密码」，此时才去钥匙串把旧值取回来
async function resolveSecrets(raw) {
  const prev = ui.connModalTarget;
  const needOld = prev && SECRET_KEYS.some(k => raw[k] === SECRET_KEEP);
  const old = needOld ? await connStore.loadSecretsFor(prev.id) : {};
  const out = {};
  for (const k of SECRET_KEYS) out[k] = raw[k] === SECRET_KEEP ? (old[k] || '') : raw[k];
  return out;
}

function sshConfig(sec) {
  return {
    enabled: true,
    host: form.sshHost,
    port: Number(form.sshPort),
    username: form.sshUser,
    password: sec.sshPassword,
    privateKey: sec.sshKey
  };
}

// 与 getConnectionConfig 完全一致：关系型用 database，redis 用 db
// （MongoDB 的 database 表示认证库，与关系型同字段）
// sec 为解析后的秘密值；不传时直接用表单里的值（新建连接、没有占位符时）
function buildPayload(sec) {
  const s = sec || { password: form.password, sshPassword: form.sshPassword, sshKey: form.sshKey };
  const config = {
    host: form.host,
    port: Number(form.port),
    user: form.user,
    password: s.password
  };
  if (hasDbField(form.type)) config.database = form.database;
  else config.db = Number(form.db);
  return {
    type: form.type,
    config,
    ssh: form.useSsh ? sshConfig(s) : { enabled: false }
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

// 校验顺序：先主机再用户名。「测试连接」按钮不走表单提交，HTML 的 required 拦不住它
function validate() {
  if (!isFileBased(form.type) && !form.host.trim()) {
    ui.showToast('请填写主机地址（本机数据库填 localhost 或 127.0.0.1）', 'error');
    return false;
  }
  // 用户名必填针对需要认证的关系型库：留空时会按匿名账号登录，只能看到极少数系统库。
  // SQLite 为本地文件，Redis 走密码认证（后端固定不带用户名），所以不校验用户名。
  if (needsAuth(form.type) && !form.user.trim()) {
    const example = DEFAULT_USERS[form.type];
    ui.showToast(
      `请填写用户名（${typeLabel(form.type)} 一般填 ${example || '管理员账号'}）`,
      'error'
    );
    return false;
  }
  return true;
}

// 测试连接可能等好几秒（超时 8 秒）：期间按钮显示旋转图标与已耗时秒数，
// 免得用户以为没反应而反复点
const testing = ref(false);
const testElapsed = ref(0);
let testTimer = null;

async function onTest() {
  if (testing.value) return; // 连点会重复发起连接
  if (!validate()) return;
  testing.value = true;
  testElapsed.value = 0;
  const startedAt = Date.now();
  testTimer = setInterval(() => {
    testElapsed.value = Math.floor((Date.now() - startedAt) / 1000);
  }, 250);
  try {
    // 占位符不是真密码：测试前得先把「沿用」的字段还原成钥匙串里的旧值
    await connStore.testConnection(buildPayload(await resolveSecrets(rawSecrets())));
    ui.showToast('连接成功！', 'success');
  } catch (err) {
    ui.showToast('连接失败：' + String(err), 'error');
  } finally {
    clearInterval(testTimer);
    testTimer = null;
    testing.value = false;
  }
}

async function onSave() {
  if (!validate()) return;
  const prev = ui.connModalTarget;
  const id = prev ? prev.id : connStore.newConnectionId();
  const raw = rawSecrets();

  // 三处秘密全是占位符：用户根本没动秘密字段 → 不读也不写钥匙串，原样保留
  const untouched = !!prev && SECRET_KEYS.every(k => raw[k] === SECRET_KEEP);

  let inKeyring = true;                                       // 秘密是否在钥匙串里（false = 降级为明文落盘）
  let secretFields = (prev && prev.secretFields) || [];
  let plain = null;                                           // 降级模式下要落盘的明文

  if (!untouched) {
    const old = prev ? await connStore.loadSecretsFor(id) : {};
    const next = {};
    for (const k of SECRET_KEYS) next[k] = raw[k] === SECRET_KEEP ? (old[k] || '') : raw[k];
    if (prev && SECRET_KEYS.every(k => (old[k] || '') === next[k])) {
      // 结果与钥匙串里已有的完全一致：不覆盖写（覆盖已有条目会弹系统授权框）
      secretFields = SECRET_KEYS.filter(k => next[k]);
    } else if (await connStore.saveSecretsFor(id, next)) {
      // 密码 / SSH 密码 / SSH 私钥写进系统钥匙串，落盘的连接里不带秘密
      secretFields = SECRET_KEYS.filter(k => next[k]);
    } else {
      // 钥匙串不可用（如 debug 构建）：降级为明文落盘，功能不受影响
      inKeyring = false;
      secretFields = [];
      plain = next;
    }
  }

  connStore.upsertConnection({
    id,
    name: form.name,
    type: form.type,
    host: form.host,
    port: Number(form.port),
    user: form.user,
    password: plain ? plain.password : undefined,
    database: form.database,
    db: Number(form.db),
    ssh: form.useSsh
      ? {
          enabled: true,
          host: form.sshHost,
          port: Number(form.sshPort),
          username: form.sshUser,
          password: plain ? plain.sshPassword : undefined,
          privateKey: plain ? plain.sshKey : undefined
        }
      : null,
    secretFields
  });
  ui.closeModal('connectionModal');
  ui.showToast(inKeyring && secretFields.length ? '连接已保存，密码已存入系统钥匙串' : '连接已保存');
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
              :required="needsAuth(form.type)" :placeholder="userPlaceholder(form.type)">
          </div>
          <div class="form-group">
            <label>密码</label>
            <input type="password" v-model="form.password">
            <div v-if="form.password === SECRET_KEEP" class="secret-hint">已保存密码（圆点为占位）：保持不变＝沿用原密码，清空＝删除密码</div>
          </div>
        </div>
        <div class="form-group" v-show="hasDbField(form.type)">
          <label>{{ dbFieldLabel(form.type) }}</label>
          <div style="display: flex; gap: 8px;">
            <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.database"
              :placeholder="isFileBased(form.type) ? '点击右侧选择 .db / .sqlite 文件，或直接填写路径' : (isMongoLike(form.type) ? '留空默认 admin' : '可选')" style="flex: 1;">
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
              <div v-if="form.sshPassword === SECRET_KEEP" class="secret-hint">已保存密码（圆点为占位）：保持不变＝沿用原密码，清空＝删除密码</div>
            </div>
          </div>
          <div class="form-group">
            <label>SSH私钥（可选）</label>
            <textarea autocapitalize="off" autocorrect="off" spellcheck="false" v-model="form.sshKey" rows="4"></textarea>
            <div v-if="form.sshKey === SECRET_KEEP" class="secret-hint">已保存私钥（圆点为占位）：保持不变＝沿用原私钥，清空＝删除私钥</div>
          </div>
        </div>
        <div style="display: flex; gap: 10px; justify-content: flex-end;">
          <button type="button" class="btn btn-secondary" @click="ui.closeModal('connectionModal')">取消</button>
          <button
            type="button"
            class="btn"
            :class="{ 'btn-loading': testing }"
            :disabled="testing"
            @click="onTest"
          >
            <!-- 进行中：图标转圈 + 已耗时秒数（1 秒、2 秒…），让用户知道在连而不是没反应 -->
            <svg
              v-if="testing"
              class="icon-spin"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              aria-hidden="true"
            >
              <circle
                cx="12"
                cy="12"
                r="9"
                fill="none"
                stroke="currentColor"
                stroke-width="2.4"
                stroke-linecap="round"
                stroke-dasharray="42 15"
              />
            </svg>
            {{ testing ? `连接中… ${testElapsed} 秒` : '测试连接' }}
          </button>
          <button type="submit" class="btn" :disabled="testing">保存</button>
        </div>
      </form>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 「已保存密码」提示：用绿色 + 前置圆点，比灰色小字更显眼，避免用户误以为密码丢了又重新填一遍 */
.secret-hint {
  margin-top: 6px;
  font-size: 12px;
  line-height: 1.5;
  color: var(--green);
}

.secret-hint::before {
  content: '';
  display: inline-block;
  width: 6px;
  height: 6px;
  margin-right: 6px;
  border-radius: 50%;
  background: currentColor;
  vertical-align: 2px;
}
</style>
