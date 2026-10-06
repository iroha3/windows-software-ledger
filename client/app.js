// client/app.js
// 软件备份台账前端交互逻辑：统一线性 SVG 图标、卡片连续切换、实时无感自动保存、云端/本地 LLM 支持

let softwareList = [];
let machinesList = [];
let availablePaths = [];
let selectedIds = new Set();
let activeMachine = 'all';
let activeItem = null;
let machineAliases = {};
let currentMachine = '';
let scanDirsByMachine = {};
let webdavEnabled = false; // WebDAV 是否已启用（关闭时隐藏同步入口）
let mcpInfo = null;        // 当前 exe 路径等（供设置页生成 MCP 配置片段）
let mcpLoaded = false;
let drawerVault = null; // 抽屉的配置归档组件实例

const REPO_URL = 'https://github.com/iroha3/windows-software-ledger';
const HOMEPAGE_URL = 'https://iroha3.github.io/windows-software-ledger/';

function getMachineDisplayName(id) {
  if (!id) return '未知设备';
  return machineAliases[id] || id;
}

// 绿色/便携软件：形态决定处置方式推导（绿色版压缩目录，安装版重新下载）。
function isPortableItem(item) {
  if (!item) return false;
  if (item.type === 'portable') return true;
  return (item.machines || []).some(m => m.form === 'portable');
}

// 依据恢复意愿推导处置方式：必须恢复 → 绿色版压缩目录、安装版重新下载；其余 → 无需操作。
function deriveStrategy(intent, portable) {
  if (intent === 'must') return portable ? 'copy_dir' : 'redownload';
  return 'none';
}

// 自动保存防抖计时器与标志
let autoSaveTimer = null;
let pendingDrawerSave = false;

// 统一轻量线性 SVG 图标库 (Feather / Lucide 风格，避免使用 Emoji)
const ICONS = {
  box: `<svg class="i" viewBox="0 0 24 24"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"></path><polyline points="3.27 6.96 12 12.01 20.73 6.96"></polyline><line x1="12" y1="22.08" x2="12" y2="12"></line></svg>`,
  settings: `<svg class="i sm" viewBox="0 0 24 24"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path></svg>`,
  moon: `<svg class="i sm" viewBox="0 0 24 24"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path></svg>`,
  sun: `<svg class="i sm" viewBox="0 0 24 24"><circle cx="12" cy="12" r="5"></circle><line x1="12" y1="1" x2="12" y2="3"></line><line x1="12" y1="21" x2="12" y2="23"></line><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line><line x1="1" y1="12" x2="3" y2="12"></line><line x1="21" y1="12" x2="23" y2="12"></line><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line></svg>`,
  plus: `<svg class="i sm" viewBox="0 0 24 24"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>`,
  refresh: `<svg class="i sm" viewBox="0 0 24 24"><polyline points="23 4 23 10 17 10"></polyline><polyline points="1 20 1 14 7 14"></polyline><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"></path></svg>`,
  download: `<svg class="i sm" viewBox="0 0 24 24"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>`,
  search: `<svg class="i sm" viewBox="0 0 24 24"><circle cx="11" cy="11" r="8"></circle><line x1="21" y1="21" x2="16.65" y2="16.65"></line></svg>`,
  folder: `<svg class="i sm" viewBox="0 0 24 24"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>`,
  star: `<svg class="i sm" viewBox="0 0 24 24"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>`,
  starFilled: `<svg class="i sm fill" viewBox="0 0 24 24" style="color: #c9a24a;"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>`,
  check: `<svg class="i sm" viewBox="0 0 24 24"><polyline points="20 6 9 17 4 12"></polyline></svg>`,
  sparkles: `<svg class="i sm" viewBox="0 0 24 24"><path d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83"></path></svg>`,
  link: `<svg class="i sm" viewBox="0 0 24 24"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"></path><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"></path></svg>`,
  trash: `<svg class="i sm" viewBox="0 0 24 24"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>`,
  x: `<svg class="i sm" viewBox="0 0 24 24"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>`,
  arrowLeft: `<svg class="i sm" viewBox="0 0 24 24"><line x1="19" y1="12" x2="5" y2="12"></line><polyline points="12 19 5 12 12 5"></polyline></svg>`,
  arrowRight: `<svg class="i sm" viewBox="0 0 24 24"><line x1="5" y1="12" x2="19" y2="12"></line><polyline points="12 5 19 12 12 19"></polyline></svg>`,
  externalLink: `<svg class="i sm" viewBox="0 0 24 24"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path><polyline points="15 3 21 3 21 9"></polyline><line x1="10" y1="14" x2="21" y2="3"></line></svg>`,
  device: `<svg class="i sm" viewBox="0 0 24 24"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>`,
  info: `<svg class="i sm" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></line></svg>`,
  alertTriangle: `<svg class="i sm" viewBox="0 0 24 24"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>`
};

// DOM 元素引用
const toastContainer = document.getElementById('toastContainer');
const tableBody = document.getElementById('tableBody');
const searchInput = document.getElementById('searchInput');
const categoryFilter = document.getElementById('categoryFilter');
const intentFilter = document.getElementById('intentFilter');
const strategyFilter = document.getElementById('strategyFilter');
const prepFilter = document.getElementById('prepFilter');
const pathFilter = document.getElementById('pathFilter');
const awesomeFilter = document.getElementById('awesomeFilter');
const machineTabs = document.getElementById('machineTabs');
const selectAllCheckbox = document.getElementById('selectAllCheckbox');

// 浮动批量操作条
const batchBar = document.getElementById('batchBar');
const batchInfo = document.getElementById('batchInfo');
const btnBatchMust = document.getElementById('btnBatchMust');
const btnBatchOnDemand = document.getElementById('btnBatchOnDemand');
const btnBatchDrop = document.getElementById('btnBatchDrop');
const btnBatchReset = document.getElementById('btnBatchReset');
const btnBatchReady = document.getElementById('btnBatchReady');
const btnBatchMerge = document.getElementById('btnBatchMerge');
const btnBatchDelete = document.getElementById('btnBatchDelete');
const btnBatchLLM = document.getElementById('btnBatchLLM');
const batchProgress = document.getElementById('batchProgress');
const batchProgressFill = document.getElementById('batchProgressFill');

// AI 批量预判的运行状态（供进度反馈与「点击停止」使用）
let batchLlmRunning = false;
let batchLlmAbort = false;

// 顶部栏按钮
const btnScanLocal = document.getElementById('btnScanLocal');
const scanIcon = document.getElementById('scanIcon');
const btnExport = document.getElementById('btnExport');
const btnAddSoftware = document.getElementById('btnAddSoftware');
const btnThemeToggle = document.getElementById('btnThemeToggle');
const themeIcon = document.getElementById('themeIcon');
const themeText = document.getElementById('themeText');
const btnConfig = document.getElementById('btnConfig');

// 侧边抽屉
const sideDrawer = document.getElementById('sideDrawer');
const drawerOverlay = document.getElementById('drawerOverlay');
const btnCloseDrawer = document.getElementById('btnCloseDrawer');
const btnCloseDrawerBottom = document.getElementById('btnCloseDrawerBottom');
const btnDeleteCurrent = document.getElementById('btnDeleteCurrent');
const btnDrawerLLM = document.getElementById('btnDrawerLLM');
const btnPrevDrawer = document.getElementById('btnPrevDrawer');
const btnNextDrawer = document.getElementById('btnNextDrawer');
const drawerIndexBadge = document.getElementById('drawerIndexBadge');
const drawerSaveStatus = document.getElementById('drawerSaveStatus');

// 批量添加弹窗
const batchAddModal = document.getElementById('batchAddModal');
const batchAddInput = document.getElementById('batchAddInput');
const batchAddIntent = document.getElementById('batchAddIntent');
const btnConfirmBatchAdd = document.getElementById('btnConfirmBatchAdd');

// 配置弹窗
const configModal = document.getElementById('configModal');
const configLlmUrl = document.getElementById('configLlmUrl');
const configLlmModel = document.getElementById('configLlmModel');
const configLlmKey = document.getElementById('configLlmKey');
const btnSaveConfig = document.getElementById('btnSaveConfig');
const configWebdavEnabled = document.getElementById('configWebdavEnabled');
const configWebdavUrl = document.getElementById('configWebdavUrl');
const configWebdavUser = document.getElementById('configWebdavUser');
const configWebdavPass = document.getElementById('configWebdavPass');
const configMcpWrite = document.getElementById('configMcpWrite');
const btnWebdavTest = document.getElementById('btnWebdavTest');
const btnSyncNow = document.getElementById('btnSyncNow');
const syncNowIcon = document.getElementById('syncNowIcon');
const syncNowLabel = document.getElementById('syncNowLabel');
const btnForcePush = document.getElementById('btnForcePush');
const btnForcePull = document.getElementById('btnForcePull');
const btnSyncAdvanced = document.getElementById('btnSyncAdvanced');
const syncAdvanced = document.getElementById('syncAdvanced');
const syncAdvancedWrap = document.getElementById('syncAdvancedWrap');
const webdavTestResult = document.getElementById('webdavTestResult');
const mcpClientSelect = document.getElementById('mcpClientSelect');
const mcpSnippet = document.getElementById('mcpSnippet');
const mcpTargetHint = document.getElementById('mcpTargetHint');
const btnMcpCopy = document.getElementById('btnMcpCopy');
const mcpDocLink = document.getElementById('mcpDocLink');
const configNav = document.getElementById('configNav');

// Toast 非阻塞消息提示系统 (使用纯净 SVG 图标)
function showToast(message, type = 'info', duration = 2500) {
  const toast = document.createElement('div');
  toast.className = `toast toast-${type}`;
  let iconSvg = ICONS.info;
  if (type === 'success') iconSvg = ICONS.check;
  if (type === 'warning') iconSvg = ICONS.alertTriangle;
  if (type === 'error') iconSvg = ICONS.alertTriangle;

  toast.innerHTML = `<span style="display:inline-flex;align-items:center;">${iconSvg}</span><span>${escapeHtml(message)}</span>`;
  toastContainer.appendChild(toast);

  setTimeout(() => {
    toast.style.opacity = '0';
    toast.style.transform = 'translateY(-10px) scale(0.95)';
    setTimeout(() => toast.remove(), 200);
  }, duration);
}

// 初始载入
async function init() {
  initTheme();
  await fetchStatus();
  // 默认激活「这台机器」标签（本机暂无数据时退回「全部机器」）
  if (currentMachine && machinesList.includes(currentMachine)) {
    activeMachine = currentMachine;
  }
  await loadSoftware();
  drawerVault = window.Vault ? window.Vault.mount({
    pill: 'drawerVaultToggle',
    panel: 'drawerVaultPanel',
    list: 'drawerVaultList',
    add: 'drawerVaultAdd',
    count: 'drawerVaultCount',
    sub: 'drawerVaultSub',
    kind: 'soft',
    id: null,
  }) : null;
  bindEvents();
  bindKeyboardShortcuts();
  enableWheelSelect();
  // WebDAV 配置（不再自动同步，只调整入口显隐）
  await initSync();
  await syncLedgerWatch();
  // 首次运行：弹新手引导（由引导页的「扫描本机」触发扫描）；否则空台账时自动扫描。
  const welcomeShown = window.Welcome ? window.Welcome.maybeShow(softwareList.length > 0) : false;
  if (!welcomeShown && softwareList.length === 0) {
    handleScanLocal();
  }
}

// ---------------------------------------------------------------------------
// WebDAV 同步（手动，无锁）
//
// 用户约定同一时刻只在一台机器上编辑；只有点「立即同步」才会拉取合并并推送。
// 没有会话锁、没有只读模式；「用本地/远端覆盖」是强制覆盖（会自动快照）。
// ---------------------------------------------------------------------------

function setSyncBusy(busy) {
  if (btnSyncNow) {
    btnSyncNow.disabled = busy || !webdavEnabled;
    if (syncNowLabel) syncNowLabel.textContent = busy ? '同步中…' : '立即同步';
    if (syncNowIcon) syncNowIcon.classList.toggle('spin', busy);
  }
  if (btnForcePush) btnForcePush.disabled = busy || !webdavEnabled;
  if (btnForcePull) btnForcePull.disabled = busy || !webdavEnabled;
}

