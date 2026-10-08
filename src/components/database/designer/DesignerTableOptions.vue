<script setup>
const props = defineProps({
  options: { type: Object, required: true }
});
const emit = defineEmits(['change']);

const ENGINES = ['InnoDB', 'MyISAM', 'MEMORY', 'ARCHIVE', 'CSV', 'BLACKHOLE'];
const CHARSETS = ['utf8mb4', 'utf8', 'latin1', 'gbk', 'gb2312', 'ascii'];
const COLLATIONS = ['utf8mb4_general_ci', 'utf8mb4_unicode_ci', 'utf8mb4_0900_ai_ci', 'utf8_general_ci', 'utf8_unicode_ci', 'latin1_swedish_ci'];

function touch() {
  emit('change');
}
</script>

<template>
  <div class="dz-opts">
    <datalist id="dz-engines"><option v-for="e in ENGINES" :key="e" :value="e" /></datalist>
    <datalist id="dz-charsets"><option v-for="c in CHARSETS" :key="c" :value="c" /></datalist>
    <datalist id="dz-collations"><option v-for="c in COLLATIONS" :key="c" :value="c" /></datalist>

    <div class="dz-opt-row">
      <label>存储引擎</label>
      <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" list="dz-engines"
        :value="options.engine || ''" placeholder="InnoDB"
        @input="options.engine = $event.target.value || null; touch()">
    </div>
    <div class="dz-opt-row">
      <label>字符集</label>
      <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" list="dz-charsets"
        :value="options.charset || ''" placeholder="utf8mb4"
        @input="options.charset = $event.target.value || null; touch()">
    </div>
    <div class="dz-opt-row">
      <label>排序规则</label>
      <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" list="dz-collations"
        :value="options.collation || ''" placeholder="utf8mb4_general_ci"
        @input="options.collation = $event.target.value || null; touch()">
    </div>
    <div class="dz-opt-row">
      <label>表注释</label>
      <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
        :value="options.comment || ''" placeholder="表说明"
        @input="options.comment = $event.target.value || null; touch()">
    </div>
    <div class="dz-opt-row">
      <label>自增起始值</label>
      <input type="number" min="1"
        :value="options.autoIncrement == null ? '' : options.autoIncrement" placeholder="留空不修改"
        @input="options.autoIncrement = $event.target.value === '' ? null : Number($event.target.value); touch()">
    </div>
    <div class="dz-hint">仅修改与当前结构不同的项；未填写的项不会出现在 ALTER 语句中。</div>
  </div>
</template>

<style scoped>
.dz-opts {
  max-width: 560px;
}
.dz-opt-row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 14px;
}
.dz-opt-row label {
  flex: 0 0 96px;
  color: var(--text-dim);
  font-size: 13px;
  letter-spacing: 1px;
}
.dz-opt-row input {
  flex: 1;
  min-width: 0;
}
.dz-hint {
  margin-top: 18px;
  color: var(--text-dim);
  font-size: 12px;
  line-height: 1.6;
}
</style>
