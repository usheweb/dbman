// MongoDB 面板的跨组件共享状态（模块级单例，不走 Pinia）：
// 左侧 MongoTree 负责写入 collections / selectedCollection，
// 右侧 MongoWorkspace 读取选中集合与查询条件，构造查询描述后交给 workspace store 执行。
import { ref } from 'vue';
import * as api from '@/api';

// 当前库的集合列表
const collections = ref([]);
// 左侧选中的集合
const selectedCollection = ref(null);

// 查询模式：find（过滤 / 排序 / 字段筛选）| aggregate（聚合管道）
const mode = ref('find');
const filterText = ref('{}');
const sortText = ref('');
const projectionText = ref('');
// 聚合管道默认给一段能直接跑通的示例，避免空数组执行后一片空白
const pipelineText = ref('[\n  { "$match": {} },\n  { "$limit": 50 }\n]');

// 请求令牌：连续切库时只认最后一次结果，避免旧结果覆盖新列表
let listToken = 0;

export function useMongo() {
  return {
    collections, selectedCollection,
    mode, filterText, sortText, projectionText, pipelineText
  };
}

// 切换连接时清空，避免上一条 MongoDB 的集合列表 / 选中项串台
export function resetMongo() {
  collections.value = [];
  selectedCollection.value = null;
  mode.value = 'find';
  filterText.value = '{}';
  sortText.value = '';
  projectionText.value = '';
  pipelineText.value = '[\n  { "$match": {} },\n  { "$limit": 50 }\n]';
}

// 拉取当前库的集合列表（切库 / 重连后都要重拉）
export async function loadMongoCollections(connStore) {
  const db = connStore.currentDatabase;
  if (!db) {
    collections.value = [];
    return;
  }
  const token = ++listToken;
  const list = await connStore.withRetry(id => api.mongoCollections(id, db));
  if (token !== listToken) return;
  collections.value = list;
}

// 构造后端 mongo_query / mongo_export 需要的查询描述。
// 条件是编辑器里的原文，交给后端解析——这样用户手写 JSON 的报错由后端统一给出
export function buildSpec() {
  const collection = selectedCollection.value;
  if (!collection) return '';
  if (mode.value === 'aggregate') {
    return JSON.stringify({ mode: 'aggregate', collection, pipeline: pipelineText.value });
  }
  const spec = { mode: 'find', collection };
  if (filterText.value.trim()) spec.filter = filterText.value;
  if (sortText.value.trim()) spec.sort = sortText.value;
  if (projectionText.value.trim()) spec.projection = projectionText.value;
  return JSON.stringify(spec);
}