// 渐进披露：未勾选「启用 WebDAV 同步」时收起下面的地址/凭据/操作区。
function toggleWebdavFields() {
  const el = document.getElementById('webdavFields');
  if (el) el.style.display = (configWebdavEnabled && configWebdavEnabled.checked) ? 'flex' : 'none';
}

// 关闭 WebDAV 时禁用同步入口；开启时恢复。
function applyWebdavUi(enabled) {
  webdavEnabled = !!enabled;
  [btnSyncNow, btnForcePush, btnForcePull].forEach(b => {
    if (b) b.disabled = !webdavEnabled;
  });
}

function describeSync(sync) {
  const s = (sync && sync.summary) || {};
  const parts = [];
  if (s.downloaded) parts.push(`下载 ${s.downloaded}`);
  if (s.uploaded) parts.push(`上传 ${s.uploaded}`);
  if (s.deletedLocal) parts.push(`本地删 ${s.deletedLocal}`);
  if (s.deletedRemote) parts.push(`远端删 ${s.deletedRemote}`);
  if (s.merged) parts.push(`合并台账 ${s.merged}`);
  if (s.conflicts) parts.push(`冲突 ${s.conflicts}`);
  return parts.length ? parts.join('，') : '已是最新';
}

function toastSyncResult(sync) {
  if (!sync || sync.enabled === false) return;
  if (sync.success) showToast('同步完成：' + describeSync(sync), 'success', 4000);
  else if (sync.error) showToast('同步失败：' + sync.error, 'error', 5000);
}

// 只在初始化 / 保存设置后读一次配置调整入口显隐；不在启动时自动同步。
async function initSync() {
  let wd = {};
  try {
    wd = await (await fetch('/api/webdav/config')).json();
  } catch (e) { return; }
  applyWebdavUi(!!wd.enabled && !!wd.url);
  await refreshSyncStatus();
}

// 展示上次同步时间（本地记录 data/.sync/last.json）。
async function refreshSyncStatus() {
  const el = document.getElementById('syncLast');
  if (!el) return;
  try {
    const st = await (await fetch('/api/sync/status')).json();
    if (st && st.last && st.last.at) {
      const when = new Date(st.last.at).toLocaleString();
      const mid = st.last.machine || '';
      const who = mid ? getMachineDisplayName(mid) : '';
      el.textContent = `上次同步：${when}${who ? '（' + who + '）' : ''}`;
    } else {
      el.textContent = '尚未进行过同步';
    }
  } catch (e) { /* 读不到就不显示 */ }
}

async function doSync(mode) {
  setSyncBusy(true);
  try {
    const res = await (await fetch('/api/sync/now', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ mode })
    })).json();
    toastSyncResult(res);
    if (res.success) {
      await fetchStatus();
      await loadSoftware();
      await refreshSyncStatus();
    }
  } catch (e) {
    showToast('同步失败：' + e.message, 'error', 5000);
  } finally {
    setSyncBusy(false);
  }
}

async function testWebdav() {
  const setStatus = (ok, msg) => {
    if (!webdavTestResult) return;
    webdavTestResult.style.display = 'inline-flex';
    webdavTestResult.className = 'sync-test-result ' + (ok ? 'ok' : 'err');
    webdavTestResult.textContent = msg;
  };
  try {
    // 先落盘再测，保证测的是当前填写的地址/凭据
    await fetch('/api/webdav/config', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        enabled: !!(configWebdavEnabled && configWebdavEnabled.checked),
        url: configWebdavUrl ? configWebdavUrl.value.trim() : '',
        username: configWebdavUser ? configWebdavUser.value.trim() : '',
        password: configWebdavPass ? configWebdavPass.value : ''
      })
    });
    const res = await (await fetch('/api/webdav/test', { method: 'POST' })).json();
    if (res.success) {
      setStatus(true, '连接正常');
      showToast('WebDAV 连接正常', 'success');
    } else {
      setStatus(false, res.error || '连接失败');
      showToast('连接失败：' + (res.error || '未知错误'), 'error', 5000);
    }
  } catch (e) {
    setStatus(false, e.message);
    showToast('连接失败：' + e.message, 'error');
  }
}

// 主页卡片（侧边抽屉）内悬浮滚轮切换处置方式：原生 <select> 在 WebView 中对滚轮无响应，
// 抽屉聚焦单条记录、操作意图明确，这里接管滚轮逐项切换并阻止面板滚动。
// 主页表格不做此处理：滚动时鼠标掠过下拉会误改数据且静默保存。
function enableWheelSelect() {
  const selector = '#drawerStrategy';
  document.addEventListener('wheel', (e) => {
    if (!(e.target instanceof Element)) return;
    const sel = e.target.closest(selector);
    if (!sel || sel.disabled || sel.options.length === 0) return;
    e.preventDefault();
    const dir = e.deltaY > 0 ? 1 : -1;
    const next = sel.selectedIndex + dir;
    if (next < 0 || next >= sel.options.length) return;
    sel.selectedIndex = next;
    sel.dispatchEvent(new Event('change', { bubbles: true }));
  }, { passive: false, capture: true });
}

// 主题切换机制 (默认日间主题)
function initTheme() {
  const savedTheme = localStorage.getItem('app-theme') || 'light';
  applyTheme(savedTheme);
}

function applyTheme(theme) {
  if (theme === 'dark') {
    document.documentElement.setAttribute('data-theme', 'dark');
    themeIcon.innerHTML = ICONS.sun;
    themeText.innerText = '日间';
  } else {
    document.documentElement.removeAttribute('data-theme');
    themeIcon.innerHTML = ICONS.moon;
    themeText.innerText = '夜间';
  }
  localStorage.setItem('app-theme', theme);
}

function toggleTheme() {
  const isDark = document.documentElement.getAttribute('data-theme') === 'dark';
  applyTheme(isDark ? 'light' : 'dark');
}

async function fetchStatus() {
  try {
    const res = await fetch('/api/status');
    const data = await res.json();
    machinesList = data.machines || [];
    machineAliases = data.machine_aliases || {};
    currentMachine = data.current_machine || '';
    availablePaths = data.availablePaths || [];
    renderStats(data.stats);
    renderMachineTabs(data.machines);
    renderPathFilter(availablePaths);
    // 这里刻意不重渲染表格：状态刷新被大量单行编辑（下拉改意愿/处置）调用，
    // 整表重建会拆掉用户正在操作的 <select>（下拉自己缩回、点了没反应）。
    // 需要重渲染的路径都会自己调 renderTable()/loadSoftware()。
  } catch (e) {
    console.error('Failed to fetch status:', e);
  }
}

function renderPathFilter(paths) {
  const currentVal = pathFilter.value;
  let html = `<option value="all">全部安装路径</option>`;
  for (const p of paths) {
    html += `<option value="${escapeHtml(p)}" ${currentVal === p ? 'selected' : ''}>${escapeHtml(p)}</option>`;
  }
  pathFilter.innerHTML = html;
}

async function loadSoftware() {
  try {
    const res = await fetch('/api/software');
    softwareList = await res.json();
    renderTable();
    if (machinesList.length > 0) {
      renderMachineTabs(machinesList);
    }
  } catch (e) {
    console.error('Failed to load software:', e);
  }
}

// ---------------------------------------------------------------------------
// 台账外部变更监听：agent 通过 MCP 写工具改了 software.json 后，
// 主动把表格刷新出来，让人实时看到它在改什么。轮询一个廉价的版本指纹，
// 只有变了才拉全量，避免空转。
// ---------------------------------------------------------------------------
let ledgerRevision = null;
let ledgerWatchTimer = null;

// 自己写入的标记：本地改动同样会 bump 版本指纹，不标记就会被误判成「外部更新」。
// 直接包一层 fetch：凡是对台账的写请求都记一笔（后端还会改 updated_at、
// 按意愿推导 backup_strategy，所以不能拿内存内容去比对）。
let selfWriteAt = 0;
(function markSelfWrites() {
  const origFetch = window.fetch.bind(window);
  window.fetch = function (input, init) {
    const url = typeof input === 'string' ? input : (input && input.url) || '';
    const method = String((init && init.method) || (input && input.method) || 'GET').toUpperCase();
    if (method === 'POST' && /\/api\/(software|scan|sync)\//.test(url)) {
      selfWriteAt = Date.now();
    }
    return origFetch(input, init);
  };
})();

// 正在编辑 / 有弹窗时先不刷新，避免打断输入或吞掉未保存的改动。
function isEditingBusy() {
  if (sideDrawer && sideDrawer.classList.contains('show')) return true;
  if (batchBar && batchBar.classList.contains('show')) return true;
  return !!document.querySelector('.modal-overlay.show');
}

async function pollLedgerRevision() {
  if (document.hidden) return; // 窗口在后台不干活，回到前台时补一次
  try {
    const data = await (await fetch('/api/ledger-revision')).json();
    const rev = data.revision;
    if (ledgerRevision === null) { ledgerRevision = rev; return; }
    if (rev === ledgerRevision) return;
    // 自己刚写的：界面已就地更新过，直接记下指纹即可。
    // 不能重刷——整表重建会拆掉用户正在拖的下拉。
    const self = (Date.now() - selfWriteAt) < 5000;
    if (self) { ledgerRevision = rev; return; }
    // 外部（MCP）写入：正在编辑/开弹窗就先等，别打断；空闲时再刷并提示。
    if (isEditingBusy()) return;
    ledgerRevision = rev;
    await loadSoftware();
    await fetchStatus();
    showToast('台账已被外部更新，已自动刷新', 'success', 3000);
  } catch (e) { /* 轮询失败静默重试 */ }
}

function stopLedgerWatch() {
  if (ledgerWatchTimer) { clearInterval(ledgerWatchTimer); ledgerWatchTimer = null; }
}

function startLedgerWatch() {
  if (ledgerWatchTimer) return;
  ledgerRevision = null; // 重新取基线，避免刚开启的那一轮误报
  pollLedgerRevision();
  ledgerWatchTimer = setInterval(pollLedgerRevision, 2000);
}

// 只有开了 MCP 写入，才可能有「外部写入者」，才值得盯台账变化；
// 写开关关着又没同步拉取时，台账不会被别人动，别白轮询。
async function syncLedgerWatch() {
  try {
    const cfg = await (await fetch('/api/config')).json();
    if (cfg.mcp_write_enabled) startLedgerWatch();
    else stopLedgerWatch();
  } catch (e) { /* 读不到配置就不轮询 */ }
}

// 窗口切回前台时立刻补一次，避免后台期间错过的变更没反映
document.addEventListener('visibilitychange', () => {
  if (!document.hidden) pollLedgerRevision();
});

function renderStats(stats) {
  if (!stats) return;
  document.getElementById('statTotal').innerText = stats.total || 0;
  document.getElementById('statMust').innerText = stats.must || 0;
  document.getElementById('statOnDemand').innerText = stats.on_demand || 0;
  document.getElementById('statDrop').innerText = stats.drop || 0;
  document.getElementById('statUnreviewed').innerText = stats.unreviewed || 0;
  document.getElementById('statBackup').innerText = stats.backupTasks || 0;
  document.getElementById('statReady').innerText = stats.ready || 0;
  document.getElementById('statAwesome').innerText = stats.awesome || 0;
}

function renderMachineTabs(machines) {
  let html = `<button class="tab-btn ${activeMachine === 'all' ? 'active' : ''}" data-machine="all">全部机器 (${softwareList.length})</button>`;
  for (const m of machines) {
    const count = softwareList.filter(s => s.machines.some(sm => sm.machine_id === m)).length;
    const displayName = getMachineDisplayName(m);
    html += `<button class="tab-btn ${activeMachine === m ? 'active' : ''}" data-machine="${m}" title="设备ID: ${m}">${machineDeviceIcon(m)}${escapeHtml(displayName)} (${count})</button>`;
  }
  machineTabs.innerHTML = html;
}

