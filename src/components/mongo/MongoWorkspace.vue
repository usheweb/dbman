<script setup>
// MongoDB 右侧工作区：集合查询条件编辑 + 结果区。
// 查询结果区完全复用关系型的 ResultArea（workspace store 已按连接类型把 fetchRows 分流到
// mongo_query），所以分页、查找、全屏、历史结果、导出这些能力对 MongoDB 一并可用。
import { computed, nextTick, ref, watch } from 'vue';
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

// 「{ "_id": 0 }」这类写法既合法也常见；只用 JSON 对象，其余（数组、null）都当作不合法。
// 空文本按 {} 处理（三个框都「留空 = 不过滤 / 不排序 / 取全部字段」）
function parseJsonObj(text) {
  const t = String(text == null ? '' : text).trim();
  if (!t) return {};
  try {
    const obj = JSON.parse(t);
    if (!obj || typeof obj !== 'object' || Array.isArray(obj)) return null;
    return obj;
  } catch {
    return null;
  }
}

function parseProjection() {
  return parseJsonObj(projectionText.value);
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

// 字段点选作用于哪个输入框：跟着「正在编辑的那个框」走 —— 点进哪个框就作用于哪个，
// 焦点离开（比如去点字段标签）后仍保留上次编辑的框，默认是字段筛选
const chipTarget = ref('projection');
const TARGET_LABEL = {
  filter: '过滤条件 filter',
  sort: '排序 sort',
  projection: '字段筛选 projection'
};
function setChipTarget(value) {
  chipTarget.value = value;
}
const CHIP_BAD_TIP = '当前输入框不是合法 JSON，先修正后再点选';

// 当前作用框的 JSON 写坏了：不按用户输入猜，直接停用点选，避免覆盖用户正写的内容
const targetInvalid = computed(() => {
  if (chipTarget.value === 'filter') return parseJsonObj(filterText.value) === null;
  if (chipTarget.value === 'sort') return parseJsonObj(sortText.value) === null;
  return parseProjection() === null;
});

// 每个字段当前是「显示」还是「隐藏」（projection）。留空 = 取全部字段，即全显示；
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

// 每个字段标签的样子与提示语，随当前作用框（过滤 / 排序 / 筛选）而变
const chipInfos = computed(() => {
  const bad = targetInvalid.value;
  const proj = parseProjection();
  const filt = parseJsonObj(filterText.value);
  const sort = parseJsonObj(sortText.value);
  const target = chipTarget.value;
  return fieldChips.value.map(name => {
    if (target === 'filter') {
      const on = !bad && !!filt && name in filt;
      return {
        name,
        cls: { on, dead: bad },
        suffix: '',
        title: bad ? CHIP_BAD_TIP
          : on ? `点一下把 ${name} 从过滤条件里去掉`
            : `点一下把 ${name} 加进过滤条件，接着填值`
      };
    }
    if (target === 'sort') {
      const dir = (bad || !sort) ? 0 : (Number(sort[name]) || 0);
      return {
        name,
        cls: { on: dir !== 0, desc: dir === -1, dead: bad },
        suffix: dir === 1 ? ' ↑' : dir === -1 ? ' ↓' : '',
        title: bad ? CHIP_BAD_TIP
          : dir === 0 ? `点一下按 ${name} 升序`
            : dir === 1 ? `点一下改为 ${name} 降序`
              : `点一下取消 ${name} 排序`
      };
    }
    const on = !bad && !!fieldShown.value[name];
    return {
      name,
      cls: { on, off: !on, dead: bad },
      suffix: '',
      title: bad ? CHIP_BAD_TIP : `点一下${on ? '隐藏' : '显示'}字段 ${name}`
    };
  });
});

// 点字段标签：按当前作用框执行对应的操作
function onChipClick(name) {
  if (targetInvalid.value) return;
  if (chipTarget.value === 'filter') return addFilterField(name);
  if (chipTarget.value === 'sort') return toggleSortField(name);
  return toggleProjField(name);
}

// 筛选：点一下切换显示/隐藏，写回的是合法写法 —— 按「哪边少写哪边」挑，
// 且不把 1 与 0 混在一起（Mongo 只允许 _id 例外）
function toggleProjField(field) {
  const proj = parseProjection();
  if (!proj) return;
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
    if (!shown.has('_id')) next._id = 0; // 只显示列出的字段时 _id 默认跟着来，不要就显式排除
  } else {
    rest.forEach(f => { next[f] = 0; });
  }
  projectionText.value = JSON.stringify(next);
}

