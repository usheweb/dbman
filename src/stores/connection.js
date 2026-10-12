// 连接 + 库表结构：支持同时打开多条连接（每条一个会话，顶部 tab 切换）
// 对外仍暴露 currentXxx 系列字段（指向「当前活跃会话」），调用方无需改动
import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import * as api from '@/api';
import { useStoreStore } from './store';
import { useUiStore } from './ui';
import { generateId } from '@/utils/format';

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
  }

  function newConnectionId() {
    return generateId();
  }

  // 统一构造后端 connect 需要的 payload
  function buildConnectPayload(conn) {
    return {
      type: conn.type,
      config: {
        host: conn.host,
        port: conn.port,
        user: conn.user,
        password: conn.password,
        database: conn.database,
        db: conn.db
      },
      ssh: conn.ssh || { enabled: false }
    };
  }

  // 用会话已保存的连接配置静默重连一次；用户不需要感知这次自愈
  async function reconnectFor(s) {
    if (!s || !s.connData || !s.connData.type) return false;
    const running = reconnecting.get(s.connId);
    if (running) return running;
    const p = (async () => {
      try {
        const connId = await api.connect(buildConnectPayload(s.connData));
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
    sessions.value[connId] = newSession(conn);
    tabOrder.value.push(connId);
    activeId.value = connId;
    try {
      const sid = await api.connect(buildConnectPayload(conn));
      const s = sessions.value[connId];
      if (!s) {
        // 连接还在建立时用户已关掉该 tab
        try { await api.disconnect(sid); } catch (e) { /* 忽略 */ }
        return;
      }
      s.serverConnId = sid;
      ui.showToast('已连接：' + conn.name, 'success');
      if (conn.type === 'mysql') await loadMySQLDatabasesFor(connId);
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
    const data = await withRetryFor(s, id => api.mysqlDatabases(id));
    s.databaseList = data;
    // 只有「记住的上次选择」仍然存在时才自动打开它。
    // 该连接首次使用（或记忆中的库已被删除）时不做任何默认选中。
    const last = connectionDatabases.value[connId];
    if (last && data.includes(last)) await selectDatabaseFor(connId, last);
  }

  function loadMySQLDatabases() {
    return loadMySQLDatabasesFor(activeId.value);
  }

  async function selectDatabaseFor(connId, db) {
    const s = sessions.value[connId];
    if (!s) return;
    const changed = db !== s.database;
    s.database = db;
    s.table = null;
    saveConnectionDatabase(connId, db);
    await loadTablesFor(connId, db);
    // 右边表列表出来了，自动收起连接列腾出空间
    if (changed && activeId.value === connId) ui.setPanel('sidebar', true);
  }

  function selectDatabase(db) {
    return selectDatabaseFor(activeId.value, db);
  }

  async function loadTablesFor(connId, db) {
    const s = sessions.value[connId];
    if (!s) return;
    const data = await withRetryFor(s, id => api.mysqlTables(id, db, null));
    s.allTables = data;
  }

  function loadTables(db) {
    return loadTablesFor(activeId.value, db);
  }

  // 只拉列信息（补全引擎与结果表头注释都要用），错误吞掉由调用方决定
  async function loadColumnsFor(connId, db, table) {
    const s = sessions.value[connId];
    if (!s) return;
    try {
      const data = await withRetryFor(s, id => api.mysqlColumns(id, db, table));
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