// 筛选逻辑
function getFilteredSoftware() {
  const query = searchInput.value.toLowerCase().trim();
  const cat = categoryFilter.value;
  const intent = intentFilter.value;
  const strat = strategyFilter.value;
  const prep = prepFilter.value;
  const pathSel = pathFilter.value;
  const awesomeOnly = awesomeFilter.checked;

  return softwareList.filter(item => {
    if (activeMachine !== 'all') {
      const onMachine = item.machines.some(m => m.machine_id === activeMachine);
      if (!onMachine) return false;
    }
    if (cat !== 'all' && item.category !== cat) return false;
    if (intent !== 'all') {
      const itemIntent = item.restore_intent || 'unreviewed';
      if (itemIntent !== intent) return false;
    }
    if (strat !== 'all') {
      const itemStrat = item.backup_strategy || 'none';
      if (itemStrat !== strat) return false;
    }
    if (prep !== 'all') {
      const itemPrep = item.prep_status || 'todo';
      if (itemPrep !== prep) return false;
    }
    if (pathSel !== 'all') {
      const matchPath = item.machines.some(m => {
        const p = (m.install_location || m.path || '').toLowerCase();
        return p.includes(pathSel.toLowerCase());
      });
      if (!matchPath) return false;
    }
    if (awesomeOnly && !item.is_awesome) return false;

    if (query) {
      const nameMatch = item.name.toLowerCase().includes(query);
      const verMatch = (item.version || '').toLowerCase().includes(query);
      const urlMatch = (item.download_url || '').toLowerCase().includes(query);
      const notesMatch = (item.config_notes || '').toLowerCase().includes(query);
      const idMatch = (item.id || '').toLowerCase().includes(query);
      const pathMatch = item.machines.some(m => (m.install_location || m.path || '').toLowerCase().includes(query));
      if (!nameMatch && !verMatch && !urlMatch && !notesMatch && !idMatch && !pathMatch) return false;
    }

    return true;
  });
}

// 软件图标：扫描时抽取的 exe 图标以 data URI 注入；缺失则回退通用方盒图标
function appIconHtml(item) {
  if (item.icon) return `<img class="app-icon" src="${item.icon}" alt="" loading="lazy">`;
  return `<span class="app-icon-fallback" title="无图标">${ICONS.box}</span>`;
}

// 渲染表格
function buildRowHtml(item) {
  const isSelected = selectedIds.has(item.id);
  const intentClass = `intent-${item.restore_intent || 'unreviewed'}`;
  const prepStatus = item.prep_status === 'ready' ? 'ready' : 'todo';
  const isNewClass = item.is_new ? 'row-newly-added' : '';

  const machinesById = new Map();
  for (const m of item.machines) {
    const mid = m.machine_id || '';
    if (!machinesById.has(mid)) machinesById.set(mid, []);
    machinesById.get(mid).push(m.install_location || m.path || '未记录路径');
  }
  const machineBadges = Array.from(machinesById.entries()).map(([mid, paths]) => {
    const alias = getMachineDisplayName(mid);
    const title = [`设备ID: ${escapeHtml(mid)}`, ...paths.map(p => `路径: ${escapeHtml(p)}`)].join('&#10;');
    return `<span class="badge-machine" style="--mc:${machineColor(mid)}" title="${title}">${escapeHtml(alias)}</span>`;
  }).join(' ');

  return `
    <tr class="${isSelected ? 'selected' : ''} ${isNewClass}" data-id="${item.id}">
      <td style="text-align: center;">
        <input type="checkbox" class="row-checkbox" data-id="${item.id}" ${isSelected ? 'checked' : ''}>
      </td>
      <td style="text-align: center;">
        <span class="awesome-star ${item.is_awesome ? 'starred' : ''}" data-action="toggle-awesome" data-id="${item.id}" title="${item.is_awesome ? '取消精选' : '设为精选'}">
          ${item.is_awesome ? ICONS.starFilled : ICONS.star}
        </span>
      </td>
      <td>
        <div class="software-name-cell">
          <div class="software-name-line">
            ${appIconHtml(item)}
            <span class="software-title" data-action="open-drawer" data-id="${item.id}">${escapeHtml(item.name)}</span>
          </div>
          <div class="software-tags">
            <span class="tag-cat">${item.category || '未分类'}</span>
            <span class="tag-form">${item.type || 'desktop'}</span>
            ${item.is_new ? '<span style="background:var(--accent-soft);color:var(--accent);padding:1px 5px;border-radius:4px;font-size:10.5px;">新添加</span>' : ''}
          </div>
        </div>
      </td>
      <td>
        <input type="text" class="cell-input" data-field="version" data-id="${item.id}" value="${escapeHtml(item.version || '')}" placeholder="—" style="font-family: monospace;">
      </td>
      <td>${machineBadges}</td>
      <td>
        <select class="badge-select ${intentClass}" data-field="restore_intent" data-id="${item.id}">
          <option value="must" ${item.restore_intent === 'must' ? 'selected' : ''}>必须恢复</option>
          <option value="on_demand" ${item.restore_intent === 'on_demand' ? 'selected' : ''}>用到再装</option>
          <option value="drop" ${item.restore_intent === 'drop' ? 'selected' : ''}>淘汰弃用</option>
          <option value="unreviewed" ${(!item.restore_intent || item.restore_intent === 'unreviewed') ? 'selected' : ''}>待确认</option>
        </select>
      </td>
      <td>
        <select class="strategy-select" data-field="backup_strategy" data-id="${item.id}">
          <option value="none" ${(!item.backup_strategy || item.backup_strategy === 'none') ? 'selected' : ''}>无需操作</option>
          <option value="copy_dir" ${item.backup_strategy === 'copy_dir' ? 'selected' : ''}>保留/压缩目录</option>
          <option value="redownload" ${item.backup_strategy === 'redownload' ? 'selected' : ''}>重新下载</option>
          <option value="sync_account" ${item.backup_strategy === 'sync_account' ? 'selected' : ''}>账号同步</option>
        </select>
      </td>
      <td style="text-align: center;">
        <button type="button" class="prep-badge toggle-mini ${prepStatus === 'ready' ? 'prep-ready' : 'prep-todo'}" data-action="toggle-prep" data-id="${item.id}" title="点击切换：待办 / 就绪">
          <span class="status-dot dot-${prepStatus === 'ready' ? 'ready' : 'unreviewed'}"></span>${prepStatus === 'ready' ? '就绪' : '待办'}
        </button>
      </td>
      <td>
        <div style="display: flex; align-items: center; gap: 6px;">
          <input type="text" class="cell-input" data-field="download_url" data-id="${item.id}" value="${escapeHtml(item.download_url || '')}" placeholder="官网或下载网址...">
          ${item.download_url ? `<span data-action="open-url" data-url="${escapeHtml(item.download_url)}" style="color:var(--accent);cursor:pointer;display:inline-flex;align-items:center;" title="用默认浏览器打开官网">${ICONS.externalLink}</span>` : ''}
        </div>
      </td>
      <td>
        <input type="text" class="cell-input" data-field="config_notes" data-id="${item.id}" value="${escapeHtml(item.config_notes || '')}" placeholder="配置路径、备忘或注意事项...">
      </td>
    </tr>
  `;
}

function renderTable() {
  const filtered = getFilteredSoftware();
  let html = '';
  for (const item of filtered) {
    html += buildRowHtml(item);
  }
  tableBody.innerHTML = html || `<tr><td colspan="10" style="text-align: center; padding: 40px; color: var(--ink-3);">没有匹配的软件项</td></tr>`;
  updateBatchBar();
  if (activeItem) updateDrawerNavigation();
}

// 就地刷新单个表格行（批量 AI 分析时逐条给出反馈，不整体重绘）
function updateRowInPlace(id, flash = false) {
  const item = softwareList.find(s => s.id === id);
  const tr = tableBody.querySelector(`tr[data-id="${id}"]`);
  if (!item || !tr) return;
  const temp = document.createElement('tbody');
  temp.innerHTML = buildRowHtml(item);
  const newTr = temp.firstElementChild;
  if (!newTr) return;
  if (flash) newTr.classList.add('row-analyzed');
  tr.replaceWith(newTr);
}

function updateBatchBar() {
  if (batchLlmRunning) return; // 分析期间由进度逻辑接管文案与进度条
  if (selectedIds.size > 0) {
    batchBar.classList.add('show');
    batchInfo.innerText = `已选 ${selectedIds.size} 项`;
  } else {
    batchBar.classList.remove('show');
  }
}

// 检查是否正在文本输入框内打字
function isTypingInField() {
  const el = document.activeElement;
  if (!el) return false;
  if (el.isContentEditable) return true;
  const tag = el.tagName ? el.tagName.toLowerCase() : '';
  if (tag === 'textarea') return true;
  if (tag === 'input') {
    const type = (el.type || 'text').toLowerCase();
    return ['text', 'search', 'url', 'email', 'password', 'number'].includes(type);
  }
  return false;
}

// 绑定全局快捷键
function bindKeyboardShortcuts() {
  window.addEventListener('keydown', e => {
    const isDrawerOpen = sideDrawer && sideDrawer.classList.contains('show');
    const isModalOpen = !!document.querySelector('.modal-overlay.show');

    // 1. 如果抽屉打开：处理连续切卡与 Esc 关闭
    if (isDrawerOpen) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeDrawer();
        return;
      }
      // PageUp / PageDown 与 Alt+Left / Alt+Right 始终切卡 (无惧 Firefox Quick Find 与输入框焦点)
      if (e.key === 'PageUp' || (e.altKey && e.key === 'ArrowLeft')) {
        e.preventDefault();
        switchDrawerCard(-1);
        return;
      }
      if (e.key === 'PageDown' || (e.altKey && e.key === 'ArrowRight')) {
        e.preventDefault();
        switchDrawerCard(1);
        return;
      }
      // 非打字状态下，按 [ 或 ] 快速切卡，或数字键修改意愿
      if (!isTypingInField()) {
        if (e.key === '[' || e.key === 'BracketLeft') {
          e.preventDefault();
          switchDrawerCard(-1);
          return;
        }
        if (e.key === ']' || e.key === 'BracketRight') {
          e.preventDefault();
          switchDrawerCard(1);
          return;
        }

        // Delete / Backspace：与底部删除按钮共享 armed 态
        if (e.key === 'Delete' || e.key === 'Backspace') {
          e.preventDefault();
          if (window.DeleteConfirm) window.DeleteConfirm.trigger(btnDeleteCurrent);
          return;
        }

        // 数字键 1~4 快速切换当前卡片意愿
        const drawerIntentMap = { '1': 'must', '2': 'on_demand', '3': 'drop', '4': 'unreviewed' };
        if (drawerIntentMap[e.key]) {
          e.preventDefault();
          const targetIntent = drawerIntentMap[e.key];
          document.getElementById('drawerIntent').value = targetIntent;
          document.getElementById('drawerStrategy').value = deriveStrategy(targetIntent, isPortableItem(activeItem));
          document.querySelectorAll('#drawerIntentSegmented .intent-seg-btn').forEach(btn => {
            btn.classList.toggle('active', btn.dataset.intent === targetIntent);
          });
          markDrawerSaving();
          return;
        }

        // 5 键快速切换准备就绪状态
        if (e.key === '5') {
          e.preventDefault();
          const prepEl = document.getElementById('drawerPrepStatus');
          const nextPrep = prepEl.value === 'ready' ? 'todo' : 'ready';
          renderDrawerPrepToggle(nextPrep);
          markDrawerSaving();
          showToast(`已标记为: ${nextPrep === 'ready' ? '已就绪' : '待办'}`);
          return;
        }
      }
      return;
    }

    // 2. 如果模态弹窗打开：只监听 Esc 关闭
    if (isModalOpen) {
      if (e.key === 'Escape') {
        document.querySelectorAll('.modal-overlay.show').forEach(m => m.classList.remove('show'));
      }
      return;
    }

    // 3. 页面常规状态：如果按 '/' 聚焦搜索框
    if (!isTypingInField() && e.key === '/') {
      e.preventDefault();
      searchInput.focus();
      searchInput.select();
      return;
    }

    // 4. 正在打字则不触发表格快捷键
    if (isTypingInField()) return;

    if (selectedIds.size === 0) return;

    const key = e.key;
    const code = e.code;

    if (key === '1' || code === 'Digit1' || code === 'Numpad1') {
      e.preventDefault();
      batchUpdate({ restore_intent: 'must' });
      showToast(`已标记选中的 ${selectedIds.size} 项为 必须恢复`, 'success');
    } else if (key === '2' || code === 'Digit2' || code === 'Numpad2') {
      e.preventDefault();
      batchUpdate({ restore_intent: 'on_demand' });
      showToast(`已标记选中的 ${selectedIds.size} 项为 用到再装`, 'info');
    } else if (key === '3' || code === 'Digit3' || code === 'Numpad3') {
      e.preventDefault();
      batchUpdate({ restore_intent: 'drop' });
      showToast(`已标记选中的 ${selectedIds.size} 项为 淘汰弃用`, 'warning');
    } else if (key === '4' || code === 'Digit4' || code === 'Numpad4') {
      e.preventDefault();
      batchUpdate({ prep_status: 'ready' });
      showToast(`已标记选中的 ${selectedIds.size} 项为 已就绪`, 'success');
    } else if (key === '5' || code === 'Digit5' || code === 'Numpad5') {
      e.preventDefault();
      handleBatchLLM();
    } else if (key === '6' || code === 'Digit6' || code === 'Numpad6') {
      e.preventDefault();
      batchMerge();
    } else if (key === '7' || code === 'Digit7' || code === 'Numpad7') {
      e.preventDefault();
      batchResetDefault();
    } else if (key === 'Escape') {
      e.preventDefault();
      selectedIds.clear();
      renderTable();
      showToast('已取消选择');
    } else if (key === 'Delete' || key === 'Backspace' || code === 'Delete') {
      e.preventDefault();
      // 表格选中项删除，与批量栏删除按钮共享同一个 armed 态
      if (window.DeleteConfirm) window.DeleteConfirm.trigger(btnBatchDelete);
    }
  });
}

