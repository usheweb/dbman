// UI 状态：侧栏收起、弹窗开关、Toast、右键菜单、导出选项
import { defineStore } from 'pinia';
import { ref } from 'vue';
import { useStoreStore } from './store';
import { DEFAULT_TABLE_SORT, normalizeSortKeys } from '@/utils/tableSort';

const PANEL_TITLES = { sidebar: '连接', tree: '表列表' };

export const useUiStore = defineStore('ui', () => {
  const store = useStoreStore();

  // 侧栏（连接列 / 表列表列）收起状态
  const panelState = ref({ sidebar: false, tree: false });

  // 弹窗可见性：以 id 为键，便于模板统一控制
  const modals = ref({
    connectionModal: false,
    contentModal: false,
    execConfirmModal: false,
    exportModal: false,
    filterModal: false,
    sqlLogModal: false,
    sqlFavModal: false,
    favEditModal: false,
    aiConfigModal: false,
    viewSettingsModal: false,
    promoModal: false
  });

  const toast = ref({ msg: '', type: 'success', visible: false });
  let toastTimer = null;

  // 内容详情弹窗：当前查看的原始值（从单元格 data-value 来）
  const contentValue = ref('');

  // 连接弹窗：非空表示正在编辑已有连接，否则为新建
  const connModalTarget = ref(null);

  // 导出选项（表头方式 / 导出范围），持久化
  const exportOpts = ref({ header: 'both', scope: 'page' });

  // 表列表排序键（可多列组合），持久化
  const tableSort = ref(DEFAULT_TABLE_SORT.map(k => ({ ...k })));

  // 视图显示选项：表列表里是否显示表注释 / 数据行数与大小 + SQL 编辑器是否折叠，持久化
  const viewOpts = ref({ tableComment: true, tableMeta: true, sqlCollapsed: false });

  // 结果区全屏：把窗口让给结果表，只对当前会话有效，不持久化
  const resultFullscreen = ref(false);

  function init() {
    Object.assign(panelState.value, store.get('dbman_panel_state') || {});
    Object.assign(exportOpts.value, store.get('dbman_export_opts') || {});
    Object.assign(viewOpts.value, store.get('dbman_view_opts') || {});
    const saved = normalizeSortKeys(store.get('dbman_table_sort'));
    tableSort.value = saved.length ? saved : DEFAULT_TABLE_SORT.map(k => ({ ...k }));
  }

  function saveViewOpts(opts) {
    viewOpts.value = { ...viewOpts.value, ...opts };
    store.set('dbman_view_opts', { ...viewOpts.value });
  }

  // 折叠 / 展开 SQL 编辑器，把纵向空间让给结果区
  function toggleSqlCollapsed() {
    saveViewOpts({ sqlCollapsed: !viewOpts.value.sqlCollapsed });
  }

  function toggleResultFullscreen() {
    resultFullscreen.value = !resultFullscreen.value;
  }

  function saveTableSort(keys) {
    const next = normalizeSortKeys(keys);
    tableSort.value = next;
    store.set('dbman_table_sort', next);
  }

  function savePanelState() {
    store.set('dbman_panel_state', { ...panelState.value });
  }

  function togglePanel(which) {
    panelState.value[which] = !panelState.value[which];
    savePanelState();
  }

  function setPanel(which, value) {
    panelState.value[which] = !!value;
    savePanelState();
  }

  function panelTitle(which) {
    return PANEL_TITLES[which] || '';
  }

  function openModal(id) {
    if (id in modals.value) modals.value[id] = true;
  }

  function closeModal(id) {
    if (id in modals.value) modals.value[id] = false;
  }

  function showToast(msg, type = 'success') {
    toast.value = { msg, type, visible: true };
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast.value = { ...toast.value, visible: false };
    }, 3000);
  }

  function saveExportOpts(opts) {
    exportOpts.value = { ...opts };
    store.set('dbman_export_opts', { ...exportOpts.value });
  }

  function openContent(value) {
    contentValue.value = value;
    openModal('contentModal');
  }

  function openConnectionModal(conn) {
    connModalTarget.value = conn || null;
    openModal('connectionModal');
  }

  return {
    panelState,
    modals,
    toast,
    exportOpts,
    contentValue,
    connModalTarget,
    tableSort,
    viewOpts,
    resultFullscreen,
    toggleSqlCollapsed,
    toggleResultFullscreen,
    init,
    togglePanel,
    setPanel,
    panelTitle,
    openModal,
    closeModal,
    showToast,
    saveExportOpts,
    saveTableSort,
    saveViewOpts,
    openContent,
    openConnectionModal
  };
});
