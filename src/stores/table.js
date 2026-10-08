// 表级操作：查看/导出 DDL、导出数据 SQL、表结构设计器、重命名/清空/删除/复制表结构
import { defineStore } from 'pinia';
import { ref } from 'vue';
import * as api from '@/api';
import { useUiStore } from './ui';
import { useConnectionStore } from './connection';
import { normalizeStructure } from '@/utils/ddl';
import { sqlIdent } from '@/utils/sql';

const qi = sqlIdent;

export const useTableStore = defineStore('table', () => {
  const ui = useUiStore();
  const conn = useConnectionStore();

  // 查看建表语句弹窗
  const ddlModal = ref({ visible: false, table: '', ddl: '', loading: false });
  // 表结构设计器弹窗
  const designerModal = ref({ visible: false, table: '' });
  // 导出 SQL 弹窗（structure / data 两种模式）
  const exportSqlModal = ref({ visible: false, table: '', mode: 'structure' });
  // 通用文本输入弹窗（WKWebView 不支持 window.prompt）
  const prompt = ref({ visible: false, title: '', label: '', placeholder: '', value: '', confirmText: '确定' });
  let promptResolver = null;

  // ===== 查看建表语句 =====
  async function openDdl(table) {
    ddlModal.value = { visible: true, table, ddl: '', loading: true };
    try {
      const ddl = await conn.withRetry(id => api.mysqlTableDdl(id, conn.currentDatabase, table));
      ddlModal.value = { visible: true, table, ddl, loading: false };
    } catch (e) {
      ddlModal.value = { ...ddlModal.value, loading: false };
      ui.showToast(String(e.message || e), 'error');
    }
  }
  function closeDdl() { ddlModal.value = { ...ddlModal.value, visible: false }; }

  // ===== 设计器 =====
  function openDesigner(table) { designerModal.value = { visible: true, table }; }
  function closeDesigner() { designerModal.value = { ...designerModal.value, visible: false }; }

  // 读取并规范化表结构（供设计器使用）
  async function loadStructure(table) {
    const raw = await conn.withRetry(id => api.mysqlTableStructure(id, conn.currentDatabase, table));
    return normalizeStructure(raw);
  }

  // ===== 导出 SQL 弹窗 =====
  function openExportSql(table, mode) { exportSqlModal.value = { visible: true, table, mode }; }
  function closeExportSql() { exportSqlModal.value = { ...exportSqlModal.value, visible: false }; }

  // ===== 通用文本输入 =====
  function askText(opts) {
    return new Promise(resolve => {
      promptResolver = resolve;
      prompt.value = {
        visible: true,
        title: opts.title || '请输入',
        label: opts.label || '',
        placeholder: opts.placeholder || '',
        value: opts.value || '',
        confirmText: opts.confirmText || '确定'
      };
    });
  }
  function resolvePrompt(value) {
    prompt.value = { ...prompt.value, visible: false };
    const resolve = promptResolver;
    promptResolver = null;
    if (resolve) resolve(value);
  }

  // ===== DDL 执行 + 刷新 =====
  // affected：受影响的表名；action：'alter' | 'drop' | 'rename' | 'create'
  async function refreshAfterDdl(affected, action) {
    if (!conn.currentDatabase) return;
    await conn.loadTables(conn.currentDatabase);
    if (!affected) return;
    if (conn.tableColumns[affected]) delete conn.tableColumns[affected];
    if (conn.currentTable === affected) {
      if (action === 'drop' || action === 'rename') {
        conn.currentTable = null;
      } else {
        await conn.loadColumns(conn.currentDatabase, affected);
      }
    }
  }

  // 直接执行 DDL 脚本（会抛错，供设计器自行处理错误展示）
  async function execDdl(sql, affected, action) {
    const count = await conn.withRetry(id => api.mysqlExecDdl(id, conn.currentDatabase, sql));
    await refreshAfterDdl(affected, action);
    return count;
  }

  // ===== 导出 =====
  async function exportStructureSql(table, opts = {}) {
    const path = await api.saveDialog({
      defaultPath: `${table}.sql`,
      filters: [{ name: 'SQL', extensions: ['sql'] }]
    });
    if (!path) return false;
    try {
      const ddl = await conn.withRetry(id => api.mysqlTableDdl(id, conn.currentDatabase, table));
      if (!ddl) throw new Error('未能获取建表语句');
      const parts = [];
      if (opts.withDrop !== false) parts.push(`DROP TABLE IF EXISTS ${qi(table)};`);
      parts.push(ddl.trim().replace(/;+$/, '') + ';');
      await api.writeTextFile(path, parts.join('\n') + '\n');
      ui.showToast('已导出表结构 SQL', 'success');
      return true;
    } catch (e) {
      ui.showToast(String(e.message || e), 'error');
      return false;
    }
  }

  async function exportDataSql(table, opts = {}) {
    const path = await api.saveDialog({
      defaultPath: `${table}_data.sql`,
      filters: [{ name: 'SQL', extensions: ['sql'] }]
    });
    if (!path) return false;
    try {
      const count = await conn.withRetry(id => api.mysqlExportDataSql(
        id, conn.currentDatabase, table, path,
        opts.withDrop !== false, opts.withCreate !== false, opts.batchSize || 1000
      ));
      ui.showToast(`已导出 ${count} 行数据 SQL`, 'success');
      return true;
    } catch (e) {
      ui.showToast(String(e.message || e), 'error');
      return false;
    }
  }

  // ===== 菜单级操作（自带 toast） =====
  async function runMenuOp(fn, successMsg) {
    try {
      await fn();
      if (successMsg) ui.showToast(successMsg, 'success');
      return true;
    } catch (e) {
      ui.showToast(String(e.message || e), 'error');
      return false;
    }
  }

  function renameTable(table, newName) {
    return runMenuOp(
      () => execDdl(`RENAME TABLE ${qi(table)} TO ${qi(newName)};`, table, 'rename'),
      `已重命名为「${newName}」`
    );
  }

  function createTableLike(table, newName) {
    return runMenuOp(
      () => execDdl(`CREATE TABLE ${qi(newName)} LIKE ${qi(table)};`, null, 'create'),
      `已创建表「${newName}」`
    );
  }

  function dropTable(table) {
    return runMenuOp(
      () => execDdl(`DROP TABLE ${qi(table)};`, table, 'drop'),
      `已删除表「${table}」`
    );
  }

  function truncateTable(table) {
    return runMenuOp(
      () => conn.withRetry(id => api.mysqlExecDdl(id, conn.currentDatabase, `TRUNCATE TABLE ${qi(table)};`)),
      `已清空表「${table}」`
    );
  }

  return {
    ddlModal,
    designerModal,
    exportSqlModal,
    prompt,
    openDdl,
    closeDdl,
    openDesigner,
    closeDesigner,
    loadStructure,
    openExportSql,
    closeExportSql,
    askText,
    resolvePrompt,
    execDdl,
    refreshAfterDdl,
    exportStructureSql,
    exportDataSql,
    renameTable,
    createTableLike,
    dropTable,
    truncateTable
  };
});