// 抽屉导航与连续切卡
function updateDrawerNavigation() {
  const list = getFilteredSoftware();
  if (!activeItem) return;
  const idx = list.findIndex(s => s.id === activeItem.id);

  if (idx !== -1) {
    drawerIndexBadge.textContent = `${idx + 1} / ${list.length}`;
    btnPrevDrawer.disabled = idx <= 0;
    btnNextDrawer.disabled = idx >= list.length - 1;
  } else {
    drawerIndexBadge.textContent = `— / ${list.length}`;
    btnPrevDrawer.disabled = true;
    btnNextDrawer.disabled = true;
  }
}

function switchDrawerCard(delta) {
  flushDrawerSave();
  const list = getFilteredSoftware();
  if (!activeItem || list.length === 0) return;
  const idx = list.findIndex(s => s.id === activeItem.id);
  if (idx === -1) return;
  const nextIdx = idx + delta;
  if (nextIdx >= 0 && nextIdx < list.length) {
    openDrawer(list[nextIdx].id);
  }
}

// 抽屉状态切换按钮渲染 (配置状态 / 就绪进度)
function renderDrawerPrepToggle(status) {
  const isReady = status === 'ready';
  const hidden = document.getElementById('drawerPrepStatus');
  if (hidden) hidden.value = isReady ? 'ready' : 'todo';
  const toggle = document.getElementById('drawerPrepToggle');
  if (toggle) {
    toggle.classList.toggle('prep-ready', isReady);
    toggle.classList.toggle('prep-todo', !isReady);
  }
  const dot = document.getElementById('drawerPrepToggleDot');
  if (dot) dot.className = `status-dot ${isReady ? 'dot-ready' : 'dot-unreviewed'}`;
  const text = document.getElementById('drawerPrepToggleText');
  if (text) text.innerText = isReady ? '就绪' : '待办';
}

function renderDrawerHasConfigToggle(hasConfig) {
  const on = !!hasConfig;
  const hidden = document.getElementById('drawerHasConfig');
  if (hidden) hidden.value = on ? 'true' : 'false';
  const toggle = document.getElementById('drawerHasConfigToggle');
  if (toggle) {
    toggle.classList.toggle('prep-hasconfig', on);
    toggle.classList.toggle('prep-todo', !on);
  }
  const dot = document.getElementById('drawerHasConfigDot');
  if (dot) dot.className = `status-dot ${on ? 'dot-ondemand' : 'dot-unreviewed'}`;
  const text = document.getElementById('drawerHasConfigText');
  if (text) text.innerText = on ? '有配置' : '无配置';
}

// 抽屉实时无感自动保存
function markDrawerSaving() {
  pendingDrawerSave = true;
  if (drawerSaveStatus) {
    drawerSaveStatus.className = 'save-status saving';
    drawerSaveStatus.innerHTML = `<span class="spinner"></span> 保存中...`;
  }
  clearTimeout(autoSaveTimer);
  autoSaveTimer = setTimeout(performDrawerAutoSave, 350);
}

async function performDrawerAutoSave() {
  if (!activeItem) return;
  clearTimeout(autoSaveTimer);
  pendingDrawerSave = false;

  const updates = {
    name: document.getElementById('drawerName').value.trim() || activeItem.name,
    version: document.getElementById('drawerVersion').value.trim(),
    category: document.getElementById('drawerCategory').value,
    type: document.getElementById('drawerType').value,
    prep_status: document.getElementById('drawerPrepStatus').value,
    has_config: document.getElementById('drawerHasConfig').value === 'true',
    download_url: document.getElementById('drawerUrl').value.trim(),
    restore_intent: document.getElementById('drawerIntent').value,
    backup_strategy: document.getElementById('drawerStrategy').value,
    config_notes: document.getElementById('drawerNotes').value.trim(),
    is_awesome: document.getElementById('drawerAwesome').checked,
    awesome_role: document.getElementById('drawerAwesomeRole').value.trim()
  };

  Object.assign(activeItem, updates);
  // 改名可能引入/消除同名条目，实时刷新合并入口。
  renderDrawerMergeBlock();
  const titleEl = document.getElementById('drawerTitle');
  if (titleEl) titleEl.innerText = activeItem.name;

  try {
    await fetch('/api/software/update', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ uuid: activeItem.uuid, updates })
    });
    renderTable();
    await fetchStatus();
    if (drawerSaveStatus) {
      drawerSaveStatus.className = 'save-status';
      drawerSaveStatus.innerHTML = `${ICONS.check} 已同步`;
    }
  } catch (e) {
    console.error('Auto save error:', e);
  }
}

function flushDrawerSave() {
  if (pendingDrawerSave) {
    performDrawerAutoSave();
  }
}

// 侧边抽屉管理
// ---------------------------------------------------------------------------
// 同名条目合并（主页抽屉入口）
// 锚点 = 当前抽屉条目；谓词与后端 find_known 对齐：名称去空白 + 小写后相等。
// ---------------------------------------------------------------------------

const MERGE_INTENT_LABEL = {
  must: '必须恢复', on_demand: '用到再装',
  drop: '淘汰弃用', unreviewed: '待确认'
};

function sameNameKey(name) {
  return (name || '').trim().toLowerCase();
}

function findSameNameItems(item) {
  if (!item) return [];
  const key = sameNameKey(item.name);
  if (!key) return [];
  return softwareList.filter(s => s.id !== item.id && sameNameKey(s.name) === key);
}

// 是否带有非默认评档（合并后会被丢弃，需弹窗确认）
function hasNonDefaultDecision(item) {
  if (!item) return false;
  const intent = item.restore_intent || 'unreviewed';
  const strategy = item.backup_strategy || 'none';
  const prep = item.prep_status || 'todo';
  return intent !== 'unreviewed' || strategy !== 'none' || prep === 'ready' || !!item.is_awesome;
}

function decisionSummary(item) {
  const parts = ['意愿: ' + (MERGE_INTENT_LABEL[item.restore_intent || 'unreviewed'] || '待确认')];
  const strategy = item.backup_strategy || 'none';
  if (strategy !== 'none') {
    const sm = { copy_dir: '保留/压缩目录', redownload: '重新下载', sync_account: '账号同步' };
    parts.push('处置: ' + (sm[strategy] || strategy));
  }
  if ((item.prep_status || 'todo') === 'ready') parts.push('已就绪');
  if (item.has_config) parts.push('有配置');
  if (item.is_awesome) parts.push('精选');
  return parts.join(' · ');
}

function renderDrawerMergeBlock() {
  const block = document.getElementById('drawerMergeBlock');
  const list = document.getElementById('drawerMergeList');
  if (!block || !list) return;
  block.style.display = 'none';
  list.innerHTML = '';

  const items = findSameNameItems(activeItem);
  if (items.length === 0) return;

  block.style.display = '';
  const count = document.getElementById('drawerMergeCount');
  if (count) count.innerText = String(items.length);
  list.innerHTML = items.map(s => {
    const machineChipsHtml = machineChips(s.machines, m => getMachineDisplayName(m.machine_id));
    const path = (s.machines || []).map(m => m.install_location || m.path).find(Boolean) || '';
    const version = (s.version || '').trim();
    const conflict = hasNonDefaultDecision(s);
    const intent = MERGE_INTENT_LABEL[s.restore_intent || 'unreviewed'] || '待确认';
    return `
      <div class="merge-item">
        <div class="merge-item-info">
          <div class="merge-item-title">
            <span class="merge-item-name">${escapeHtml(s.name)}</span>
            <span class="merge-item-id">${escapeHtml(s.id)}</span>
          </div>
          <div class="merge-item-meta">${machineChipsHtml} · ${escapeHtml(s.type || 'desktop')}${version ? ` · v${escapeHtml(version)}` : ''} · 意愿: ${escapeHtml(intent)}${conflict ? '<span class="merge-item-warn">已评档</span>' : ''}</div>
          ${path ? `<div class="merge-item-path" title="${escapeHtml(path)}">${escapeHtml(path)}</div>` : ''}
        </div>
        <button class="btn btn-secondary btn-sm" type="button" data-action="merge-one" data-id="${escapeHtml(s.id)}">并入</button>
      </div>`;
  }).join('');
}

// 把选中的同名条目并入当前抽屉条目（锚点）。
async function mergeIntoSelf(sourceIds) {
  if (!activeItem || !sourceIds || sourceIds.length === 0) return;
  const sources = sourceIds.map(id => softwareList.find(s => s.id === id)).filter(Boolean);
  if (sources.length === 0) return;

  const conflicts = sources.filter(hasNonDefaultDecision);
  if (conflicts.length > 0) {
    const ok = await confirmMergeConflicts(conflicts);
    if (!ok) return;
  }

  const targetId = activeItem.id;
  const targetUuid = activeItem.uuid;
  const mergeUuids = sources.map(s => s.uuid);
  // 抽屉编辑为防抖自动保存，合并前先落盘，避免读旧 software.json 导致评档回退。
  if (pendingDrawerSave) {
    clearTimeout(autoSaveTimer);
    await performDrawerAutoSave();
  }
  const targetName = activeItem ? activeItem.name : '';

  try {
    const res = await fetch('/api/software/merge', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ targetUuid, mergeUuids })
    });
    const data = await res.json();
    if (!data.success) {
      showToast(data.message || '合并失败', 'error');
      return;
    }
    showToast(`已将 ${mergeUuids.length} 项合并至【${targetName}】`, 'success');
    await loadSoftware();
    openDrawer(targetId);
  } catch (e) {
    showToast('合并请求异常: ' + e.message, 'error');
  }
}

function confirmMergeConflicts(conflicts) {
  const modal = document.getElementById('mergeConfirmModal');
  if (!modal) return Promise.resolve(true);
  const listEl = document.getElementById('mergeConfirmList');
  if (listEl) {
    listEl.innerHTML = conflicts.map(s => `
      <div class="merge-confirm-item">
        <div class="name">${escapeHtml(s.name)} <span class="merge-item-id">${escapeHtml(s.id)}</span></div>
        <div class="detail">${escapeHtml(decisionSummary(s))}</div>
      </div>`).join('');
  }
  modal.classList.add('show');
  return new Promise(resolve => {
    const done = (result) => {
      modal.classList.remove('show');
      modal.removeEventListener('click', onClick);
      window.removeEventListener('keydown', onKey, true);
      resolve(result);
    };
    const onClick = (e) => {
      if (e.target === modal) return done(false);
      if (e.target.closest('#btnMergeConfirm')) return done(true);
      if (e.target.closest('#btnMergeCancel') || e.target.closest('#btnMergeCancelX')) return done(false);
    };
    const onKey = (e) => { if (e.key === 'Escape') { e.preventDefault(); done(false); } };
    modal.addEventListener('click', onClick);
    window.addEventListener('keydown', onKey, true);
  });
}

