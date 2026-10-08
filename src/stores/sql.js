// SQL 日志 + 收藏（持久化到 JSONL 文件，见 Rust 侧 store）
import { defineStore } from 'pinia';
import { ref } from 'vue';
import { useStoreStore } from './store';
import { detectSqlType, sqlLogClass, fuzzyScore } from '@/utils/sql';
import { formatDateTime } from '@/utils/format';

const SQL_LOG_MAX = 500;
const SQL_FAV_MAX = 300;

export const useSqlStore = defineStore('sql', () => {
  const store = useStoreStore();

  const sqlLog = ref([]);
  const sqlFavs = ref([]);

  // 收藏编辑弹窗的草稿
  const favDraft = ref({ sql: '', db: '', conn: '', name: '', note: '', editId: '' });

  function init() {
    sqlLog.value = store.get('dbman_sql_log') || [];
    sqlFavs.value = store.get('dbman_sql_favorites') || [];
  }

  // ===== 日志 =====
  function saveLog() {
    store.set('dbman_sql_log', sqlLog.value);
  }

  function addLog(sql, result, connData, database) {
    sqlLog.value.unshift({
      t: Date.now(),
      sql: String(sql).trim(),
      type: detectSqlType(sql),
      db: database || '',
      conn: connData ? connData.name : '',
      ok: !!result.success,
      elapsed: result.elapsed || 0,
      rows: result.rows || 0,
      err: result.error || ''
    });
    if (sqlLog.value.length > SQL_LOG_MAX) sqlLog.value = sqlLog.value.slice(0, SQL_LOG_MAX);
    saveLog();
  }

  function removeLog(idx) {
    if (!sqlLog.value[idx]) return;
    sqlLog.value.splice(idx, 1);
    saveLog();
  }

  function clearLog() {
    sqlLog.value = [];
    saveLog();
  }

  // 日志的日期键（本地时区 YYYY-MM-DD），用于按日期筛选
  function logDateKey(t) {
    return formatDateTime(new Date(t)).slice(0, 10);
  }

  // 按关键词（模糊）与日期筛选日志；kw 为空时保持原顺序
  function filterLogs(kw, date) {
    kw = (kw || '').trim();
    const items = sqlLog.value
      .map((e, idx) => ({ e, idx, score: kw ? fuzzyScore(e.sql, kw) : 0 }))
      .filter(x => !date || logDateKey(x.e.t) === date)
      .filter(x => !kw || x.score > -1)
      .sort((a, b) => (kw ? b.score - a.score : 0));
    return items;
  }

  // 日志里出现过的日期及条数（倒序）
  function logDates() {
    const count = {};
    sqlLog.value.forEach(e => {
      const d = logDateKey(e.t);
      count[d] = (count[d] || 0) + 1;
    });
    return Object.keys(count).sort().reverse().map(d => ({ date: d, count: count[d] }));
  }

  // ===== 收藏 =====
  function saveFavs() {
    store.set('dbman_sql_favorites', sqlFavs.value);
  }

  // 查重键：忽略空白差异与大小写
  function favKey(sql) {
    return String(sql || '').replace(/\s+/g, ' ').trim().toLowerCase();
  }

  function favDisplayName(f) {
    return f.name || String(f.sql || '').split('\n')[0].trim().slice(0, 80);
  }

  function findFavIndexBySql(sql) {
    const k = favKey(sql);
    return sqlFavs.value.findIndex(f => favKey(f.sql) === k);
  }

  function isFaved(sql) {
    return findFavIndexBySql(sql) > -1;
  }

  function openFavDraft(opts) {
    favDraft.value = {
      sql: String(opts.sql || '').trim(),
      db: opts.db || '',
      conn: opts.conn || '',
      name: opts.name || '',
      note: opts.note || '',
      editId: opts.editId || ''
    };
  }

  // 保存草稿：editId 存在改名称/备注；否则新建（同 SQL 已收藏则合并）
  function saveFavDraft() {
    const d = favDraft.value;
    if (!d.sql) return '';
    const name = (d.name || '').trim();
    const note = (d.note || '').trim();
    if (d.editId) {
      const f = sqlFavs.value.find(x => x.id === d.editId);
      if (!f) return '';
      f.name = name;
      f.note = note;
      saveFavs();
      return '收藏已更新';
    }
    const dup = findFavIndexBySql(d.sql);
    if (dup > -1) {
      sqlFavs.value[dup].name = name;
      sqlFavs.value[dup].note = note;
      saveFavs();
      return '这条 SQL 已在收藏中，已更新名称 / 备注';
    }
    sqlFavs.value.unshift({
      id: 'f' + Date.now().toString(36) + Math.random().toString(36).slice(2, 7),
      t: Date.now(),
      sql: d.sql,
      name,
      note,
      type: detectSqlType(d.sql),
      db: d.db,
      conn: d.conn
    });
    if (sqlFavs.value.length > SQL_FAV_MAX) sqlFavs.value = sqlFavs.value.slice(0, SQL_FAV_MAX);
    saveFavs();
    return '已加入 SQL 收藏';
  }

  // 日志行 ☆：已收藏则取消，返回 '已取消收藏'；否则返回 '' 由调用方打开弹窗
  function toggleFavBySql(sql) {
    const at = findFavIndexBySql(sql);
    if (at > -1) {
      sqlFavs.value.splice(at, 1);
      saveFavs();
      return '已取消收藏';
    }
    return '';
  }

  function removeFav(id) {
    const at = sqlFavs.value.findIndex(x => x.id === id);
    if (at < 0) return;
    sqlFavs.value.splice(at, 1);
    saveFavs();
  }

  function clearFavs() {
    sqlFavs.value = [];
    saveFavs();
  }

  function filterFavs(kw) {
    kw = (kw || '').trim();
    const items = sqlFavs.value
      .map(f => {
        const score = kw
          ? Math.max(fuzzyScore(favDisplayName(f), kw), fuzzyScore(f.note || '', kw), fuzzyScore(f.sql, kw))
          : 0;
        return { f, score };
      })
      .filter(x => !kw || x.score > -1)
      .sort((a, b) => (kw ? b.score - a.score : 0));
    return items;
  }

  return {
    sqlLog, sqlFavs, favDraft,
    init,
    addLog, removeLog, clearLog, filterLogs, logDates, logDateKey,
    favKey, favDisplayName, findFavIndexBySql, isFaved,
    openFavDraft, saveFavDraft, toggleFavBySql, removeFav, clearFavs, filterFavs,
    sqlLogClass
  };
});
