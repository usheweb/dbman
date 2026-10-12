<script setup>
// CSV 导入向导：选文件 → 编码/分隔符 → 列关联 → 导入选项 → 执行
import ModalShell from '@/components/common/ModalShell.vue';
import CsvColumnMapper from '@/components/database/CsvColumnMapper.vue';
import { useCsvImportStore } from '@/stores/csvImport';

const csv = useCsvImportStore();

const ENCODINGS = [
  { value: 'auto', label: '自动检测' },
  { value: 'utf-8', label: 'UTF-8' },
  { value: 'gb18030', label: 'GB18030 / GBK' }
];

const DELIMITERS = [
  { value: 'auto', label: '自动检测' },
  { value: ',', label: '逗号 ,' },
  { value: ';', label: '分号 ;' },
  { value: 'tab', label: '制表符 Tab' },
  { value: '|', label: '竖线 |' }
];

const fileName = () => (csv.path ? csv.path.split(/[\\/]/).pop() : '');
</script>

<template>
  <ModalShell :visible="csv.modal.visible" width="1100px" @close="csv.close()">
    <template #title>导入 CSV 数据 · {{ csv.modal.table }}</template>

    <div class="ci-row">
      <span class="ci-label">CSV 文件</span>
      <input
        class="filter-input ci-path" type="text" readonly
        :value="csv.path" placeholder="尚未选择文件"
        :title="csv.path"
      >
      <button class="btn btn-secondary" :disabled="csv.importing" @click="csv.pickFile()">选择文件…</button>
    </div>

    <div class="ci-row">
      <span class="ci-label">编码</span>
      <select class="filter-input ci-sel" v-model="csv.options.encoding" @change="csv.reloadPreview()">
        <option v-for="e in ENCODINGS" :key="e.value" :value="e.value">{{ e.label }}</option>
      </select>

      <span class="ci-label">分隔符</span>
      <select class="filter-input ci-sel" v-model="csv.options.delimiter" @change="csv.reloadPreview()">
        <option v-for="d in DELIMITERS" :key="d.value" :value="d.value">{{ d.label }}</option>
      </select>

      <label class="export-opt ci-opt">
        <input type="checkbox" v-model="csv.options.hasHeader" @change="csv.reloadPreview()">首行为表头
      </label>
      <span v-if="csv.preview" class="ci-detected">
        实际：{{ csv.preview.encoding }} · 分隔符「{{ csv.preview.delimiter === '\t' ? 'Tab' : csv.preview.delimiter }}」 · {{ csv.preview.columns }} 列
      </span>
    </div>

    <div v-if="csv.loading" class="ci-loading">正在读取 {{ fileName() }} …</div>

    <template v-else-if="csv.preview">
      <CsvColumnMapper
        :headers="csv.preview.headers"
        :rows="csv.preview.rows"
        :columns="csv.targetColumns"
        :mapping="csv.mapping"
        :total-rows="csv.preview.totalRows"
      />

      <div class="ci-actions-line">
        <button class="btn btn-secondary" @click="csv.autoMap()">按名称自动关联</button>
        <button class="btn btn-secondary" @click="csv.resetMapping()">全部清空</button>
        <span class="ci-hint">已关联 {{ csv.mapping.filter(Boolean).length }} / {{ csv.targetColumns.length }} 个字段</span>
      </div>

      <div class="ci-row ci-row-wrap">
        <span class="ci-label">空值</span>
        <label class="export-opt ci-opt">
          <input type="checkbox" v-model="csv.options.emptyAsNull">空单元格写为 NULL
        </label>
        <input
          class="filter-input ci-null" type="text"
          autocapitalize="off" autocorrect="off" spellcheck="false"
          v-model="csv.options.nullValue" placeholder="如 NULL / \N"
        >
        <span class="ci-hint">填写的文本会被当作 NULL（留空表示不启用）</span>
      </div>

      <div class="ci-row ci-row-wrap">
        <span class="ci-label">导入方式</span>
        <label class="export-opt ci-opt">
          <input type="checkbox" v-model="csv.options.truncateFirst">导入前清空表（TRUNCATE）
        </label>
        <label class="export-opt ci-opt">
          <input type="checkbox" v-model="csv.options.stopOnError">遇到错误行立即停止
        </label>
        <span class="ci-label">批量</span>
        <input class="filter-input ci-num" type="number" min="1" v-model.number="csv.options.batchSize">
      </div>

      <div v-if="csv.result" class="ci-result">
        导入完成：成功 <b>{{ csv.result.inserted }}</b> 行，失败 <b>{{ csv.result.failed }}</b> 行
        <div v-for="(e, i) in csv.result.errors" :key="i" class="ci-err">{{ e }}</div>
      </div>
    </template>

    <div v-else class="ci-loading">请先选择要导入的 CSV 文件。</div>

    <div class="confirm-actions ci-footer">
      <span class="ci-hint">目标表：{{ csv.modal.table }} · 失败在第几条会明确提示；DDL/DML 均无法回滚。</span>
      <button class="btn btn-secondary" @click="csv.close()">取消</button>
      <button class="btn" :disabled="!csv.canImport || csv.importing" @click="csv.run()">
        {{ csv.importing ? '导入中…' : '开始导入' }}
      </button>
    </div>
  </ModalShell>
</template>

<style scoped>
.ci-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
}
/* .filter-input 自带 12px 下边距（为侧栏搜索框设计），横向排列时会让它整体上移，这里去掉 */
.ci-row .filter-input {
  margin-bottom: 0;
}
.ci-row .btn {
  flex: 0 0 auto;
  white-space: nowrap;
}
.ci-row-wrap {
  flex-wrap: wrap;
}
.ci-label {
  flex: 0 0 auto;
  color: var(--text-dim);
  font-size: 13px;
  letter-spacing: 1px;
}
.ci-path {
  flex: 1;
  min-width: 0;
  color: var(--text-dim);
}
.ci-sel {
  flex: 0 0 150px;
}
.ci-null {
  flex: 0 0 140px;
}
.ci-num {
  flex: 0 0 90px;
}
.ci-opt {
  flex: 0 0 auto;
}
/* 与输入框（9px 上下内边距）保持相近高度，避免同排元素高低不一 */
.ci-row .export-opt {
  padding: 8px 12px;
}
.ci-detected {
  color: var(--cyan-soft);
  font-family: var(--font-mono);
  font-size: 12px;
}
.ci-hint {
  color: var(--text-dim);
  font-size: 12px;
}
.ci-loading {
  padding: 30px;
  text-align: center;
  color: var(--text-dim);
  font-size: 13px;
}
.ci-actions-line {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 12px 0;
}
.ci-result {
  margin-top: 12px;
  padding: 10px 12px;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: rgba(var(--cyan-rgb), .05);
  color: var(--text);
  font-size: 13px;
}
.ci-result b {
  color: var(--cyan);
}
.ci-err {
  margin-top: 4px;
  color: var(--magenta);
  font-family: var(--font-mono);
  font-size: 12px;
}
.ci-footer {
  margin-top: 16px;
  align-items: center;
}
.ci-footer .ci-hint {
  margin-right: auto;
}
</style>