function openDrawer(id) {
  flushDrawerSave();
  activeItem = softwareList.find(s => s.id === id);
  if (!activeItem) return;

  const idEl = document.getElementById('drawerId');
  if (idEl) idEl.textContent = activeItem.id || 'SW-ITEM';

  const titleEl = document.getElementById('drawerTitle');
  if (titleEl) titleEl.innerText = activeItem.name;

  document.getElementById('drawerName').value = activeItem.name || '';
  const drawerIconWrap = document.getElementById('drawerIconWrap');
  if (drawerIconWrap) drawerIconWrap.innerHTML = appIconHtml(activeItem);
  document.getElementById('drawerVersion').value = activeItem.version || '';
  document.getElementById('drawerCategory').value = activeItem.category || '开发工具';
  document.getElementById('drawerType').value = activeItem.type || 'desktop';
  renderDrawerPrepToggle(activeItem.prep_status || 'todo');
  renderDrawerHasConfigToggle(activeItem.has_config);
  if (drawerVault) drawerVault.setTarget(activeItem.uuid);
  if (window.DeleteConfirm) window.DeleteConfirm.disarmAll();
  document.getElementById('drawerUrl').value = activeItem.download_url || '';

  // 触觉意愿大胶囊
  const activeIntent = activeItem.restore_intent || 'unreviewed';
  document.getElementById('drawerIntent').value = activeIntent;
  document.querySelectorAll('#drawerIntentSegmented .intent-seg-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.intent === activeIntent);
  });

  document.getElementById('drawerStrategy').value = activeItem.backup_strategy || 'none';
  document.getElementById('drawerNotes').value = activeItem.config_notes || '';

  // 紧凑精选
  const isAwesome = !!activeItem.is_awesome;
  const awesomeCheckbox = document.getElementById('drawerAwesome');
  if (awesomeCheckbox) awesomeCheckbox.checked = isAwesome;
  const awesomePill = document.getElementById('drawerAwesomePill');
  if (awesomePill) awesomePill.classList.toggle('active', isAwesome);
  const awesomeExpand = document.getElementById('awesomeExpandContent');
  if (awesomeExpand) awesomeExpand.style.display = isAwesome ? 'flex' : 'none';
  document.getElementById('drawerAwesomeRole').value = activeItem.awesome_role || '';

  if (drawerSaveStatus) {
    drawerSaveStatus.className = 'save-status';
    drawerSaveStatus.innerHTML = `${ICONS.check} 已同步`;
  }

  updateDrawerNavigation();

  const listEl = document.getElementById('drawerMachinesList');
  listEl.innerHTML = activeItem.machines.map(m => `
    <div class="machine-item-card">
      <div style="font-weight: 600; color: var(--accent); display: inline-flex; align-items: center; gap: 5px;">
        <span class="machine-icon" style="--mc:${machineColor(m.machine_id)}">${ICONS.device}</span> ${escapeHtml(getMachineDisplayName(m.machine_id))} <span class="machine-raw-tag">${m.form}</span>
      </div>
      <div>路径: <code>${m.install_location || m.path || '未记录路径'}</code></div>
      ${m.version ? `<div>版本: <code>${m.version}</code></div>` : ''}
      ${m.publisher ? `<div>发布者: ${m.publisher}</div>` : ''}
    </div>
  `).join('') || '<div style="color: var(--ink-3)">暂无关联机器信息</div>';

  renderDrawerMergeBlock();

  sideDrawer.classList.add('show');
  drawerOverlay.classList.add('show');
}

function closeDrawer() {
  flushDrawerSave();
  sideDrawer.classList.remove('show');
  drawerOverlay.classList.remove('show');
  activeItem = null;
}

async function deleteCurrentItem() {
  if (!activeItem) return;
  const name = activeItem.name;
  const id = activeItem.id;
  const uuid = activeItem.uuid;
  softwareList = softwareList.filter(s => s.id !== id);
  await fetch('/api/software/delete', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids: [uuid] })
  });

  closeDrawer();
  renderTable();
  await fetchStatus();
  showToast(`已删除软件【${name}】`, 'info');
}

// 恢复默认：清空决策状态与官网/描述等内容字段
function resetCurrentItem() {
  if (!activeItem) return;
  renderDrawerPrepToggle('todo');
  renderDrawerHasConfigToggle(false);
  document.getElementById('drawerIntent').value = 'unreviewed';
  document.querySelectorAll('#drawerIntentSegmented .intent-seg-btn').forEach(b => {
    b.classList.toggle('active', b.dataset.intent === 'unreviewed');
  });
  document.getElementById('drawerStrategy').value = 'none';
  document.getElementById('drawerUrl').value = '';
  document.getElementById('drawerNotes').value = '';
  markDrawerSaving();
  showToast('已恢复默认（清空决策、官网与描述）', 'info');
}

