<script setup>
// MongoDB 右侧工作区：集合查询条件编辑 + 结果区。
// 查询结果区完全复用关系型的 ResultArea（workspace store 已按连接类型把 fetchRows 分流到
// mongo_query），所以分页、查找、全屏、历史结果、导出这些能力对 MongoDB 一并可用。
import { watch } from 'vue';
import { useConnectionStore } from '@/stores/connection';
import { useWorkspaceStore } from '@/stores/workspace';
import { useMongo, buildSpec } from '@/composables/useMongo';
import ItemIcon from '@/components/common/ItemIcon.vue';
import ResultArea from '@/components/workspace/ResultArea.vue';

const connStore = useConnectionStore();
const ws = useWorkspaceStore();
const { selectedCollection, mode, filterText, sortText, projectionText, pipelineText } = useMongo();

async function run() {
  if (!selectedCollection.value || !connStore.currentDatabase) return;
  await ws.executeSqlText(buildSpec());
}

// 选中集合就自动查一次，省去「选中 → 再点执行」的多余步骤（与表树点表的行为一致）
watch(selectedCollection, name => {
  if (!name) return;
  run();
});
</script>

<template>
  <div class="workspace mongo-workspace">
    <div v-if="!connStore.currentDatabase" class="mongo-empty">请先在左侧连接卡片中选择数据库</div>
    <div v-else-if="!selectedCollection" class="mongo-empty">请从左侧选择一个集合</div>
    <template v-else>
      <div class="mongo-query">
        <div class="mongo-query-head">
          <span class="mongo-target" :title="selectedCollection">
            <ItemIcon name="collection" />
            <span class="mongo-target-name">{{ selectedCollection }}</span>
          </span>
          <div class="mongo-mode">
            <button
              class="mongo-mode-btn"
              :class="{ active: mode === 'find' }"
              title="按过滤条件查询文档"
              @click="mode = 'find'"
            >find</button>
            <button
              class="mongo-mode-btn"
              :class="{ active: mode === 'aggregate' }"
              title="按聚合管道查询文档"
              @click="mode = 'aggregate'"
            >aggregate</button>
          </div>
          <span class="mongo-spacer"></span>
          <button class="btn" :disabled="ws.busy" @click="run">{{ ws.busy ? '执行中…' : '执行' }}</button>
        </div>
        <div v-if="mode === 'find'" class="mongo-fields">
          <label class="mongo-field">
            <span class="mongo-field-label">过滤条件 filter</span>
            <textarea v-model="filterText" rows="2" spellcheck="false" autocapitalize="off" autocorrect="off" placeholder='{"status":"active"}'></textarea>
          </label>
          <label class="mongo-field">
            <span class="mongo-field-label">排序 sort</span>
            <textarea v-model="sortText" rows="2" spellcheck="false" autocapitalize="off" autocorrect="off" placeholder='{"_id":1} 留空不排序'></textarea>
          </label>
          <label class="mongo-field">
            <span class="mongo-field-label">字段筛选 projection</span>
            <textarea v-model="projectionText" rows="2" spellcheck="false" autocapitalize="off" autocorrect="off" placeholder='{"name":1,"_id":0} 留空取全部字段'></textarea>
          </label>
        </div>
        <label v-else class="mongo-field mongo-field-wide">
          <span class="mongo-field-label">聚合管道 pipeline（JSON 数组）</span>
          <textarea v-model="pipelineText" rows="6" spellcheck="false" autocapitalize="off" autocorrect="off" placeholder='[{"$match":{}}]'></textarea>
        </label>
      </div>
      <ResultArea />
    </template>
  </div>
</template>

<style scoped>
.mongo-workspace {
  padding: 0;
}

.mongo-empty {
  padding: 20px;
  color: var(--text-dim);
  font-size: 13px;
}

/* 查询条件区：固定高度，不参与滚动，结果区占满剩余空间 */
.mongo-query {
  flex: 0 0 auto;
  padding: 12px 16px;
  border-bottom: 1px solid var(--line);
  background: rgba(var(--ink-4-rgb), .4);
}

.mongo-query-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
}

.mongo-target {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  color: var(--cyan);
  font-family: var(--font-mono);
  font-size: 13px;
}

.mongo-target-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 模式切换：find / aggregate，选中态用青色描边与文字 */
.mongo-mode {
  display: inline-flex;
  flex: 0 0 auto;
  border: 1px solid var(--line);
  border-radius: 6px;
  overflow: hidden;
}

.mongo-mode-btn {
  padding: 4px 12px;
  border: none;
  background: transparent;
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.6;
  cursor: pointer;
  transition: all .18s ease;
}

.mongo-mode-btn:hover {
  color: var(--text);
}

.mongo-mode-btn.active {
  color: var(--cyan);
  background: rgba(var(--cyan-rgb), .12);
}

.mongo-spacer {
  flex: 1;
}

.mongo-fields {
  display: flex;
  gap: 10px;
}

.mongo-field {
  display: flex;
  flex: 1;
  min-width: 0;
  flex-direction: column;
  gap: 4px;
}

.mongo-field-wide {
  flex: 0 0 auto;
}

.mongo-field-label {
  color: var(--text-dim);
  font-size: 11px;
  letter-spacing: .5px;
}

.mongo-field textarea {
  width: 100%;
  padding: 8px 10px;
  box-sizing: border-box;
  background: rgba(var(--void-rgb), .6);
  border: 1px solid var(--line);
  border-radius: 6px;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.5;
  resize: vertical;
  outline: none;
  transition: border-color .18s ease, box-shadow .18s ease;
}

.mongo-field textarea::placeholder {
  color: var(--text-dim);
}

.mongo-field textarea:focus {
  border-color: var(--cyan);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .25);
}
</style>
