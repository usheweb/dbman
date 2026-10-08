// 本地 JSON 存储（对应原 server.js 的 /api/store）
// 原来存 localStorage，有 5MB 限制；现在落盘到应用数据目录，这里只维护一份内存缓存。
import { defineStore } from 'pinia';
import { ref } from 'vue';
import * as api from '@/api';

export const STORE_KEYS = [
  'dbman_connections',
  'dbman_connection_databases',
  'dbman_page_size',
  'dbman_global_display',
  'dbman_panel_state',
  'dbman_export_opts',
  'dbman_sql_log',
  'dbman_sql_favorites',
  'dbman_ai_config'
];

export const useStoreStore = defineStore('store', () => {
  const cache = ref({});
  let loaded = false;

  // 服务端还没有这个 key 时，把 localStorage 里的旧数据搬过去；
  // 只有写盘成功才清掉旧键，读写失败时旧数据保持不动，不会丢
  async function migrateKey(key) {
    const raw = localStorage.getItem(key);
    if (raw === null) return null;
    let value;
    try {
      value = key === 'dbman_page_size' ? raw : JSON.parse(raw);
    } catch (e) {
      localStorage.removeItem(key);
      return null;
    }
    if (await set(key, value)) localStorage.removeItem(key);
    return value;
  }

  // 启动引导：先拉全量文件，缺的 key 走 localStorage 迁移
  async function load() {
    if (loaded) return;
    loaded = true;
    let data = {};
    try {
      data = (await api.storeLoad()) || {};
    } catch (e) {
      /* 读取失败：全部退回默认值 */
    }
    for (const key of STORE_KEYS) {
      const v = data[key];
      cache.value[key] = (v === null || v === undefined) ? await migrateKey(key) : v;
    }
  }

  function get(key) {
    return cache.value[key];
  }

  async function set(key, value) {
    cache.value[key] = value;
    try {
      return await api.storeSet(key, value);
    } catch (e) {
      return false;
    }
  }

  return { cache, load, get, set };
});