// 事件绑定
function bindEvents() {
  btnThemeToggle.addEventListener('click', toggleTheme);

  // 机器 Tab 点击
  machineTabs.addEventListener('click', e => {
    const btn = e.target.closest('.tab-btn');
    if (!btn) return;
    activeMachine = btn.dataset.machine;
    document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
    btn.classList.add('active');
    renderTable();
  });

  // 搜索与过滤器
  searchInput.addEventListener('input', () => renderTable());
  categoryFilter.addEventListener('change', () => renderTable());
  intentFilter.addEventListener('change', () => renderTable());
  strategyFilter.addEventListener('change', () => renderTable());
  prepFilter.addEventListener('change', () => renderTable());
  pathFilter.addEventListener('change', () => renderTable());
  awesomeFilter.addEventListener('change', () => renderTable());

  // 统计栏快捷筛选
  document.getElementById('statsBar').addEventListener('click', e => {
    const item = e.target.closest('.stat-item');
    if (!item) return;
    if (item.dataset.filterIntent) {
      intentFilter.value = item.dataset.filterIntent;
      renderTable();
    } else if (item.dataset.filterPrep) {
      prepFilter.value = item.dataset.filterPrep;
      renderTable();
    } else if (item.dataset.filterStrategy) {
      strategyFilter.value = 'all';
      intentFilter.value = 'all';
      softwareList = softwareList.sort((a, b) => {
        const aB = a.backup_strategy === 'copy_dir' || a.backup_strategy === 'copy_config';
        const bB = b.backup_strategy === 'copy_dir' || b.backup_strategy === 'copy_config';
        return bB - aB;
      });
      renderTable();
    } else if (item.dataset.filterAwesome) {
      awesomeFilter.checked = !awesomeFilter.checked;
      renderTable();
    }
  });

  // 全选
  selectAllCheckbox.addEventListener('change', e => {
    const filtered = getFilteredSoftware();
    if (e.target.checked) {
      filtered.forEach(s => selectedIds.add(s.id));
    } else {
      selectedIds.clear();
    }
    renderTable();
  });

  // 表格内部事件代理
  tableBody.addEventListener('change', async e => {
    const target = e.target;
    const id = target.dataset.id;
    if (!id) return;

    if (target.classList.contains('row-checkbox')) {
      if (target.checked) selectedIds.add(id);
      else selectedIds.delete(id);
      const row = target.closest('tr');
      if (row) row.classList.toggle('selected', target.checked);
      updateBatchBar();
      target.blur();
      return;
    }

    if (target.dataset.field === 'restore_intent') {
      const val = target.value;
      const item = softwareList.find(s => s.id === id);
      const updates = { restore_intent: val };
      // 评档即按形态推导处置方式（绿色版压缩目录 / 安装版重新下载），并同步刷新本行下拉。
      if (item) updates.backup_strategy = deriveStrategy(val, isPortableItem(item));
      await updateItemField(id, updates);
      target.className = `badge-select intent-${val}`;
      if (updates.backup_strategy) {
        const stratSel = document.querySelector(`.strategy-select[data-id="${id}"]`);
        if (stratSel) stratSel.value = updates.backup_strategy;
      }
      await fetchStatus();
      return;
    }

    if (target.dataset.field === 'backup_strategy') {
      const val = target.value;
      await updateItemField(id, { backup_strategy: val });
      await fetchStatus();
      return;
    }
  });

  // 单元格失焦保存 (version, download_url, config_notes)
  tableBody.addEventListener('blur', async e => {
    const target = e.target;
    if (target.classList.contains('cell-input')) {
      const id = target.dataset.id;
      const field = target.dataset.field;
      const val = target.value.trim();
      await updateItemField(id, { [field]: val });
    }
  }, true);

  // 点击事件 (详情、星标、就绪状态切换)
  tableBody.addEventListener('click', async e => {
    const star = e.target.closest('[data-action="toggle-awesome"]');
    if (star) {
      const id = star.dataset.id;
      const item = softwareList.find(s => s.id === id);
      if (item) {
        item.is_awesome = !item.is_awesome;
        star.classList.toggle('starred', item.is_awesome);
        star.innerHTML = item.is_awesome ? ICONS.starFilled : ICONS.star;
        await updateItemField(id, { is_awesome: item.is_awesome });
        await fetchStatus();
        showToast(item.is_awesome ? '已加入 Awesome 精选库' : '已取消精选标记');
      }
      return;
    }

    const prepBtn = e.target.closest('[data-action="toggle-prep"]');
    if (prepBtn) {
      const id = prepBtn.dataset.id;
      const item = softwareList.find(s => s.id === id);
      if (item) {
        item.prep_status = item.prep_status === 'ready' ? 'todo' : 'ready';
        const ready = item.prep_status === 'ready';
        // 单行编辑不重渲染整表，这里手动同步按钮外观
        prepBtn.classList.toggle('prep-ready', ready);
        prepBtn.classList.toggle('prep-todo', !ready);
        prepBtn.innerHTML = `<span class="status-dot dot-${ready ? 'ready' : 'unreviewed'}"></span>${ready ? '就绪' : '待办'}`;
        await updateItemField(id, { prep_status: item.prep_status });
        await fetchStatus();
      }
      return;
    }

    const openBtn = e.target.closest('[data-action="open-drawer"]');
    if (openBtn) {
      const id = openBtn.dataset.id;
      openDrawer(id);
      return;
    }

    const linkBtn = e.target.closest('[data-action="open-url"]');
    if (linkBtn) {
      e.preventDefault();
      window.openExternal(linkBtn.dataset.url);
      return;
    }

    // 点击行内任意空白处即可切换勾选，无需精确点中小复选框
    const row = e.target.closest('tr[data-id]');
    if (row && !e.target.closest('input, select, button, a, textarea, [data-action]')) {
      const id = row.dataset.id;
      const cb = row.querySelector('.row-checkbox');
      if (cb) {
        cb.checked = !cb.checked;
        if (cb.checked) selectedIds.add(id); else selectedIds.delete(id);
        row.classList.toggle('selected', cb.checked);
        updateBatchBar();
      }
    }
  });

  // 抽屉字段实时自动保存监听 (无感同步，无需手动点击保存)
  const autoSaveInputs = ['drawerName', 'drawerVersion', 'drawerUrl', 'drawerNotes', 'drawerAwesomeRole'];
  autoSaveInputs.forEach(id => {
    const el = document.getElementById(id);
    if (el) el.addEventListener('input', markDrawerSaving);
  });

  const autoSaveSelects = ['drawerType', 'drawerCategory', 'drawerPrepStatus', 'drawerIntent', 'drawerStrategy', 'drawerAwesome'];
  autoSaveSelects.forEach(id => {
    const el = document.getElementById(id);
    if (el) el.addEventListener('change', markDrawerSaving);
  });

  // 抽屉意愿触觉大胶囊
  const drawerIntentSegmented = document.getElementById('drawerIntentSegmented');
  if (drawerIntentSegmented) {
    drawerIntentSegmented.addEventListener('click', e => {
      const btn = e.target.closest('.intent-seg-btn');
      if (!btn) return;
      const targetIntent = btn.dataset.intent;
      document.getElementById('drawerIntent').value = targetIntent;
      document.getElementById('drawerStrategy').value = deriveStrategy(targetIntent, isPortableItem(activeItem));
      drawerIntentSegmented.querySelectorAll('.intent-seg-btn').forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      markDrawerSaving();
    });
  }

  // 配置状态 / 就绪进度切换按钮
  const drawerPrepToggle = document.getElementById('drawerPrepToggle');
  if (drawerPrepToggle) {
    drawerPrepToggle.addEventListener('click', () => {
      const hidden = document.getElementById('drawerPrepStatus');
      renderDrawerPrepToggle(hidden.value === 'ready' ? 'todo' : 'ready');
      markDrawerSaving();
    });
  }
  const drawerHasConfigToggle = document.getElementById('drawerHasConfigToggle');
  if (drawerHasConfigToggle) {
    drawerHasConfigToggle.addEventListener('click', () => {
      const hidden = document.getElementById('drawerHasConfig');
      renderDrawerHasConfigToggle(hidden.value !== 'true');
      markDrawerSaving();
    });
  }

  // 紧凑精选胶囊点击切换
  const drawerAwesomePill = document.getElementById('drawerAwesomePill');
  const drawerAwesomeEl = document.getElementById('drawerAwesome');
  if (drawerAwesomePill && drawerAwesomeEl) {
    drawerAwesomePill.addEventListener('click', (e) => {
      e.preventDefault();
      drawerAwesomeEl.checked = !drawerAwesomeEl.checked;
      const checked = drawerAwesomeEl.checked;
      drawerAwesomePill.classList.toggle('active', checked);
      const expand = document.getElementById('awesomeExpandContent');
      if (expand) expand.style.display = checked ? 'flex' : 'none';
      if (checked) {
        const roleInput = document.getElementById('drawerAwesomeRole');
        if (roleInput) roleInput.focus();
      }
      markDrawerSaving();
    });
  }

  // 同名条目合并：逐条「并入」与「全部并入」
  const drawerMergeList = document.getElementById('drawerMergeList');
  if (drawerMergeList) {
    drawerMergeList.addEventListener('click', (e) => {
      const btn = e.target.closest('[data-action="merge-one"]');
      if (!btn) return;
      mergeIntoSelf([btn.dataset.id]);
    });
  }
  const drawerMergeAll = document.getElementById('drawerMergeAll');
  if (drawerMergeAll) {
    drawerMergeAll.addEventListener('click', () => {
      mergeIntoSelf(findSameNameItems(activeItem).map(s => s.id));
    });
  }

  // 抽屉官网链接快速打开
  const btnOpenUrl = document.getElementById('btnOpenUrl');
  if (btnOpenUrl) {
    btnOpenUrl.addEventListener('click', () => {
      const url = document.getElementById('drawerUrl').value.trim();
      if (url) window.openExternal(url);
      else showToast('暂无下载或官网链接', 'info');
    });
  }

  // 抽屉切卡按钮
  btnPrevDrawer.addEventListener('click', () => switchDrawerCard(-1));
  btnNextDrawer.addEventListener('click', () => switchDrawerCard(1));

  // 抽屉关闭与删除
  btnCloseDrawer.addEventListener('click', closeDrawer);
  btnCloseDrawerBottom.addEventListener('click', closeDrawer);
  drawerOverlay.addEventListener('click', closeDrawer);
  if (window.DeleteConfirm) window.DeleteConfirm.register(btnDeleteCurrent, deleteCurrentItem);
  const btnResetCurrent = document.getElementById('btnResetCurrent');
  if (btnResetCurrent) btnResetCurrent.addEventListener('click', resetCurrentItem);
  btnDrawerLLM.addEventListener('click', handleDrawerLLM);

  // 批量操作按钮
  btnBatchMust.addEventListener('click', () => {
    batchUpdate({ restore_intent: 'must' });
    showToast('已设为必须恢复', 'success');
  });
  if (btnBatchOnDemand) {
    btnBatchOnDemand.addEventListener('click', () => {
      batchUpdate({ restore_intent: 'on_demand' });
      showToast('已设为用到再装', 'info');
    });
  }
  btnBatchDrop.addEventListener('click', () => {
    batchUpdate({ restore_intent: 'drop' });
    showToast('已设为淘汰弃用', 'warning');
  });
  btnBatchReset.addEventListener('click', batchResetDefault);
  btnBatchReady.addEventListener('click', () => {
    batchUpdate({ prep_status: 'ready' });
    showToast('已标记为已就绪', 'success');
  });
  if (window.DeleteConfirm) window.DeleteConfirm.register(btnBatchDelete, executeBatchDelete);
  btnBatchMerge.addEventListener('click', batchMerge);
  btnBatchLLM.addEventListener('click', handleBatchLLM);

  // 扫描导入弹窗
  const btnImportConfirm = document.getElementById('btnImportConfirm');
  if (btnImportConfirm) btnImportConfirm.addEventListener('click', () => commitImport(getImportCheckedKeys()));
  const btnImportSelectAll = document.getElementById('btnImportSelectAll');
  if (btnImportSelectAll) btnImportSelectAll.addEventListener('click', () => {
    document.querySelectorAll('#importModalBody .import-check').forEach(el => { el.checked = true; });
    updateImportConfirm();
  });
  const btnImportSelectNone = document.getElementById('btnImportSelectNone');
  if (btnImportSelectNone) btnImportSelectNone.addEventListener('click', () => {
    document.querySelectorAll('#importModalBody .import-check').forEach(el => { el.checked = false; });
    updateImportConfirm();
  });
  const importModalBody = document.getElementById('importModalBody');
  if (importModalBody) importModalBody.addEventListener('change', updateImportConfirm);

  // 顶部操作
  btnScanLocal.addEventListener('click', handleScanLocal);
  btnExport.addEventListener('click', handleExport);
  btnAddSoftware.addEventListener('click', () => {
    batchAddInput.value = '';
    batchAddModal.classList.add('show');
    batchAddInput.focus();
  });
  btnConfirmBatchAdd.addEventListener('click', handleConfirmBatchAdd);

  // 配置弹窗与快捷预设
  btnConfig.addEventListener('click', openConfigModal);
  btnSaveConfig.addEventListener('click', saveConfigModal);

  // 设置弹窗左栏分页
  if (configNav) {
    configNav.addEventListener('click', (e) => {
      const btn = e.target.closest('.config-nav-item');
      if (btn) showConfigPane(btn.dataset.pane);
    });
  }
  if (mcpClientSelect) mcpClientSelect.addEventListener('change', renderMcpSnippet);
  if (btnMcpCopy) btnMcpCopy.addEventListener('click', copyMcpSnippet);
  if (mcpDocLink) mcpDocLink.addEventListener('click', (e) => { e.preventDefault(); window.openExternal(REPO_URL + '/blob/master/MCP.md'); });

  // WebDAV 同步（手动）
  if (btnWebdavTest) btnWebdavTest.addEventListener('click', testWebdav);
  if (btnSyncNow) btnSyncNow.addEventListener('click', () => doSync('auto'));
  if (btnSyncAdvanced) btnSyncAdvanced.addEventListener('click', () => {
    const open = !!(syncAdvanced && syncAdvanced.style.display !== 'none');
    if (syncAdvanced) syncAdvanced.style.display = open ? 'none' : 'flex';
    if (syncAdvancedWrap) syncAdvancedWrap.classList.toggle('open', !open);
  });
  if (configWebdavEnabled) configWebdavEnabled.addEventListener('change', toggleWebdavFields);
  if (btnForcePush) btnForcePush.addEventListener('click', () => {
    if (window.confirm('将用本机数据覆盖 WebDAV 远端（远端现状自动备份到 _backup/）。确定继续？')) doSync('push');
  });
  if (btnForcePull) btnForcePull.addEventListener('click', () => {
    if (window.confirm('将用 WebDAV 远端覆盖本机（本机现状自动移入 data/trash/）。确定继续？')) doSync('pull');
  });

  // 关于弹窗
  const btnAbout = document.getElementById('btnAbout');
  if (btnAbout) btnAbout.addEventListener('click', openAboutModal);
  const aboutGitHub = document.getElementById('aboutGitHub');
  if (aboutGitHub) aboutGitHub.addEventListener('click', (e) => { e.preventDefault(); window.openExternal(REPO_URL); });
  const aboutHomepage = document.getElementById('aboutHomepage');
  if (aboutHomepage) aboutHomepage.addEventListener('click', (e) => { e.preventDefault(); window.openExternal(HOMEPAGE_URL); });

  bindPreset('presetLocal', () => {
    configLlmUrl.value = 'http://127.0.0.1:1234/v1/chat/completions';
    configLlmModel.value = 'qwen3.5-4b';
    configLlmKey.value = '';
    showToast('已填入本地 LM Studio 预设', 'info');
  });
  bindPreset('presetDeepseek', () => {
    configLlmUrl.value = 'https://api.deepseek.com/chat/completions';
    configLlmModel.value = 'deepseek-flash';
    configLlmKey.focus();
    showToast('已填入 DeepSeek 预设，请填入 API Key', 'info');
  });
  bindPreset('presetOpenai', () => {
    configLlmUrl.value = 'https://api.openai.com/v1/chat/completions';
    configLlmModel.value = 'gpt-4o-mini';
    configLlmKey.focus();
    showToast('已填入 OpenAI 预设，请填入 API Key', 'info');
  });
  bindPreset('presetSilicon', () => {
    configLlmUrl.value = 'https://api.siliconflow.cn/v1/chat/completions';
    configLlmModel.value = 'Qwen/Qwen2.5-7B-Instruct';
    configLlmKey.focus();
    showToast('已填入硅基流动预设，请填入 API Key', 'info');
  });
}

// 安全绑定预设按钮 (元素可能不存在)
function bindPreset(id, handler) {
  const el = document.getElementById(id);
  if (el) el.addEventListener('click', handler);
}

// 单项更新
async function updateItemField(id, updates) {
  const item = softwareList.find(s => s.id === id);
  if (item) Object.assign(item, updates);

  try {
    await fetch('/api/software/update', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ uuid: item ? item.uuid : '', updates })
    });
  } catch (e) {
    showToast('保存失败: ' + e.message, 'error');
  }
}

// 批量修改
async function batchUpdate(updates) {
  if (selectedIds.size === 0) return;
  const ids = softwareList.filter(s => selectedIds.has(s.id)).map(s => s.uuid);

  // 意愿变更会由后端按形态推导处置方式，本地不能只套用 updates，否则下拉会不同步。
  const derivesStrategy = updates.restore_intent && !updates.backup_strategy;
  if (!derivesStrategy) {
    for (const s of softwareList) {
      if (selectedIds.has(s.id)) Object.assign(s, updates);
    }
  }

  await fetch('/api/software/batch-update', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids, updates })
  });

  if (derivesStrategy) await loadSoftware();

  // 保留勾选，方便连续批量设置多项（按 Esc 可取消选择）
  await fetchStatus();
  renderTable();
  updateBatchBar();
}

// 恢复默认 / 重置
function batchResetDefault() {
  if (selectedIds.size === 0) return;
  const count = selectedIds.size;
  batchUpdate({
    restore_intent: 'unreviewed',
    backup_strategy: 'none',
    prep_status: 'todo',
    has_config: false,
    download_url: '',
    config_notes: ''
  });
  showToast(`已重置 ${count} 项（含官网与描述已清空）`, 'info');
}

// 执行批量删除
async function executeBatchDelete() {
  if (selectedIds.size === 0) return;
  const count = selectedIds.size;
  const ids = softwareList.filter(s => selectedIds.has(s.id)).map(s => s.uuid);
  softwareList = softwareList.filter(s => !selectedIds.has(s.id));

  await fetch('/api/software/delete', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids })
  });

  selectedIds.clear();
  await fetchStatus();
  renderTable();
  showToast(`已成功删除 ${count} 个软件条目`, 'info');
}

// 批量合并
async function batchMerge() {
  if (selectedIds.size < 2) {
    showToast('请勾选至少两项要合并的软件条目', 'warning');
    return;
  }
  const items = Array.from(selectedIds)
    .map(id => softwareList.find(s => s.id === id))
    .filter(Boolean);
  const targetItem = items[0];
  const targetUuid = targetItem.uuid;
  const mergeUuids = items.slice(1).map(s => s.uuid);

  const res = await fetch('/api/software/merge', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ targetUuid, mergeUuids })
  });
  const data = await res.json();
  if (data.success) {
    selectedIds.clear();
    await loadSoftware();
    await fetchStatus();
    showToast(`已将 ${mergeUuids.length} 项合并至【${targetItem.name}】`, 'success');
  }
}

