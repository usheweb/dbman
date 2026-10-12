// Redis 面板的跨组件共享状态（模块级单例，不走 Pinia）：
// 左侧 RedisTree 负责写入 keys / keyword / selectedKey，
// 右侧 RedisDetail 读取 selectedKey 去拉值，两边靠这里共享。
import { computed, ref } from 'vue';
import * as api from '@/api';

const keys = ref([]);
const keyword = ref('');          // 搜索框关键词，支持 * ? 通配
const globalSearch = ref(true);   // true=在全部 key 中搜索（服务端 SCAN）；false=只筛当前页
const selectedKey = ref(null);
const info = ref('');
const dbsize = ref(0);
// 当前选中的库序号与库总数（Redis 默认 16 个逻辑库）
const dbIndex = ref(0);
const dbCount = ref(16);
// key 列表分页：SCAN 游标式，cursor 是当前页的出发游标
const keyTotal = ref(0);
const pageIndex = ref(1);
const cursor = ref(0);
const nextCursor = ref(0);
// 已经走过的游标，用于「上一页」回到原位置
const cursorStack = ref([]);
// 请求令牌：连续搜索时只认最后一次结果，避免旧结果覆盖新列表
let keyReqToken = 0;
const hasPrev = computed(() => cursorStack.value.length > 0);
const hasNext = computed(() => nextCursor.value !== 0);

// 关键词 → Redis glob：带通配符就原样用，否则按「任意位置包含」处理
export function toGlob(kw) {
  const k = (kw || '').trim();
  if (!k) return '*';
  return /[*?]/.test(k) ? k : '*' + k + '*';
}

// 同一套 glob 语义在前端做匹配（本页筛选用）。
// 不加 i 标志：与 Redis 的 MATCH 一致，大小写敏感
export function globMatch(glob, name) {
  const body = glob
    .replace(/[.+^${}()|[\]\\]/g, '\\$&')
    .replace(/\*/g, '.*')
    .replace(/\?/g, '.');
  return new RegExp('^' + body + '$').test(name);
}

// 真正发给后端的 pattern：只有「全库」模式才带关键词，本页模式取全量页交给前端筛
function serverPattern() {
  return globalSearch.value ? toGlob(keyword.value) : '*';
}

export function useRedis() {
  return {
    keys, keyword, globalSearch, selectedKey, info, dbsize, dbIndex, dbCount,
    keyTotal, pageIndex, hasPrev, hasNext
  };
}

// 切换连接时清空，避免上一条 Redis 的 key 列表/选中项串台
export function resetRedis() {
  keys.value = [];
  keyword.value = '';
  selectedKey.value = null;
  info.value = '';
  dbsize.value = 0;
  dbIndex.value = 0;
  dbCount.value = 16;
  keyTotal.value = 0;
  pageIndex.value = 1;
  cursor.value = 0;
  nextCursor.value = 0;
  cursorStack.value = [];
}

export async function loadRedisInfo(connStore) {
  const data = await connStore.withRetry(id => api.redisInfo(id));
  info.value = data.info;
  dbsize.value = data.dbsize;
}

// 取一页并同步分页状态；from 为 0 表示从第一页开始。
// 返回 false 表示结果已被更新的请求取代，调用方不要再改页码。
async function fetchKeys(connStore, from) {
  const token = ++keyReqToken;
  const data = await connStore.withRetry(id => api.redisKeys(id, serverPattern(), from));
  if (token !== keyReqToken) return false;
  keys.value = data.keys || [];
  nextCursor.value = data.cursor || 0;
  keyTotal.value = data.total || 0;
  cursor.value = from;
  return true;
}

// 从第一页重新取：换关键词、换库、换连接都走这里
export async function loadRedisKeys(connStore) {
  cursorStack.value = [];
  pageIndex.value = 1;
  await fetchKeys(connStore, 0);
}

// 下一页：把当前游标压栈，再用后端给的游标继续扫
export async function redisKeysNextPage(connStore) {
  if (!nextCursor.value) return;
  const back = cursor.value;
  if (!await fetchKeys(connStore, nextCursor.value)) return;
  cursorStack.value.push(back);
  pageIndex.value += 1;
}

// 上一页：弹回上一个游标重新扫
export async function redisKeysPrevPage(connStore) {
  if (!cursorStack.value.length) return;
  const back = cursorStack.value[cursorStack.value.length - 1];
  if (!await fetchKeys(connStore, back)) return;
  cursorStack.value.pop();
  pageIndex.value = back === 0 ? 1 : pageIndex.value - 1;
}

// 库总数：拿不到（托管实例禁用 CONFIG）时沿用默认 16
export async function loadRedisDatabases(connStore) {
  try {
    const n = await connStore.withRetry(id => api.redisDatabases(id));
    if (n > 0) dbCount.value = n;
  } catch (e) {
    dbCount.value = 16;
  }
}

// 切换到指定库：后端在该连接上执行 SELECT
export async function selectRedisDb(connStore, db) {
  await connStore.withRetry(id => api.redisSelectDb(id, db));
  dbIndex.value = db;
}