// 排序：未指定 → 升序 → 降序 → 取消；多字段按点击先后排在后面（JSON 顺序即排序优先级）
function toggleSortField(field) {
  const sort = parseJsonObj(sortText.value);
  if (!sort) return;
  const cur = Number(sort[field]) || 0;
  if (cur === 1) sort[field] = -1;
  else if (cur === -1) delete sort[field];
  else sort[field] = 1;
  sortText.value = Object.keys(sort).length ? JSON.stringify(sort) : '';
}

// 过滤：点一下把字段名写进条件（值留空、光标落在值上，只填值不用打字段名），再点一下移出
function addFilterField(field) {
  const filt = parseJsonObj(filterText.value);
  if (!filt) return;
  if (field in filt) {
    delete filt[field];
    filterText.value = Object.keys(filt).length ? JSON.stringify(filt) : '';
    return;
  }
  filt[field] = '';
  filterText.value = JSON.stringify(filt);
  focusFilterValue(field);
}

// 把光标放进刚插入字段的空值引号中间，用户直接敲值即可
async function focusFilterValue(field) {
  await nextTick();
  const el = filterEl.value;
  if (!el) return;
  const at = el.value.indexOf(`"${field}":""`);
  if (at < 0) return;
  const pos = at + field.length + 4;
  el.focus();
  el.setSelectionRange(pos, pos);
}

const filterEl = ref(null);
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
            <label class="mongo-field" :class="{ 'field-active': chipTarget === 'filter' }">
              <span class="mongo-field-label">过滤条件 filter</span>
              <textarea ref="filterEl" v-model="filterText" rows="2" spellcheck="false" autocapitalize="off" autocorrect="off" placeholder='{"status":"active"}' @focus="setChipTarget('filter')"></textarea>
            </label>
            <label class="mongo-field" :class="{ 'field-active': chipTarget === 'sort' }">
              <span class="mongo-field-label">排序 sort</span>
              <textarea v-model="sortText" rows="2" spellcheck="false" autocapitalize="off" autocorrect="off" placeholder='{"_id":1} 留空不排序' @focus="setChipTarget('sort')"></textarea>
            </label>
            <label class="mongo-field" :class="{ 'field-active': chipTarget === 'projection' }">
              <span class="mongo-field-label">字段筛选 projection</span>
              <textarea v-model="projectionText" rows="2" spellcheck="false" autocapitalize="off" autocorrect="off" placeholder='{"name":1,"_id":0} 留空取全部字段' @focus="setChipTarget('projection')"></textarea>
            </label>
          </div>
          <!-- 点字段名就能改三个框里的 JSON，不必手打字段名；写回的都是能直接跑的写法，也能反过来手改文本框 -->
          <div class="proj-bar">
            <span
              class="proj-bar-target"
              :title="`点字段名只改高亮的那个框：点进要编辑的输入框即可切换（当前是「${TARGET_LABEL[chipTarget]}」）`"
            >作用于 {{ TARGET_LABEL[chipTarget] }}</span>
            <template v-if="chipInfos.length">
              <button
                v-for="c in chipInfos"
                :key="c.name"
                type="button"
                class="proj-chip"
                :class="c.cls"
                :title="c.title"
                @click.stop.prevent="onChipClick(c.name)"
              >{{ c.name }}{{ c.suffix }}</button>
            </template>
            <span v-else class="proj-bar-hint">执行一次查询后，这里会列出结果字段，点一下即可用</span>
            <button
              v-if="chipTarget === 'projection' && projectionText.trim()"
              type="button"
              class="proj-chip proj-chip-all"
              title="清空字段筛选，恢复显示全部字段"
              @click.stop.prevent="projectionText = ''"
            >全部字段</button>
            <span v-if="targetInvalid" class="proj-bar-warn">当前输入框不是合法 JSON，点选已停用</span>
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

/* 作用框提示：不再摆一排可点的 tab，跟着输入框焦点自动切换 */
.proj-bar-target {
  flex: 0 0 auto;
  margin-right: 2px;
  color: var(--text-dim);
  font-size: 11px;
  letter-spacing: .5px;
}

/* 字段标签：青色=已启用，虚线灰=未启用；点一下切换，不用手写 JSON */
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

/* 排序方向：升序青、降序洋红，和结果表里可排序字段的配色一致 */
.proj-chip.desc {
  color: var(--magenta);
  border-color: rgba(var(--magenta-rgb), .45);
  background: rgba(var(--magenta-rgb), .12);
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

/* 点字段名会作用到的那个框：焦点离开后（去点字段标签）也保持高亮，
   免得不知道点一下到底改的是哪个输入框 */
.mongo-field.field-active .mongo-field-label {
  color: var(--cyan);
}

.mongo-field.field-active textarea {
  border-color: rgba(var(--cyan-rgb), .55);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .18);
}
</style>