// LLM 批量智能推断
async function handleBatchLLM() {
  // 运行中再次点击 = 请求停止
  if (batchLlmRunning) {
    batchLlmAbort = true;
    return;
  }
  if (selectedIds.size === 0) {
    showToast('请先勾选需要 AI 预判的软件', 'warning');
    return;
  }

  const ids = Array.from(selectedIds);
  const total = ids.length;
  const originalHtml = btnBatchLLM.innerHTML;
  const originalTitle = btnBatchLLM.title;

  batchLlmRunning = true;
  batchLlmAbort = false;
  btnBatchLLM.classList.add('is-running');
  btnBatchLLM.title = '点击停止 AI 预判';
  batchProgress.hidden = false;
  batchProgressFill.style.width = '0%';

  let successCount = 0;
  let failCount = 0;
  let done = 0;

  const renderProgress = () => {
    const pct = total ? Math.round((done / total) * 100) : 0;
    batchProgressFill.style.width = `${pct}%`;
    batchInfo.innerText = batchLlmAbort
      ? `已停止 · ${done} / ${total}`
      : `AI 预判中 ${done} / ${total}`;
    btnBatchLLM.innerHTML = `<span class="spinner"></span> ${done}/${total} · ${batchLlmAbort ? '停止中' : '停止'}`;
  };
  renderProgress();

  try {
    for (const id of ids) {
      if (batchLlmAbort) break;
      const item = softwareList.find(s => s.id === id);
      if (!item) { done++; renderProgress(); continue; }
      const paths = item.machines.map(m => m.install_location || m.path).filter(Boolean).join('; ');

      try {
        const res = await fetch('/api/llm/analyze', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ id, name: item.name, paths, category: item.category })
        });
        const data = await res.json();
        if (data.success && data.suggestion) {
          const sug = data.suggestion;
          const updates = {};
          if (sug.category) updates.category = sug.category;
          if (sug.type) updates.type = sug.type;
          if (sug.restore_intent) {
            updates.restore_intent = sug.restore_intent;
            // 处置方式不再由 LLM 判断，按意愿 + 形态推导。
            updates.backup_strategy = deriveStrategy(sug.restore_intent, isPortableItem(item));
          }
          if (sug.download_url) updates.download_url = sug.download_url;
          if (sug.config_notes) updates.config_notes = sug.config_notes;

          Object.assign(item, updates);
          await updateItemField(id, updates);
          successCount++;
          updateRowInPlace(id, true);
        } else {
          failCount++;
        }
      } catch (err) {
        console.error(`AI analyze failed for ${item.name}:`, err);
        failCount++;
      }

      done++;
      renderProgress();
    }

    selectedIds.clear();
    await fetchStatus();
    renderTable();
    if (batchLlmAbort) {
      showToast(`已停止：更新 ${successCount} 项，失败 ${failCount} 项`, 'warning');
    } else {
      showToast(`AI 预判完成：更新 ${successCount} 项${failCount ? `，失败 ${failCount} 项` : ''}`, 'success');
    }
  } finally {
    batchLlmRunning = false;
    batchLlmAbort = false;
    batchProgress.hidden = true;
    batchProgressFill.style.width = '0%';
    btnBatchLLM.classList.remove('is-running');
    btnBatchLLM.disabled = false;
    btnBatchLLM.title = originalTitle;
    btnBatchLLM.innerHTML = originalHtml;
    updateBatchBar();
  }
}

// 抽屉单条 AI 补全
async function handleDrawerLLM() {
  if (!activeItem) return;
  btnDrawerLLM.disabled = true;
  btnDrawerLLM.innerHTML = '<span class="spinner"></span> 思考中...';

  const paths = activeItem.machines.map(m => m.install_location || m.path).filter(Boolean).join('; ');
  try {
    const res = await fetch('/api/llm/analyze', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: activeItem.id, name: activeItem.name, paths, category: activeItem.category })
    });
    const data = await res.json();
    if (data.success && data.suggestion) {
      const sug = data.suggestion;
      if (sug.category) document.getElementById('drawerCategory').value = sug.category;
      if (sug.type) document.getElementById('drawerType').value = sug.type;
      if (sug.restore_intent) {
        document.getElementById('drawerIntent').value = sug.restore_intent;
        document.querySelectorAll('#drawerIntentSegmented .intent-seg-btn').forEach(btn => {
          btn.classList.toggle('active', btn.dataset.intent === sug.restore_intent);
        });
        // 处置方式不再由 LLM 判断，按意愿 + 形态推导。
        document.getElementById('drawerStrategy').value = deriveStrategy(sug.restore_intent, isPortableItem(activeItem));
      }
      if (sug.download_url) {
        document.getElementById('drawerUrl').value = sug.download_url;
      }
      if (sug.config_notes) {
        document.getElementById('drawerNotes').value = sug.config_notes;
      }
      markDrawerSaving();
      showToast('AI 预填完成，已自动保存！', 'success');
    } else {
      showToast('AI 建议生成异常: ' + (data.error || '未知错误'), 'warning');
    }
  } catch (e) {
    showToast('调用失败: ' + e.message, 'error');
  } finally {
    btnDrawerLLM.disabled = false;
    btnDrawerLLM.innerHTML = `${ICONS.sparkles} AI预填`;
  }
}

// 本机扫描：采集 -> 预览候选 -> 勾选导入
async function handleScanLocal() {
  btnScanLocal.disabled = true;
  scanIcon.innerHTML = '<span class="spinner"></span>';
  showToast('正在启动 Windows 采集脚本扫描本机...', 'info');

  try {
    const res = await fetch('/api/scan/preview', { method: 'POST' });
    const data = await res.json();
    if (!data.success) {
      showToast('扫描执行失败: ' + data.error, 'error');
      return;
    }
    const candidates = data.candidates || [];
    if (candidates.length === 0) {
      await fetchStatus();
      showToast('本机没有发现新的软件条目', 'info');
      return;
    }

    // 首次运行 / 清单为空：视为全量重建，直接导入全部候选（含“之前已删除”），无需弹窗
    const wasEmpty = data.was_empty !== undefined ? data.was_empty : softwareList.length === 0;
    if (wasEmpty) {
      const keys = candidates.map(c => c.key);
      await commitImport(keys);
      return;
    }

    openImportModal(data);
  } catch (e) {
    showToast('扫描请求异常: ' + e.message, 'error');
  } finally {
    btnScanLocal.disabled = false;
    scanIcon.innerHTML = ICONS.refresh;
  }
}

// 扫描导入弹窗
function importReasonLabel(kind) {
  if (kind === 'deleted_before') return { text: '之前已删除', cls: 'import-badge-deleted' };
  return { text: '新发现', cls: 'import-badge-new' };
}

function openImportModal(data) {
  const candidates = data.candidates || [];
  const s = data.summary || {};
  const body = document.getElementById('importModalBody');
  const rows = candidates.map(c => {
    const reason = importReasonLabel(c.kind);
    const paths = (c.machines || []).map(m => m.install_location || m.path || '').filter(Boolean).join('  ·  ');
    const checked = c.kind === 'new' ? 'checked' : '';
    return `
      <label class="import-row">
        <input type="checkbox" class="import-check" data-key="${escapeHtml(c.key)}" data-kind="${escapeHtml(c.kind)}" ${checked}>
        <div class="import-row-main">
          <div class="import-row-title">
            <strong>${escapeHtml(c.name)}</strong>
            <span class="tag-cat">${escapeHtml(c.category || '未分类')}</span>
            <span class="tag-form">${escapeHtml(c.type || 'desktop')}</span>
            <span class="import-badge ${reason.cls}">${reason.text}</span>
          </div>
          ${paths ? `<div class="import-row-paths" title="${escapeHtml(paths)}">${escapeHtml(paths)}</div>` : ''}
        </div>
      </label>
    `;
  }).join('');

  body.innerHTML = `
    <p style="font-size: 12.5px; color: var(--ink-2);">
      共发现 <strong>${s.total || candidates.length}</strong> 项：新增 <strong>${s.new || 0}</strong>、之前已删除 <strong>${s.deleted_before || 0}</strong>。
      <span style="color: var(--ink-3);">已存在的条目不会被改动。</span>
    </p>
    <div class="import-list">${rows}</div>
  `;
  document.getElementById('importModal').classList.add('show');
  updateImportConfirm();
}

function getImportCheckedKeys() {
  return Array.from(document.querySelectorAll('#importModalBody .import-check:checked')).map(el => el.dataset.key);
}

function updateImportConfirm() {
  const n = getImportCheckedKeys().length;
  const btn = document.getElementById('btnImportConfirm');
  if (btn) {
    btn.disabled = n === 0;
    btn.innerText = `导入选中 (${n})`;
  }
}

async function commitImport(keys) {
  try {
    const res = await fetch('/api/scan/commit', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ selectedKeys: keys })
    });
    const data = await res.json();
    if (!data.success) {
      showToast('导入失败: ' + data.error, 'error');
      return;
    }
    document.getElementById('importModal').classList.remove('show');
    await loadSoftware();
    await fetchStatus();
    showToast(`已导入 ${data.added} 项${data.revived ? `，复活 ${data.revived} 项` : ''}`, 'success');
  } catch (e) {
    showToast('导入异常: ' + e.message, 'error');
  }
}

async function saveExportFile(which) {
  if (!window.__lastExportData) return;
  const isChecklist = which === 'checklist';
  const filename = isChecklist
    ? (window.__lastExportData.checklistFilename || 'RECOVERY_CHECKLIST.md')
    : (window.__lastExportData.awesomeFilename || 'AWESOME_LIST.md');
  if (typeof window.dialogSave !== 'function') {
    showToast('当前环境不支持另存为', 'warning');
    return;
  }
  const dest = await window.dialogSave({
    defaultPath: filename,
    title: '保存清单到…',
    filters: [{ name: 'Markdown', extensions: ['md'] }]
  });
  if (!dest) return;
  try {
    const res = await fetch('/api/export/save', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ which, dest })
    });
    const data = await res.json();
    if (data.success) showToast('已保存: ' + dest, 'success');
    else showToast('保存失败: ' + (data.error || ''), 'error');
  } catch (e) {
    showToast('保存异常: ' + e.message, 'error');
  }
}

window.saveExportFile = saveExportFile;

// 导出软件清单为 Excel 表格（.xlsx）
async function saveXlsxFile() {
  if (typeof window.dialogSave !== 'function') {
    showToast('当前环境不支持另存为', 'warning');
    return;
  }
  const dest = await window.dialogSave({
    defaultPath: 'SOFTWARE_LEDGER.xlsx',
    title: '保存 Excel 表格到…',
    filters: [{ name: 'Excel 工作簿', extensions: ['xlsx'] }]
  });
  if (!dest) return;
  try {
    const res = await fetch('/api/export/xlsx', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ dest })
    });
    const data = await res.json();
    if (data.success) showToast('已保存: ' + dest, 'success');
    else showToast('保存失败: ' + (data.error || ''), 'error');
  } catch (e) {
    showToast('保存异常: ' + e.message, 'error');
  }
}

window.saveXlsxFile = saveXlsxFile;

