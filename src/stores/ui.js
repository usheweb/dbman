// UI 状态：侧栏收起、弹窗开关、Toast、右键菜单、导出选项
import { defineStore } from 'pinia';
import { ref } from 'vue';
import { useStoreStore } from './store';

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

  function init() {
    Object.assign(panelState.value, store.get('dbman_panel_state') || {});
    Object.assign(exportOpts.value, store.get('dbman_export_opts') || {});
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
    init,
    togglePanel,
    setPanel,
    panelTitle,
    openModal,
    closeModal,
    showToast,
    saveExportOpts,
    openContent,
    openConnectionModal
  };
});
