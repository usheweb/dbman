<script setup>
// MongoDB 右侧工作区：集合查询条件编辑 + 结果区。
// 查询结果区完全复用关系型的 ResultArea（workspace store 已按连接类型把 fetchRows 分流到
// mongo_query），所以分页、查找、全屏、历史结果、导出这些能力对 MongoDB 一并可用。
import { computed, watch } from 'vue';
import { useConnectionStore } from '@/stores/connection';
import { useWorkspaceStore } from '@/stores/workspace';
import { useMongo, buildSpec, rememberMongoFields, clearMongoFields } from '@/composables/useMongo';
import ItemIcon from '@/components/common/ItemIcon.vue';
import ResultArea from '@/components/workspace/ResultArea.vue';

const connStore = useConnectionStore();
const ws = useWorkspaceStore();
const { selectedCollection, mode, filterText, sortText, projectionText, pipelineText, knownFields } = useMongo();

async function run() {
  if (!selectedCollection.value || !connStore.currentDatabase) return;
  await ws.executeSqlText(buildSpec());
}

// 选中集合就自动查一次，省去「选中 → 再点执行」的多余步骤（与表树点表的行为一致）
watch(selectedCollection, name => {
  clearMongoFields(); // 换集合了，字段名字典重新攒
  if (!name) return;
  run();
});

// 每次出结果就把列名并进候选字段：字段筛选一旦生效，结果只剩被选中的列，
// 只靠「当前结果列」当候选的话就再也点不回被过滤掉的字段了
watch(() => ws.resultColumns, cols => rememberMongoFields(cols));

// ===== 字段筛选用「点字段名」代替手写 JSON =====

// 「{ "_id": 0 }」这类写法既合法也常见；只用 JSON 对象，其余（数组、null）都当作不合法
function parseProjection() {
  const text = projectionText.value.trim();
  if (!text) return {};
  try {
    const obj = JSON.parse(text);
    if (!obj || typeof obj !== 'object' || Array.isArray(obj)) return null;
    return obj;
  } catch {
    return null;
  }
}

// 候选字段：结果列 + 记住的字段 + projection 里已有的字段（用户手写过的也要能点）
const fieldChips = computed(() => {
  const names = [];
  const seen = new Set();
  const push = name => {
    if (name && !seen.has(name)) {
      seen.add(name);
      names.push(name);
    }
  };
  ws.resultColumns.forEach(push);
  knownFields.value.forEach(push);
  const proj = parseProjection();
  if (proj) Object.keys(proj).forEach(push);
  return names;
});

// 字段筛选的 JSON 写坏了：不按用户输入猜，直接停用点选，避免把手上正在写的 JSON 覆盖掉
const projInvalid = computed(() => parseProjection() === null);

// 每个字段当前是「显示」还是「隐藏」。projection 留空 = 取全部字段，即全显示；
// 写成 {"a":1,"b":1} 是「只显示列出的」；写成 {"a":0} 是「排除列出的」
const fieldShown = computed(() => {
  const proj = parseProjection();
  const out = {};
  if (!proj) {
    fieldChips.value.forEach(f => { out[f] = true; });
    return out;
  }
  const onlyListed = Object.keys(proj).some(k => k !== '_id' && Number(proj[k]) === 1);
  fieldChips.value.forEach(f => {
    out[f] = (f in proj) ? Number(proj[f]) === 1 : !onlyListed;
  });
  return out;
});

// 点字段名切换显示/隐藏，并把结果写回 projection 文本框。
// 生成时按「哪边少写哪边」挑写法，且不把 1 和 0 混在一起（Mongo 只允许 _id 例外），
// 所以点出来的 JSON 永远能跑
function toggleProjField(field) {
  if (projInvalid.value) return;
  const shown = new Set(fieldChips.value.filter(f => fieldShown.value[f]));
  if (shown.has(field)) shown.delete(field);
  else shown.add(field);
  const rest = fieldChips.value.filter(f => !shown.has(f));
  if (!rest.length) {
    projectionText.value = ''; // 全部显示 → 留空，即取全部字段
    return;
  }
  const next = {};
  if (shown.size && shown.size <= rest.length) {
    shown.forEach(f => { if (f !== '_id') next[f] = 1; });
    if (!shown.has('_id')) next._id = 0; // 只显示列出的字段时，_id 默认跟着来，不要就显式排除
  } else {
    rest.forEach(f => { next[f] = 0; });
  }
  projectionText.value = JSON.stringify(next);
}
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
        <div v-if="mode === 'find'" class="mongo-find">
          <div class="mongo-fields">
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
          <!-- 点字段名就能筛字段，不必手打 projection 的 JSON；写回的是合法写法，也能反过来编辑文本框 -->
          <div class="proj-bar">
            <span class="proj-bar-label">字段点选</span>
            <template v-if="fieldChips.length">
              <button
                v-for="f in fieldChips"
                :key="f"
                type="button"
                class="proj-chip"
                :class="{ on: fieldShown[f], off: !fieldShown[f], dead: projInvalid }"
                :title="projInvalid ? '字段筛选不是合法 JSON，先修正后再点选' : `点击${fieldShown[f] ? '隐藏' : '显示'}字段 ${f}`"
                @click.stop.prevent="toggleProjField(f)"
              >{{ f }}</button>
            </template>
            <span v-else class="proj-bar-hint">执行一次查询后，这里会列出结果字段，点一下即可筛选</span>
            <button
              v-if="projectionText.trim()"
              type="button"
              class="proj-chip proj-chip-all"
              title="清空字段筛选，恢复显示全部字段"
              @click.stop.prevent="projectionText = ''"
            >全部字段</button>
            <span v-if="projInvalid" class="proj-bar-warn">字段筛选不是合法 JSON，点选已停用</span>
          </div>
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

/* 条件区 + 字段点选条 */
.mongo-find {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.proj-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.proj-bar-label {
  margin-right: 2px;
  color: var(--text-dim);
  font-size: 11px;
  letter-spacing: .5px;
}

/* 字段标签：青色描边=显示，虚线灰=隐藏；点一下切换，不用手写 projection JSON */
.proj-chip {
  max-width: 180px;
  padding: 2px 9px;
  overflow: hidden;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: transparent;
  color: var(--text-dim);
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.7;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
  transition: all .18s ease;
}

.proj-chip:hover {
  color: var(--text);
  border-color: var(--text-dim);
}

.proj-chip.on {
  color: var(--cyan);
  border-color: rgba(var(--cyan-rgb), .45);
  background: rgba(var(--cyan-rgb), .12);
}

.proj-chip.off {
  border-style: dashed;
  opacity: .65;
  text-decoration: line-through;
}

.proj-chip.dead {
  opacity: .5;
  cursor: not-allowed;
}

.proj-chip-all {
  color: var(--text);
  border-color: var(--text-dim);
}

.proj-bar-hint,
.proj-bar-warn {
  font-size: 11px;
}

.proj-bar-hint {
  color: var(--text-dim);
}

.proj-bar-warn {
  color: var(--magenta);
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