// 导出 Markdown
async function handleExport() {
  btnExport.disabled = true;
  const oldText = btnExport.innerHTML;
  btnExport.innerHTML = '<span class="spinner"></span> 导出中...';
  try {
    const res = await fetch('/api/export', { method: 'POST' });
    const data = await res.json();
    if (data.success) {
      window.__lastExportData = data;

      const modalBody = document.getElementById('exportModalBody');
      modalBody.innerHTML = `
        <p style="margin-bottom: 12px; color: var(--ink-2);">
          清单已生成，点击右侧按钮<strong>选择保存路径</strong>：
        </p>
        <div style="display: flex; flex-direction: column; gap: 8px; margin-bottom: 14px;">
          <div style="background: var(--surface-2); padding: 10px 14px; border-radius: 6px; box-shadow: inset 0 0 0 1px var(--rule); display: flex; justify-content: space-between; align-items: center;">
            <div>
              <div style="font-weight: 600; color: var(--ink);">📋 重装恢复备忘清单</div>
              <div style="font-size: 11.5px; color: var(--ink-3); font-family: var(--mono);">${escapeHtml(data.checklistFilename || 'RECOVERY_CHECKLIST.md')}</div>
            </div>
            <button class="btn btn-secondary btn-sm" onclick="saveExportFile('checklist')">
              <svg class="i sm" viewBox="0 0 24 24"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"></path><polyline points="17 21 17 13 7 13 7 21"></polyline><polyline points="7 3 7 8 15 8"></polyline></svg> 另存为…
            </button>
          </div>
          <div style="background: var(--surface-2); padding: 10px 14px; border-radius: 6px; box-shadow: inset 0 0 0 1px var(--rule); display: flex; justify-content: space-between; align-items: center;">
            <div>
              <div style="font-weight: 600; color: var(--ink);">⭐ 个人精选资产库</div>
              <div style="font-size: 11.5px; color: var(--ink-3); font-family: var(--mono);">${escapeHtml(data.awesomeFilename || 'AWESOME_LIST.md')}</div>
            </div>
            <button class="btn btn-secondary btn-sm" onclick="saveExportFile('awesome')">
              <svg class="i sm" viewBox="0 0 24 24"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"></path><polyline points="17 21 17 13 7 13 7 21"></polyline><polyline points="7 3 7 8 15 8"></polyline></svg> 另存为…
            </button>
          </div>
          <div style="background: var(--surface-2); padding: 10px 14px; border-radius: 6px; box-shadow: inset 0 0 0 1px var(--rule); display: flex; justify-content: space-between; align-items: center;">
            <div>
              <div style="font-weight: 600; color: var(--ink);">📊 Excel 软件表格</div>
              <div style="font-size: 11.5px; color: var(--ink-3); font-family: var(--mono);">SOFTWARE_LEDGER.xlsx</div>
            </div>
            <button class="btn btn-secondary btn-sm" onclick="saveXlsxFile()">
              <svg class="i sm" viewBox="0 0 24 24"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"></path><polyline points="17 21 17 13 7 13 7 21"></polyline><polyline points="7 3 7 8 15 8"></polyline></svg> 另存为…
            </button>
          </div>
        </div>
      `;
      document.getElementById('exportModal').classList.add('show');
    } else {
      showToast('导出失败: ' + (data.error || data.message), 'error');
    }
  } catch (e) {
    showToast('导出异常: ' + e.message, 'error');
  } finally {
    btnExport.disabled = false;
    btnExport.innerHTML = oldText;
  }
}

// 批量添加软件
async function handleConfirmBatchAdd() {
  const text = batchAddInput.value.trim();
  if (!text) {
    showToast('请输入要添加的软件名称', 'warning');
    return;
  }

  const rawNames = text.split(/[\n,，;；]+/).map(s => s.trim()).filter(Boolean);
  if (rawNames.length === 0) {
    showToast('未检测到有效的软件名称', 'warning');
    return;
  }

  const restore_intent = batchAddIntent.value;
  btnConfirmBatchAdd.disabled = true;

  try {
    const res = await fetch('/api/software/batch-add', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ names: rawNames, restore_intent })
    });
    const data = await res.json();
    if (data.success) {
      batchAddModal.classList.remove('show');
      batchAddInput.value = '';

      for (const item of data.items) {
        softwareList.unshift(item);
      }

      categoryFilter.value = 'all';
      intentFilter.value = 'all';
      strategyFilter.value = 'all';
      prepFilter.value = 'all';

      renderTable();
      await fetchStatus();

      if (data.items.length === 1) {
        openDrawer(data.items[0].id);
      } else {
        showToast(`已成功新增 ${data.items.length} 个软件，并在顶部高亮！`, 'success');
      }
    }
  } catch (e) {
    showToast('添加失败: ' + e.message, 'error');
  } finally {
    btnConfirmBatchAdd.disabled = false;
  }
}

// 关于弹窗
function openAboutModal() {
  document.getElementById('aboutModal').classList.add('show');
}

// ---- Agent 集成 (MCP)：为常见客户端生成配置片段 ----
// 各客户端配置格式不一：Claude/Cursor/Windsurf 用 mcpServers，VS Code 用 servers，Codex 用 TOML。
const MCP_CLIENTS = {
  'claude-code-cmd': '在终端执行（写入 ~/.claude.json；加 -s project 可写入项目 .mcp.json）',
  'claude-code-json': '项目根目录 .mcp.json',
  'codex': '%USERPROFILE%\\.codex\\config.toml',
  'pi': '在终端执行（写入用户级 ~/.pi/agent/mcp.json；也可写项目 .pi/mcp.json）',
  'cursor': '%USERPROFILE%\\.cursor\\mcp.json（或项目 .cursor/mcp.json）',
  'claude-desktop': '%APPDATA%\\Claude\\claude_desktop_config.json',
  'vscode': '.vscode/mcp.json',
  'generic': '任意使用 mcpServers 格式的客户端（Windsurf / Cline / Roo 等）',
};

function mcpSnippetText(client) {
  // 路径统一用正斜杠：Windows 可接受，且不用操心 JSON / TOML 的反斜杠转义。
  const exe = ((mcpInfo && mcpInfo.exe_path) || 'D:/Tools/windows-software-ledger.exe').replace(/\\/g, '/');
  const name = (mcpInfo && mcpInfo.server_name) || 'software-ledger';
  const stdio = { command: exe, args: ['--mcp'] };
  switch (client) {
    case 'claude-code-cmd':
      return `claude mcp add ${name} -- "${exe}" --mcp`;
    case 'codex':
      return `[mcp_servers.${name}]\ncommand = "${exe}"\nargs = ["--mcp"]`;
    case 'pi':
      return `pi mcp add ${name} -- "${exe}" --mcp`;
    case 'vscode':
      return JSON.stringify({ servers: { [name]: Object.assign({ type: 'stdio' }, stdio) } }, null, 2);
    case 'claude-code-json':
    case 'cursor':
    case 'claude-desktop':
    case 'generic':
    default:
      return JSON.stringify({ mcpServers: { [name]: stdio } }, null, 2);
  }
}

function renderMcpSnippet() {
  if (!mcpSnippet || !mcpClientSelect) return;
  const client = mcpClientSelect.value;
  mcpSnippet.textContent = mcpSnippetText(client);
  if (mcpTargetHint) mcpTargetHint.textContent = '配置文件：' + (MCP_CLIENTS[client] || '');
}

async function loadMcpInfo() {
  if (mcpLoaded) return;
  try {
    mcpInfo = await (await fetch('/api/mcp')).json();
    mcpLoaded = true;
  } catch (e) { /* 纯浏览器模式下没有该接口，忽略 */ }
}

// 设置弹窗左栏分页：切换面板；切到 MCP 时按需加载配置片段。
function showConfigPane(name) {
  if (configNav) {
    configNav.querySelectorAll('.config-nav-item').forEach((b) => {
      b.classList.toggle('active', b.dataset.pane === name);
    });
  }
  document.querySelectorAll('#configModal .config-pane').forEach((p) => {
    p.classList.toggle('active', p.dataset.pane === name);
  });
  if (name === 'mcp') loadMcpInfo().then(renderMcpSnippet);
}

async function copyMcpSnippet() {
  if (!mcpSnippet) return;
  const text = mcpSnippet.textContent || '';
  let ok = false;
  try {
    await navigator.clipboard.writeText(text);
    ok = true;
  } catch (e) {
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.style.position = 'fixed';
    ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    try { ok = document.execCommand('copy'); } catch (_) { ok = false; }
    document.body.removeChild(ta);
  }
  showToast(ok ? 'MCP 配置已复制' : '复制失败，请手动选择复制', ok ? 'success' : 'error');
}

// 配置弹窗管理
async function openConfigModal() {
  try {
    const res = await fetch('/api/config');
    const cfg = await res.json();
    configLlmUrl.value = cfg.llm_url || '';
    configLlmModel.value = cfg.llm_model || '';
    configLlmKey.value = cfg.llm_api_key || '';
    if (configMcpWrite) configMcpWrite.checked = !!cfg.mcp_write_enabled;
    const scanDirsEl = document.getElementById('configScanDirs');
    scanDirsByMachine = (cfg.scan_directories && !Array.isArray(cfg.scan_directories)) ? cfg.scan_directories : {};
    if (scanDirsEl) {
      scanDirsEl.value = ((scanDirsByMachine[currentMachine] || []).join('\n'));
    }

    // 渲染机器别名列表
    const aliasesListEl = document.getElementById('configMachineAliasesList');
    if (aliasesListEl) {
      const aliases = cfg.machine_aliases || {};
      machineAliases = aliases;
      const allKnownMachines = Array.from(new Set([...machinesList, ...Object.keys(aliases)]));
      aliasesListEl.innerHTML = allKnownMachines.map(mid => `
        <div class="machine-alias-row">
          <span class="machine-alias-id">${machineDeviceIcon(mid)} ${escapeHtml(mid)}</span>
          <input type="text" class="machine-alias-input" data-mid="${escapeHtml(mid)}" value="${escapeHtml(aliases[mid] || '')}" placeholder="设置友好别名 (如：主力台式机 / 便携本)">
        </div>
      `).join('');
    }

    // 读取 WebDAV 配置（data/webdav.json，仅本地）
    try {
      const wd = await (await fetch('/api/webdav/config')).json();
      if (configWebdavEnabled) configWebdavEnabled.checked = !!wd.enabled;
      if (configWebdavUrl) configWebdavUrl.value = wd.url || '';
      if (configWebdavUser) configWebdavUser.value = wd.username || '';
      if (configWebdavPass) configWebdavPass.value = wd.password || '';
      toggleWebdavFields();
      applyWebdavUi(!!wd.enabled && !!wd.url);
      await refreshSyncStatus();
    } catch (e) { /* 忽略：未配置时也允许打开设置 */ }

    configModal.classList.add('show');
  } catch (e) {
    showToast('读取配置失败', 'error');
  }
}

async function saveConfigModal() {
  const llm_url = configLlmUrl.value.trim();
  const llm_model = configLlmModel.value.trim();
  const llm_api_key = configLlmKey.value.trim();
  const scanDirsEl = document.getElementById('configScanDirs');
  const dirs = scanDirsEl ? scanDirsEl.value.split('\n').map(s => s.trim()).filter(Boolean) : [];
  // 扫描目录按主机名分键，保留其它机器的条目
  const scan_directories = Object.assign({}, scanDirsByMachine);
  if (currentMachine) scan_directories[currentMachine] = dirs;
  
  const machine_aliases = {};
  document.querySelectorAll('#configMachineAliasesList .machine-alias-input').forEach(input => {
    const mid = input.dataset.mid;
    const val = input.value.trim();
    if (mid && val) {
      machine_aliases[mid] = val;
    }
  });

  // LLM 为可选能力：留空即关闭 AI 预判，不应阻碍保存扫描目录 / 设备别名等设置。
  // 仅在填了地址时做基本格式校验，避免存入明显错误的地址。
  if (llm_url && !/^https?:\/\//i.test(llm_url)) {
    showToast('LLM 地址需以 http:// 或 https:// 开头，或留空以关闭 AI 预判', 'warning');
    return;
  }

  try {
    const res = await fetch('/api/config', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        llm_url, llm_model, llm_api_key, scan_directories, machine_aliases,
        mcp_write_enabled: !!(configMcpWrite && configMcpWrite.checked)
      })
    });
    const data = await res.json();
    if (data.success) {
      // 单独保存 WebDAV 凭据（本地文件，不进 config.json）
      try {
        await fetch('/api/webdav/config', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            enabled: !!(configWebdavEnabled && configWebdavEnabled.checked),
            url: configWebdavUrl ? configWebdavUrl.value.trim() : '',
            username: configWebdavUser ? configWebdavUser.value.trim() : '',
            password: configWebdavPass ? configWebdavPass.value : ''
          })
        });
      } catch (e) {}
      scanDirsByMachine = scan_directories;
      machineAliases = machine_aliases;
      configModal.classList.remove('show');
      renderTable();
      renderMachineTabs(machinesList);
      showToast('设置与设备别名已保存生效！', 'success');
      // 若刚启用 WebDAV，立即开始会话同步
      await initSync();
      // 开关了 MCP 写入则开始/停止台账轮询
      await syncLedgerWatch();
    }
  } catch (e) {
    showToast('保存失败: ' + e.message, 'error');
  }
}

function escapeHtml(str) {
  if (!str) return '';
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

window.addEventListener('DOMContentLoaded', init);
