// 连接 + 库表结构：连接列表、当前连接句柄、数据库列表、表列表、列信息
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
  const currentConnId = ref(null);
  const serverConnId = ref(null);
  const currentConnData = ref(null);
  const currentDatabase = ref(null);
  const currentDatabaseList = ref([]);
  const currentTable = ref(null);

  // 表结构缓存
  const allTables = ref([]);
  const tableColumns = ref({});
  const triedColumns = ref({});

  const isConnected = computed(() => !!serverConnId.value && !!currentConnData.value);

  // 并发请求共用同一次重连过程，避免连出多条连接
  let reconnecting = null;

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
  }

  function removeConnection(connId) {
    connections.value = connections.value.filter(c => c.id !== connId);
    saveConnections();
    if (currentConnId.value === connId) disconnect();
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

  // 用已保存的连接配置静默重连一次；用户不需要感知这次自愈
  async function reconnectServer() {
    if (reconnecting) return reconnecting;
    const conn = currentConnData.value;
    if (!conn || !conn.type) return false;
    reconnecting = (async () => {
      try {
        const connId = await api.connect(buildConnectPayload(conn));
        serverConnId.value = connId;
        return true;
      } catch (e) {
        return false;
      }
    })();
    try {
      return await reconnecting;
    } finally {
      reconnecting = null;
    }
  }

  // 所有需要 connId 的请求统一走这里：遇到「句柄失效」就重连并重试一次
  // fn 收到当前 serverConnId，返回一个 Promise
  async function withRetry(fn, retried) {
    try {
      return await fn(serverConnId.value);
    } catch (err) {
      const msg = String(err);
      if (msg === INVALID_CONN) {
        if (!retried && await reconnectServer()) return withRetry(fn, true);
        throw new Error('连接已失效，请重新选择连接');
      }
      throw new Error(msg);
    }
  }

  async function selectConnection(connId) {
    if (currentConnId.value === connId) return;
    if (currentConnId.value) await disconnect();
    const conn = connections.value.find(c => c.id === connId);
    if (!conn) return;
    currentConnId.value = connId;
    currentConnData.value = conn;
    try {
      serverConnId.value = await api.connect(buildConnectPayload(conn));
      ui.showToast('连接成功', 'success');
      if (conn.type === 'mysql') {
        await loadMySQLDatabases();
      }
    } catch (err) {
      ui.showToast('连接失败：' + String(err), 'error');
      currentConnId.value = null;
      serverConnId.value = null;
      currentConnData.value = null;
      throw err;
    }
  }

  // 测试连接：连上后立刻断开，只验证可用性
  async function testConnection(payload) {
    const connId = await api.connect(payload);
    try {
      await api.disconnect(connId);
    } catch (e) { /* 断开失败不影响测试结果 */ }
  }

  async function disconnect() {
    if (serverConnId.value) {
      try {
        await api.disconnect(serverConnId.value);
      } catch (e) { /* 忽略 */ }
    }
    currentConnId.value = null;
    serverConnId.value = null;
    currentConnData.value = null;
    currentDatabase.value = null;
    currentDatabaseList.value = [];
    currentTable.value = null;
    allTables.value = [];
    tableColumns.value = {};
    triedColumns.value = {};
  }

  // 用户主动断开：断开后把连接列展开，方便重新选连接
  async function disconnectFromUi() {
    await disconnect();
    ui.setPanel('sidebar', false);
    ui.showToast('已断开连接', 'success');
  }

  async function loadMySQLDatabases() {
    const data = await withRetry(id => api.mysqlDatabases(id));
    currentDatabaseList.value = data;
    const last = connectionDatabases.value[currentConnId.value];
    if (last && data.includes(last)) {
      await selectDatabase(last);
    } else if (data.length > 0) {
      await selectDatabase(data[0]);
    }
  }

  async function selectDatabase(db) {
    const changed = db !== currentDatabase.value;
    currentDatabase.value = db;
    currentTable.value = null;
    saveConnectionDatabase(currentConnId.value, db);
    await loadTables(db);
    // 右边表列表出来了，自动收起连接列腾出空间
    if (changed) ui.setPanel('sidebar', true);
  }

  async function loadTables(db) {
    const data = await withRetry(id => api.mysqlTables(id, db, null));
    allTables.value = data;
  }

  // 只拉列信息（补全引擎与结果表头注释都要用），错误吞掉由调用方决定
  async function loadColumns(db, table) {
    try {
      const data = await withRetry(id => api.mysqlColumns(id, db, table));
      tableColumns.value = { ...tableColumns.value, [table]: data };
    } catch (e) {
      /* 拿不到列信息不阻塞主流程 */
    }
  }

  return {
    connections,
    connectionDatabases,
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
    buildConnectPayload,
    reconnectServer,
    withRetry,
    selectConnection,
    testConnection,
    disconnect,
    disconnectFromUi,
    loadMySQLDatabases,
    selectDatabase,
    loadTables,
    loadColumns
  };
});
