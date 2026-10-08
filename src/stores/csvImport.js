// CSV 导入：文件选择、编码/分隔符探测、列关联（CSV 列 → 表字段）、批量导入
import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import * as api from '@/api';
import { useUiStore } from './ui';
import { useConnectionStore } from './connection';

// 新建一次导入时的默认选项
function defaultOptions() {
  return {
    hasHeader: true,
    encoding: 'auto',
    delimiter: 'auto',
    nullValue: '',
    emptyAsNull: false,
    truncateFirst: false,
    stopOnError: true,
    batchSize: 500
  };
}

export const useCsvImportStore = defineStore('csvImport', () => {
  const ui = useUiStore();
  const conn = useConnectionStore();

  const modal = ref({ visible: false, table: '' });
  const path = ref('');
  const preview = ref(null);      // { headers, rows, totalRows, encoding, delimiter, columns }
  const mapping = ref([]);        // 下标 = CSV 列序，值 = 目标字段名 或 null
  const targetColumns = ref([]);  // 目标表的字段名数组
  const options = ref(defaultOptions());
  const loading = ref(false);
  const importing = ref(false);
  const result = ref(null);       // { inserted, failed, errors, truncated }

  const canImport = computed(() => !!path.value && !!preview.value && mapping.value.some(Boolean));

  function open(tableName) {
    modal.value = { visible: true, table: tableName };
    path.value = '';
    preview.value = null;
    mapping.value = [];
    result.value = null;
    options.value = defaultOptions();
    loadTargetColumns();
  }

  function close() {
    modal.value = { ...modal.value, visible: false };
  }

  async function loadTargetColumns() {
    try {
      const cols = await conn.withRetry(id =>
        api.mysqlColumns(id, conn.currentDatabase, modal.value.table)
      );
      targetColumns.value = cols.map(c => c.name);
    } catch (e) {
      targetColumns.value = [];
      ui.showToast(String(e.message || e), 'error');
    }
  }

  // 选择 CSV 文件并读取预览
  async function pickFile() {
    const picked = await api.openDialog({
      multiple: false,
      filters: [{ name: 'CSV', extensions: ['csv', 'txt', 'tsv'] }]
    });
    const file = Array.isArray(picked) ? picked[0] : picked;
    if (!file) return;
    path.value = file;
    await reloadPreview();
  }

  // 编码 / 分隔符 / 表头变化后重新读取预览
  async function reloadPreview() {
    if (!path.value) return;
    loading.value = true;
    try {
      const data = await api.csvPreview(
        path.value,
        options.value.encoding,
        options.value.delimiter,
        options.value.hasHeader,
        20
      );
      preview.value = data;
      autoMap();
    } catch (e) {
      preview.value = null;
      ui.showToast(String(e.message || e), 'error');
    } finally {
      loading.value = false;
    }
  }

  // 按表头与字段名（忽略大小写、去空格）自动关联，无法匹配的留空由用户手选
  function autoMap() {
    const headers = preview.value ? preview.value.headers : [];
    const byLower = new Map(targetColumns.value.map(c => [c.toLowerCase(), c]));
    mapping.value = headers.map(h => {
      if (!h) return null;
      const key = String(h).trim().toLowerCase();
      return byLower.get(key) || null;
    });
  }

  function resetMapping() {
    mapping.value = (preview.value ? preview.value.headers : []).map(() => null);
  }

  async function run() {
    if (!canImport.value) return false;
    importing.value = true;
    result.value = null;
    // 数字输入被清空时是空串，这里兜底，避免后端反序列化失败
    const opts = {
      ...options.value,
      batchSize: parseInt(options.value.batchSize, 10) || 500
    };
    try {
      const res = await conn.withRetry(id => api.mysqlImportCsv(
        id,
        conn.currentDatabase,
        modal.value.table,
        path.value,
        mapping.value.map(v => (v ? v : null)),
        opts
      ));
      result.value = res;
      if (res.failed) {
        ui.showToast(`导入完成：成功 ${res.inserted} 行，失败 ${res.failed} 行`, 'error');
      } else {
        ui.showToast(`导入完成：共 ${res.inserted} 行`, 'success');
      }
      // 数据变了，刷新当前表的列缓存与结果集
      delete conn.tableColumns[modal.value.table];
      if (conn.currentTable === modal.value.table) {
        await conn.loadColumns(conn.currentDatabase, modal.value.table);
      }
      return true;
    } catch (e) {
      ui.showToast(String(e.message || e), 'error');
      return false;
    } finally {
      importing.value = false;
    }
  }

  return {
    modal, path, preview, mapping, targetColumns, options, loading, importing, result,
    canImport,
    open, close, pickFile, reloadPreview, autoMap, resetMapping, run
  };
});
