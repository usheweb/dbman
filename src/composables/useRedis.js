// Redis 面板的跨组件共享状态（模块级单例，不走 Pinia）：
// 左侧 RedisTree 负责写入 keys / pattern / selectedKey，
// 右侧 RedisDetail 读取 selectedKey 去拉值，两边靠这里共享。
import { ref } from 'vue';
import * as api from '@/api';

const keys = ref([]);
const pattern = ref('*');
const selectedKey = ref(null);
const info = ref('');
const dbsize = ref(0);

export function useRedis() {
  return { keys, pattern, selectedKey, info, dbsize };
}

export async function loadRedisInfo(connStore) {
  const data = await connStore.withRetry(id => api.redisInfo(id));
  info.value = data.info;
  dbsize.value = data.dbsize;
}

export async function loadRedisKeys(connStore, pat) {
  const data = await connStore.withRetry(id => api.redisKeys(id, pat || '*'));
  keys.value = data || [];
}
