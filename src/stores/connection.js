// 连接 + 库表结构：支持同时打开多条连接（每条一个会话，顶部 tab 切换）
// 对外仍暴露 currentXxx 系列字段（指向「当前活跃会话」），调用方无需改动
import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import * as api from '@/api';
import { useStoreStore } from './store';
import { useUiStore } from './ui';
import { generateId } from '@/utils/format';
import { isRelational, isMongoLike } from '@/utils/dbType';

// 句柄失效的精确文案，与 Rust 后端保持一致，用于触发静默重连
const INVALID_CONN = 'Invalid connection';

export const useConnectionStore = defineStore('connection', () => {
  const store = useStoreStore();
  const ui = useUiStore();

  const connections = ref([]);
  const connectionDatabases = ref({});

  // 已打开的连接会话：{ [connId]: Session }
  const sessions = ref({});
  // 打开顺序，决定 tab 排列
  const tabOrder = ref([]);
  // 当前活跃的 tab（连接的 id）
  const activeId = ref(null);

  // 每条会话的静默重连 Promise，避免并发请求连出多条连接
  const reconnecting = new Map();

  function newSession(conn) {
    return {
      connId: conn.id,
      connData: conn,
      serverConnId: null,
      database: null,
      databaseList: [],
      table: null,
      allTables: [],
      // 首屏小批表已经渲染、全量还在回：对外显示「加载中」，避免表数从 20 跳到 N
      tablesLoading: false,
      tableColumns: {},
      triedColumns: {}
    };
  }

  const active = computed(() => (activeId.value ? sessions.value[activeId.value] : null));

  // ===== 对外兼容字段：都指向当前活跃会话 =====
  const currentConnId = computed(() => activeId.value);
  const currentConnData = computed(() => (active.value ? active.value.connData : null));
  const serverConnId = computed(() => (active.value ? active.value.serverConnId : null));
  const currentDatabaseList = computed(() => (active.value ? active.value.databaseList : []));
  const allTables = computed(() => (active.value ? active.value.allTables : []));
  const tablesLoading = computed(() => (active.value ? active.value.tablesLoading : false));
  // 返回会话内那个响应式对象，外部 `delete conn.tableColumns[x]` 可原地生效
  const tableColumns = computed(() => (active.value ? active.value.tableColumns : {}));

  const triedColumns = computed({
    get: () => (active.value ? active.value.triedColumns : {}),
    set: v => { if (active.value) active.value.triedColumns = v; }
  });
  const currentDatabase = computed({
    get: () => (active.value ? active.value.database : null),
    set: v => { if (active.value) active.value.database = v; }
  });
  const currentTable = computed({
    get: () => (active.value ? active.value.table : null),
    set: v => { if (active.value) active.value.table = v; }
  });

  const isConnected = computed(() => !!(currentConnData.value && serverConnId.value));

  // 供外部（workspace 快照清理等）观察当前打开了哪些连接
  const openIds = computed(() => tabOrder.value.filter(id => sessions.value[id]));

  // tab 条展示数据
  const tabs = computed(() => openIds.value.map(id => ({
    id,
    name: sessions.value[id].connData.name,
    type: sessions.value[id].connData.type,
    active: id === activeId.value
  })));

  function isOpen(connId) {
    return !!sessions.value[connId];
  }

  function init() {
    connections.value = store.get('dbman_connections') || [];
    connectionDatabases.value = store.get('dbman_connection_databases') || {};
  }

  function saveConnections() {
    store.set('dbman_connections', connections.value);
  }

  function saveConnectionDatabase(connId, database) {
    connectionDatabases.value[connId] = database;
    store.set('dbman_connection_databases', { ...connectionDatabases.value });
  }

  function upsertConnection(connData) {
    const idx = connections.value.findIndex(c => c.id === connData.id);
    if (idx >= 0) connections.value[idx] = connData;
    else connections.value.push(connData);
    saveConnections();
    // 该连接已打开时同步会话里的配置（改名/备注即时反映到 tab）
    const s = sessions.value[connData.id];
    if (s) s.connData = connData;
  }

  function removeConnection(connId) {
    if (sessions.value[connId]) closeTab(connId);
    connections.value = connections.value.filter(c => c.id !== connId);
    saveConnections();
    // 连接都删了，钥匙串里的密码一并清掉，避免留下游离条目
    delete secretsCache.value[connId];
    api.secretDelete(secretAccount(connId)).catch(() => { /* 本来没有或钥匙串不可用：忽略 */ });
  }

  // 旧版本把密码明文存在 connections.json 里：用户点这条连接时，才把明文搬进钥匙串并抹掉原文。
  // 放在连接时做（而不是启动时），是为了让「打开软件」不碰钥匙串、不弹系统授权框。
  // 钥匙串不可用时原样保留明文（降级），不影响使用
  async function migrateConnSecrets(conn) {
    const ssh = conn.ssh || {};
    const secrets = { password: conn.password, sshPassword: ssh.password, sshKey: ssh.privateKey };
    if (!secrets.password && !secrets.sshPassword && !secrets.sshKey) return;
    const ok = await saveSecretsFor(conn.id, secrets);
    if (!ok) return;
    delete conn.password;
    if (conn.ssh) { delete conn.ssh.password; delete conn.ssh.privateKey; }
    // 记下哪几处秘密已在钥匙串里：编辑弹窗据此显示「已保存」占位符，不必去读钥匙串
    conn.secretFields = Object.keys(secrets).filter(k => secrets[k]);
    saveConnections();
  }

  // 清除全部已保存的密码：钥匙串条目 + 降级模式下的明文残留。连接本身保留
  async function purgeSecrets() {
    let allOk = true;
    for (const conn of connections.value) {
      try {
        await api.secretDelete(secretAccount(conn.id));
      } catch (e) {
        allOk = false;
      }
      delete conn.password;
      if (conn.ssh) { delete conn.ssh.password; delete conn.ssh.privateKey; }
      delete conn.secretFields;
    }
    secretsCache.value = {};
    saveConnections();
    return allOk;
  }

  function newConnectionId() {
    return generateId();
  }

  // ===== 系统钥匙串里的秘密（密码 / SSH 密码 / SSH 私钥）=====
  // 每次连接都读一次钥匙串会反复触发系统授权框，所以按连接缓存在内存里
  const secretsCache = ref({}); // { [connId]: { password, sshPassword, sshKey } }

  function secretAccount(connId) {
    return `conn-${connId}`;
  }

  // 读一条连接的秘密；没存过或钥匙串不可用都返回空对象
  async function loadSecretsFor(connId) {
    if (secretsCache.value[connId]) return secretsCache.value[connId];
    let blob = {};
    try {
      const raw = await api.secretGet(secretAccount(connId));
      if (raw) blob = JSON.parse(raw) || {};
    } catch (e) { /* 钥匙串不可用：按「没存过」处理 */ }
    secretsCache.value[connId] = blob;
    return blob;
  }

  // 写一条连接的秘密；返回 false 表示钥匙串不可用，调用方需降级为明文落盘
  async function saveSecretsFor(connId, secrets) {
    const clean = {
      password: secrets.password || '',
      sshPassword: secrets.sshPassword || '',
      sshKey: secrets.sshKey || ''
    };
    try {
      if (clean.password || clean.sshPassword || clean.sshKey) {
        await api.secretSet(secretAccount(connId), JSON.stringify(clean));
      } else {
        await api.secretDelete(secretAccount(connId));
      }
    } catch (e) {
      delete secretsCache.value[connId];
      return false;
    }
    secretsCache.value[connId] = clean;
    return true;
  }

  // 统一构造后端 connect 需要的 payload：密码优先取钥匙串，
  // conn 上的 password / ssh.password / ssh.privateKey 只在钥匙串不可用的降级模式下才存在
  async function buildConnectPayload(conn) {
    const sec = await loadSecretsFor(conn.id);
    const ssh = conn.ssh;
    return {
      type: conn.type,
      config: {
        host: conn.host,
        port: conn.port,
        user: conn.user,
        password: sec.password || conn.password || '',
        database: conn.database,
        db: conn.db
      },
      ssh: ssh
        ? {
            ...ssh,
            password: sec.sshPassword || ssh.password || '',
            privateKey: sec.sshKey || ssh.privateKey || ''
          }
        : { enabled: false }
    };
  }

  // 用会话已保存的连接配置静默重连一次；用户不需要感知这次自愈
  async function reconnectFor(s) {
    if (!s || !s.connData || !s.connData.type) return false;
    const running = reconnecting.get(s.connId);
    if (running) return running;
    const p = (async () => {
      try {
        const connId = await api.connect(await buildConnectPayload(s.connData));
        // 重连期间会话可能已被关闭，避免写回已删除的会话
        if (sessions.value[s.connId]) s.serverConnId = connId;
        return true;
      } catch (e) {
        return false;
      }
    })();
    reconnecting.set(s.connId, p);
    try {
      return await p;
    } finally {
      reconnecting.delete(s.connId);
    }
  }

  // 指定会话的请求统一走这里：遇到「句柄失效」就重连并重试一次
  async function withRetryFor(s, fn, retried) {
    if (!s) throw new Error('请先创建并选择一个数据库连接');
    try {
      return await fn(s.serverConnId);
    } catch (err) {
      const msg = String(err);
      if (msg === INVALID_CONN) {
        if (!retried && await reconnectFor(s)) return withRetryFor(s, fn, true);
        throw new Error('连接已失效，请重新选择连接');
      }
      throw new Error(msg);
    }
  }

  // 对当前活跃会话的请求（保持旧签名）
  function withRetry(fn) {
    return withRetryFor(active.value, fn);
  }

  const reconnectServer = () => reconnectFor(active.value);

  // ===== 打开 / 切换 / 关闭 tab =====
  async function openConnection(connId) {
    // 已打开：直接切过去，不重复建连接
    if (sessions.value[connId]) {
      activeId.value = connId;
      return;
    }
    const conn = connections.value.find(c => c.id === connId);
    if (!conn) return;
    // 遗留的明文密码在这一刻才搬进钥匙串（启动时不动钥匙串，不弹授权框）
    await migrateConnSecrets(conn);
    sessions.value[connId] = newSession(conn);
    tabOrder.value.push(connId);
    activeId.value = connId;
    try {
      const sid = await api.connect(await buildConnectPayload(conn));
      const s = sessions.value[connId];
      if (!s) {
        // 连接还在建立时用户已关掉该 tab
        try { await api.disconnect(sid); } catch (e) { /* 忽略 */ }
        return;
      }
      s.serverConnId = sid;
      // 连接成功不再弹提示：左侧库表列表加载出来本身就是成功的信号
      if (isRelational(conn.type)) await loadMySQLDatabasesFor(connId);
      else if (conn.type === 'redis') await loadRedisDatabasesFor(connId);
      else if (isMongoLike(conn.type)) await loadMongoDatabasesFor(connId);
    } catch (err) {
      ui.showToast('连接失败：' + String(err), 'error');
      // 回滚这个 tab
      delete sessions.value[connId];
      tabOrder.value = tabOrder.value.filter(id => id !== connId);
      if (activeId.value === connId) {
        activeId.value = tabOrder.value[tabOrder.value.length - 1] || null;
      }
      throw err;
    }
  }

  // ConnectionItem 等旧调用点沿用的名字
  const selectConnection = openConnection;

  function activateTab(connId) {
    if (sessions.value[connId]) activeId.value = connId;
  }

  async function closeTab(connId) {
    if (!sessions.value[connId]) return;
    const wasActive = activeId.value === connId;
    const idx = tabOrder.value.indexOf(connId);
    const sid = sessions.value[connId].serverConnId;

    // 先摘掉会话与 tab，断开期间的异步回调不会再写回
    delete sessions.value[connId];
    tabOrder.value = tabOrder.value.filter(id => id !== connId);
    if (wasActive) {
      // 优先接右邻，其次左邻
      activeId.value = tabOrder.value[idx] || tabOrder.value[idx - 1] || null;
    }
    if (sid) {
      try { await api.disconnect(sid); } catch (e) { /* 忽略 */ }
    }
  }

  // 测试连接：连上后立刻断开，只验证可用性
  async function testConnection(payload) {
    const connId = await api.connect(payload);
    try {
      await api.disconnect(connId);
    } catch (e) { /* 断开失败不影响测试结果 */ }
  }

  // 断开当前活跃连接（等价于关掉它的 tab）
  async function disconnect() {
    if (activeId.value) await closeTab(activeId.value);
  }

  // ===== 库 / 表 / 列（都作用在指定会话上）=====
  async function loadMySQLDatabasesFor(connId) {
    const s = sessions.value[connId];
    if (!s) return;
    const data = await withRetryFor(s, id => api.dbDatabases(id));
    s.databaseList = data;
    // 只有「记住的上次选择」仍然存在时才自动打开它。
    // 该连接首次使用（或记忆中的库已被删除）时不做任何默认选中。
    const last = connectionDatabases.value[connId];
    if (last && data.includes(last)) await selectDatabaseFor(connId, last);
  }

  function loadMySQLDatabases() {
    return loadMySQLDatabasesFor(activeId.value);
  }

  // Redis 的逻辑库：按 CONFIG GET databases 拿到的数量列出 DB 0 ~ n-1。
  // 连接本身就是按配置里的 db 建立的，这里先把选中的库同步成「记忆里的库 / 配置里的库」
  async function loadRedisDatabasesFor(connId) {
    const s = sessions.value[connId];
    if (!s) return;
    let n = 16;
    try {
      n = await withRetryFor(s, id => api.redisDatabases(id));
    } catch (e) { n = 16; }
    const list = Array.from({ length: n }, (_, i) => String(i));
    s.databaseList = list;
    const last = connectionDatabases.value[connId];
    const want = list.includes(last) ? last : String(s.connData.db ?? 0);
    await selectDatabaseFor(connId, list.includes(want) ? want : '0');
  }

  // MongoDB 的「库」是真实库名：列出来后由左侧集合树按当前库拉集合。
  // 认证库（config.database）不参与这里的默认选中，只沿用用户上次选过的库
  async function loadMongoDatabasesFor(connId) {
    const s = sessions.value[connId];
    if (!s) return;
    const data = await withRetryFor(s, id => api.mongoDatabases(id));
    s.databaseList = data;
    const last = connectionDatabases.value[connId];
    if (last && data.includes(last)) await selectDatabaseFor(connId, last);
  }

  async function selectDatabaseFor(connId, db) {
    const s = sessions.value[connId];
    if (!s) return;
    s.database = db;
    s.table = null;
    saveConnectionDatabase(connId, db);
    // Redis 没有表结构：切库就是在该连接上执行 SELECT，key 列表由右侧面板重新拉
    if (s.connData.type === 'redis') {
      await withRetryFor(s, id => api.redisSelectDb(id, Number(db)));
      return;
    }
    // MongoDB 没有表结构：切库后集合列表由左侧集合树监听 currentDatabase 自行重拉
    if (isMongoLike(s.connData.type)) return;
    // 选中库后不再自动收起连接列，避免页面跳动突兀；是否隐藏交给用户点面板上的收起按钮
    await loadTablesFor(connId, db);
  }

  function selectDatabase(db) {
    return selectDatabaseFor(activeId.value, db);
  }

  // 表列表一次拉全量：表信息全部来自 information_schema.TABLES 的一条 SQL。
  // 以前会先拉 20 张预览再拉全量，但预览那条同样要扫一遍 information_schema，等于白多一次往返。
  // 递增序号防竞态：期间又切了库的话，旧请求的结果不再写回
  const tableLoadSeq = {};

  async function loadTablesFor(connId, db) {
    const s = sessions.value[connId];
    if (!s) return;
    const seq = (tableLoadSeq[connId] = (tableLoadSeq[connId] || 0) + 1);
    // 清掉上一个库的残留：全量拉取期间列表先空着、顶部显示「表列表加载中…」，
    // 否则旧库的表会一直挂在列表里，用户可能在加载期间点到别的库的表
    s.allTables = [];
    s.tablesLoading = true;
    try {
      const all = await withRetryFor(s, id => api.dbTables(id, db, null));
      if (tableLoadSeq[connId] !== seq) return;
      s.allTables = all;
    } finally {
      if (tableLoadSeq[connId] === seq) s.tablesLoading = false;
    }
  }

  function loadTables(db) {
    return loadTablesFor(activeId.value, db);
  }

  // 只拉列信息（补全引擎与结果表头注释都要用），错误吞掉由调用方决定
  async function loadColumnsFor(connId, db, table) {
    const s = sessions.value[connId];
    if (!s) return;
    try {
      const data = await withRetryFor(s, id => api.dbColumns(id, db, table));
      s.tableColumns = { ...s.tableColumns, [table]: data };
    } catch (e) {
      /* 拿不到列信息不阻塞主流程 */
    }
  }

  function loadColumns(db, table) {
    return loadColumnsFor(activeId.value, db, table);
  }

  return {
    connections,
    connectionDatabases,
    openIds,
    tabs,
    currentConnId,
    serverConnId,
    currentConnData,
    currentDatabase,
    currentDatabaseList,
    currentTable,
    allTables,
    tablesLoading,
    tableColumns,
    triedColumns,
    isConnected,
    init,
    saveConnections,
    saveConnectionDatabase,
    upsertConnection,
    removeConnection,
    newConnectionId,
    isOpen,
    loadSecretsFor,
    saveSecretsFor,
    migrateConnSecrets,
    purgeSecrets,
    buildConnectPayload,
    reconnectServer,
    withRetry,
    openConnection,
    selectConnection,
    activateTab,
    closeTab,
    testConnection,
    disconnect,
    loadMySQLDatabases,
    selectDatabase,
    loadTables,
    loadColumns
  };
});