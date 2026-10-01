// client/app.js
// 软件备份台账前端交互逻辑：统一线性 SVG 图标、卡片连续切换、实时无感自动保存、云端/本地 LLM 支持

let softwareList = [];
let machinesList = [];
let availablePaths = [];
let selectedIds = new Set();
let activeMachine = 'all';
let activeItem = null;
let machineAliases = {};
let lastDeleteTime = 0; // 用于双击 Delete 防误触

function getMachineDisplayName(id) {
  if (!id) return '未知设备';
  return machineAliases[id] || id;
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
const btnBatchShould = document.getElementById('btnBatchShould');
const btnBatchOnDemand = document.getElementById('btnBatchOnDemand');
const btnBatchDrop = document.getElementById('btnBatchDrop');
const btnBatchReset = document.getElementById('btnBatchReset');
const btnBatchReady = document.getElementById('btnBatchReady');
const btnBatchConfig = document.getElementById('btnBatchConfig');
const batchConfigLabel = document.getElementById('batchConfigLabel');
const btnBatchMerge = document.getElementById('btnBatchMerge');
const btnBatchDelete = document.getElementById('btnBatchDelete');
const btnBatchLLM = document.getElementById('btnBatchLLM');

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
  await loadSoftware();
  bindEvents();
  bindKeyboardShortcuts();
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
    availablePaths = data.availablePaths || [];
    renderStats(data.stats);
    renderMachineTabs(data.machines);
    renderPathFilter(availablePaths);
    if (softwareList.length > 0) {
      renderTable();
    }
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

function renderStats(stats) {
  if (!stats) return;
  document.getElementById('statTotal').innerText = stats.total || 0;
  document.getElementById('statMust').innerText = stats.must || 0;
  document.getElementById('statShould').innerText = stats.should || 0;
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
    html += `<button class="tab-btn ${activeMachine === m ? 'active' : ''}" data-machine="${m}" title="设备ID: ${m}">${escapeHtml(displayName)} (${count})</button>`;
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
      const pathMatch = item.machines.some(m => (m.install_location || m.path || '').toLowerCase().includes(query));
      if (!nameMatch && !verMatch && !urlMatch && !notesMatch && !pathMatch) return false;
    }

    return true;
  });
}

// 渲染表格
function renderTable() {
  const filtered = getFilteredSoftware();
  let html = '';

  for (const item of filtered) {
    const isSelected = selectedIds.has(item.id);
    const intentClass = `intent-${item.restore_intent || 'unreviewed'}`;
    const prepStatus = item.prep_status === 'ready' ? 'ready' : 'todo';
    const isNewClass = item.is_new ? 'row-newly-added' : '';

    const machineBadges = item.machines.map(m => {
      const alias = getMachineDisplayName(m.machine_id);
      return `<span class="badge-machine" title="设备ID: ${m.machine_id}&#10;路径: ${m.install_location || m.path || '未记录路径'}">${escapeHtml(alias)}</span>`;
    }).join(' ');

    html += `
      <tr class="${isSelected ? 'selected' : ''} ${isNewClass}" data-id="${item.id}">
        <td>
          <input type="checkbox" class="row-checkbox" data-id="${item.id}" ${isSelected ? 'checked' : ''}>
        </td>
        <td style="text-align: center;">
          <span class="awesome-star ${item.is_awesome ? 'starred' : ''}" data-action="toggle-awesome" data-id="${item.id}" title="${item.is_awesome ? '取消精选' : '设为精选'}">
            ${item.is_awesome ? ICONS.starFilled : ICONS.star}
          </span>
        </td>
        <td>
          <div class="software-name-cell">
            <span class="software-title" data-action="open-drawer" data-id="${item.id}">${escapeHtml(item.name)}</span>
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
            <option value="should" ${item.restore_intent === 'should' ? 'selected' : ''}>建议恢复</option>
            <option value="on_demand" ${item.restore_intent === 'on_demand' ? 'selected' : ''}>用到再装</option>
            <option value="drop" ${item.restore_intent === 'drop' ? 'selected' : ''}>淘汰弃用</option>
            <option value="unreviewed" ${(!item.restore_intent || item.restore_intent === 'unreviewed') ? 'selected' : ''}>待确认</option>
          </select>
        </td>
        <td>
          <select class="strategy-select" data-field="backup_strategy" data-id="${item.id}">
            <option value="none" ${(!item.backup_strategy || item.backup_strategy === 'none') ? 'selected' : ''}>无需操作</option>
            <option value="copy_dir" ${item.backup_strategy === 'copy_dir' ? 'selected' : ''}>保留/压缩目录</option>
            <option value="copy_config" ${item.backup_strategy === 'copy_config' ? 'selected' : ''}>导出/备份配置</option>
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
            ${item.download_url ? `<a href="${escapeHtml(item.download_url)}" target="_blank" rel="noopener noreferrer" style="color:var(--accent);text-decoration:none;display:inline-flex;align-items:center;" title="打开外链">${ICONS.externalLink}</a>` : ''}
          </div>
        </td>
        <td>
          <input type="text" class="cell-input" data-field="config_notes" data-id="${item.id}" value="${escapeHtml(item.config_notes || '')}" placeholder="配置路径、备忘或注意事项...">
        </td>
      </tr>
    `;
  }

  tableBody.innerHTML = html || `<tr><td colspan="10" style="text-align: center; padding: 40px; color: var(--ink-3);">没有匹配的软件项</td></tr>`;
  updateBatchBar();
  if (activeItem) updateDrawerNavigation();
}

function updateBatchBar() {
  if (selectedIds.size > 0) {
    batchBar.classList.add('show');
    batchInfo.innerText = `已选 ${selectedIds.size} 项`;
    const items = softwareList.filter(s => selectedIds.has(s.id));
    const allHaveConfig = items.length > 0 && items.every(s => s.has_config);
    if (batchConfigLabel) batchConfigLabel.innerText = allHaveConfig ? '设为无配置' : '设为有配置';
    if (btnBatchConfig) btnBatchConfig.title = allHaveConfig ? '快捷键: 7 (批量标记为无配置)' : '快捷键: 7 (批量标记为有配置)';
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

        // 数字键 1~5 快速切换当前卡片意愿
        const drawerIntentMap = { '1': 'must', '2': 'should', '3': 'on_demand', '4': 'drop', '5': 'unreviewed' };
        if (drawerIntentMap[e.key]) {
          e.preventDefault();
          const targetIntent = drawerIntentMap[e.key];
          document.getElementById('drawerIntent').value = targetIntent;
          document.querySelectorAll('#drawerIntentSegmented .intent-seg-btn').forEach(btn => {
            btn.classList.toggle('active', btn.dataset.intent === targetIntent);
          });
          markDrawerSaving();
          return;
        }

        // 6 键快速切换准备就绪状态
        if (e.key === '6') {
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
      batchUpdate({ restore_intent: 'should' });
      showToast(`已标记选中的 ${selectedIds.size} 项为 建议恢复`, 'info');
    } else if (key === '3' || code === 'Digit3' || code === 'Numpad3') {
      e.preventDefault();
      batchUpdate({ restore_intent: 'on_demand' });
      showToast(`已标记选中的 ${selectedIds.size} 项为 用到再装`, 'info');
    } else if (key === '4' || code === 'Digit4' || code === 'Numpad4') {
      e.preventDefault();
      batchUpdate({ restore_intent: 'drop' });
      showToast(`已标记选中的 ${selectedIds.size} 项为 淘汰弃用`, 'warning');
    } else if (key === '5' || code === 'Digit5' || code === 'Numpad5') {
      e.preventDefault();
      batchResetDefault();
    } else if (key === '6' || code === 'Digit6' || code === 'Numpad6') {
      e.preventDefault();
      batchUpdate({ prep_status: 'ready' });
      showToast(`已标记选中的 ${selectedIds.size} 项为 已就绪`, 'success');
    } else if (key === '7' || code === 'Digit7' || code === 'Numpad7') {
      e.preventDefault();
      batchConfigToggle();
    } else if (key === '8' || code === 'Digit8' || code === 'Numpad8') {
      e.preventDefault();
      handleBatchLLM();
    } else if (key === '9' || code === 'Digit9' || code === 'Numpad9') {
      e.preventDefault();
      batchMerge();
    } else if (key === 'Escape') {
      e.preventDefault();
      selectedIds.clear();
      renderTable();
      showToast('已取消选择');
    } else if (key === 'Delete' || key === 'Backspace' || code === 'Delete') {
      e.preventDefault();
      const now = Date.now();
      if (now - lastDeleteTime < 2000) {
        lastDeleteTime = 0;
        executeBatchDelete();
      } else {
        lastDeleteTime = now;
        showToast('2秒内再次按 Delete 确认删除', 'warning', 2000);
      }
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
  const titleEl = document.getElementById('drawerTitle');
  if (titleEl) titleEl.innerText = activeItem.name;

  try {
    await fetch('/api/software/update', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: activeItem.id, updates })
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
function openDrawer(id) {
  flushDrawerSave();
  activeItem = softwareList.find(s => s.id === id);
  if (!activeItem) return;

  const idEl = document.getElementById('drawerId');
  if (idEl) idEl.textContent = activeItem.id || 'SW-ITEM';

  const titleEl = document.getElementById('drawerTitle');
  if (titleEl) titleEl.innerText = activeItem.name;

  document.getElementById('drawerName').value = activeItem.name || '';
  document.getElementById('drawerVersion').value = activeItem.version || '';
  document.getElementById('drawerCategory').value = activeItem.category || '开发工具';
  document.getElementById('drawerType').value = activeItem.type || 'desktop';
  renderDrawerPrepToggle(activeItem.prep_status || 'todo');
  renderDrawerHasConfigToggle(activeItem.has_config);
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
        ${ICONS.device} ${escapeHtml(getMachineDisplayName(m.machine_id))} <span class="machine-raw-tag">(${m.machine_id}) · ${m.form}</span>
      </div>
      <div>路径: <code>${m.install_location || m.path || '未记录路径'}</code></div>
      ${m.version ? `<div>版本: <code>${m.version}</code></div>` : ''}
      ${m.publisher ? `<div>发布者: ${m.publisher}</div>` : ''}
    </div>
  `).join('') || '<div style="color: var(--ink-3)">暂无关联机器信息</div>';

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
  softwareList = softwareList.filter(s => s.id !== id);
  await fetch('/api/software/delete', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids: [id] })
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
      await updateItemField(id, { restore_intent: val });
      target.className = `badge-select intent-${val}`;
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

  // 折叠展开机器证据
  const evidenceHeader = document.getElementById('evidenceHeader');
  if (evidenceHeader) {
    evidenceHeader.addEventListener('click', () => {
      const list = document.getElementById('drawerMachinesList');
      const icon = document.getElementById('evidenceToggleIcon');
      if (list) {
        const isHidden = list.style.display === 'none';
        list.style.display = isHidden ? 'flex' : 'none';
        if (icon) icon.innerText = isHidden ? '▼' : '▶';
      }
    });
  }

  // 抽屉官网链接快速打开
  const btnOpenUrl = document.getElementById('btnOpenUrl');
  if (btnOpenUrl) {
    btnOpenUrl.addEventListener('click', () => {
      const url = document.getElementById('drawerUrl').value.trim();
      if (url) window.open(url.startsWith('http') ? url : `https://${url}`, '_blank', 'noopener,noreferrer');
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
  btnDeleteCurrent.addEventListener('click', deleteCurrentItem);
  const btnResetCurrent = document.getElementById('btnResetCurrent');
  if (btnResetCurrent) btnResetCurrent.addEventListener('click', resetCurrentItem);
  btnDrawerLLM.addEventListener('click', handleDrawerLLM);

  // 批量操作按钮
  btnBatchMust.addEventListener('click', () => {
    batchUpdate({ restore_intent: 'must' });
    showToast('已设为必须恢复', 'success');
  });
  btnBatchShould.addEventListener('click', () => {
    batchUpdate({ restore_intent: 'should' });
    showToast('已设为建议恢复', 'info');
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
  btnBatchConfig.addEventListener('click', batchConfigToggle);
  btnBatchDelete.addEventListener('click', () => {
    const now = Date.now();
    if (now - lastDeleteTime < 2000) {
      lastDeleteTime = 0;
      executeBatchDelete();
    } else {
      lastDeleteTime = now;
      showToast('2秒内再次点击删除确认', 'warning', 2000);
    }
  });
  btnBatchMerge.addEventListener('click', batchMerge);
  btnBatchLLM.addEventListener('click', handleBatchLLM);

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

  // 关于弹窗
  const btnAbout = document.getElementById('btnAbout');
  if (btnAbout) btnAbout.addEventListener('click', openAboutModal);

  bindPreset('presetLocal', () => {
    configLlmUrl.value = 'http://127.0.0.1:1234/v1/chat/completions';
    configLlmModel.value = 'qwen3.5-4b';
    configLlmKey.value = '';
    showToast('已填入本地 LM Studio 预设', 'info');
  });
  bindPreset('presetDeepseek', () => {
    configLlmUrl.value = 'https://api.deepseek.com';
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
      body: JSON.stringify({ id, updates })
    });
  } catch (e) {
    showToast('保存失败: ' + e.message, 'error');
  }
}

// 批量修改
async function batchUpdate(updates) {
  if (selectedIds.size === 0) return;
  const ids = Array.from(selectedIds);

  for (const s of softwareList) {
    if (selectedIds.has(s.id)) Object.assign(s, updates);
  }

  await fetch('/api/software/batch-update', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids, updates })
  });

  // 保留勾选，方便连续批量设置多项（按 Esc 可取消选择）
  await fetchStatus();
  renderTable();
  updateBatchBar();
}

// 批量切换有无配置：若选中项已全部有配置则清空，否则统一标记为有配置
function batchConfigToggle() {
  if (selectedIds.size === 0) return;
  const items = softwareList.filter(s => selectedIds.has(s.id));
  const allHaveConfig = items.length > 0 && items.every(s => s.has_config);
  const next = !allHaveConfig;
  batchUpdate({ has_config: next });
  showToast(`已将 ${items.length} 项标记为「${next ? '有配置' : '无配置'}」`, 'info');
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
  const ids = Array.from(selectedIds);
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
  const ids = Array.from(selectedIds);
  const targetId = ids[0];
  const mergeIds = ids.slice(1);
  const targetItem = softwareList.find(s => s.id === targetId);

  const res = await fetch('/api/software/merge', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ targetId, mergeIds })
  });
  const data = await res.json();
  if (data.success) {
    selectedIds.clear();
    await loadSoftware();
    await fetchStatus();
    showToast(`已将 ${mergeIds.length} 项合并至【${targetItem.name}】`, 'success');
  }
}

// LLM 批量智能推断
async function handleBatchLLM() {
  if (selectedIds.size === 0) {
    showToast('请先勾选需要 AI 预判的软件', 'warning');
    return;
  }
  const ids = Array.from(selectedIds);
  btnBatchLLM.disabled = true;
  const originalHtml = btnBatchLLM.innerHTML;
  btnBatchLLM.innerHTML = '<span class="spinner"></span> 思考中...';

  try {
    let successCount = 0;
    for (const id of ids) {
      const item = softwareList.find(s => s.id === id);
      if (!item) continue;
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
          if (sug.restore_intent) updates.restore_intent = sug.restore_intent;
          if (sug.backup_strategy) updates.backup_strategy = sug.backup_strategy;
          if (sug.download_url) updates.download_url = sug.download_url;
          if (sug.config_notes) updates.config_notes = sug.config_notes;

          Object.assign(item, updates);
          await updateItemField(id, updates);
          successCount++;
        }
      } catch (err) {
        console.error(`AI analyze failed for ${item.name}:`, err);
      }
    }

    selectedIds.clear();
    await fetchStatus();
    renderTable();
    showToast(`LLM 已成功预判并更新了 ${successCount} 款软件！`, 'success');
  } finally {
    btnBatchLLM.disabled = false;
    btnBatchLLM.innerHTML = originalHtml;
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
      }
      if (sug.backup_strategy) document.getElementById('drawerStrategy').value = sug.backup_strategy;
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

// 本机扫描
async function handleScanLocal() {
  btnScanLocal.disabled = true;
  scanIcon.innerHTML = '<span class="spinner"></span>';
  showToast('正在启动 Windows 采集脚本扫描本机...', 'info');

  try {
    const res = await fetch('/api/scan', { method: 'POST' });
    const data = await res.json();
    if (data.success) {
      await loadSoftware();
      await fetchStatus();
      showToast(`本机扫描完成！已归一化 ${data.ingestRes?.totalIngested || 0} 个软件条目`, 'success');
    } else {
      showToast('扫描执行失败: ' + data.error, 'error');
    }
  } catch (e) {
    showToast('扫描请求异常: ' + e.message, 'error');
  } finally {
    btnScanLocal.disabled = false;
    scanIcon.innerHTML = ICONS.refresh;
  }
}

function downloadMarkdownFile(content, filename) {
  const blob = new Blob([content], { type: 'text/markdown;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  showToast('已下载到 下载\\' + filename, 'success');
}

window.downloadExportFile = function(type) {
  if (!window.__lastExportData) return;
  if (type === 'checklist') {
    downloadMarkdownFile(window.__lastExportData.checklistContent || '', window.__lastExportData.checklistFilename || 'RECOVERY_CHECKLIST.md');
  } else if (type === 'awesome') {
    downloadMarkdownFile(window.__lastExportData.awesomeContent || '', window.__lastExportData.awesomeFilename || 'AWESOME_LIST.md');
  }
};

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

      // 自动触发浏览器直接下载
      if (data.checklistContent) {
        downloadMarkdownFile(data.checklistContent, data.checklistFilename || 'RECOVERY_CHECKLIST.md');
      }
      if (data.awesomeContent) {
        setTimeout(() => {
          downloadMarkdownFile(data.awesomeContent, data.awesomeFilename || 'AWESOME_LIST.md');
        }, 300);
      }

      const modalBody = document.getElementById('exportModalBody');
      modalBody.innerHTML = `
        <p style="margin-bottom: 12px; color: var(--ink-2);">
          两份清单文档已<strong>自动下载</strong>至浏览器下载目录：
        </p>
        <div style="display: flex; flex-direction: column; gap: 8px; margin-bottom: 14px;">
          <div style="background: var(--surface-2); padding: 10px 14px; border-radius: 6px; box-shadow: inset 0 0 0 1px var(--rule); display: flex; justify-content: space-between; align-items: center;">
            <div>
              <div style="font-weight: 600; color: var(--ink);">📋 重装恢复备忘清单</div>
              <div style="font-size: 11.5px; color: var(--ink-3); font-family: var(--mono);">${escapeHtml(data.checklistFilename || 'RECOVERY_CHECKLIST.md')}</div>
            </div>
            <button class="btn btn-secondary btn-sm" onclick="downloadExportFile('checklist')">
              <svg class="i sm" viewBox="0 0 24 24"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg> 重新下载
            </button>
          </div>
          <div style="background: var(--surface-2); padding: 10px 14px; border-radius: 6px; box-shadow: inset 0 0 0 1px var(--rule); display: flex; justify-content: space-between; align-items: center;">
            <div>
              <div style="font-weight: 600; color: var(--ink);">⭐ 个人精选资产库</div>
              <div style="font-size: 11.5px; color: var(--ink-3); font-family: var(--mono);">${escapeHtml(data.awesomeFilename || 'AWESOME_LIST.md')}</div>
            </div>
            <button class="btn btn-secondary btn-sm" onclick="downloadExportFile('awesome')">
              <svg class="i sm" viewBox="0 0 24 24"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg> 重新下载
            </button>
          </div>
        </div>
        <p style="font-size: 12px; color: var(--ink-3);">如果浏览器拦截了自动弹出下载，可点击上方按钮重新下载。</p>
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
async function openAboutModal() {
  const endpointEl = document.getElementById('aboutLlmEndpoint');
  if (endpointEl) endpointEl.innerText = '读取中...';
  document.getElementById('aboutModal').classList.add('show');
  if (endpointEl) {
    try {
      const res = await fetch('/api/config');
      const cfg = await res.json();
      const keyState = cfg.llm_api_key ? '已配置 Key' : '无 Key';
      endpointEl.innerText = `${cfg.llm_url || '未设置'}  ·  ${cfg.llm_model || '未设置'}  (${keyState})`;
    } catch (e) {
      endpointEl.innerText = '读取失败';
    }
  }
}

// 配置弹窗管理
async function openConfigModal() {
  try {
    const res = await fetch('/api/config');
    const cfg = await res.json();
    configLlmUrl.value = cfg.llm_url || 'http://127.0.0.1:1234/v1/chat/completions';
    configLlmModel.value = cfg.llm_model || 'qwen3.5-4b';
    configLlmKey.value = cfg.llm_api_key || '';
    const scanDirsEl = document.getElementById('configScanDirs');
    if (scanDirsEl) {
      scanDirsEl.value = (cfg.scan_directories || []).join('\n');
    }

    // 渲染机器别名列表
    const aliasesListEl = document.getElementById('configMachineAliasesList');
    if (aliasesListEl) {
      const aliases = cfg.machine_aliases || {};
      machineAliases = aliases;
      const allKnownMachines = Array.from(new Set([...machinesList, ...Object.keys(aliases)]));
      if (allKnownMachines.length === 0) {
        allKnownMachines.push('DESKTOP-HEGVCTR');
      }
      aliasesListEl.innerHTML = allKnownMachines.map(mid => `
        <div class="machine-alias-row">
          <span class="machine-alias-id">${ICONS.device} ${escapeHtml(mid)}</span>
          <input type="text" class="machine-alias-input" data-mid="${escapeHtml(mid)}" value="${escapeHtml(aliases[mid] || '')}" placeholder="设置友好别名 (如：主力台式机 / 便携本)">
        </div>
      `).join('');
    }

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
  const scan_directories = scanDirsEl ? scanDirsEl.value.split('\n').map(s => s.trim()).filter(Boolean) : [];
  
  const machine_aliases = {};
  document.querySelectorAll('#configMachineAliasesList .machine-alias-input').forEach(input => {
    const mid = input.dataset.mid;
    const val = input.value.trim();
    if (mid && val) {
      machine_aliases[mid] = val;
    }
  });

  if (!llm_url) {
    showToast('请输入有效的 LLM API URL', 'warning');
    return;
  }

  try {
    const res = await fetch('/api/config', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ llm_url, llm_model, llm_api_key, scan_directories, machine_aliases })
    });
    const data = await res.json();
    if (data.success) {
      machineAliases = machine_aliases;
      configModal.classList.remove('show');
      renderTable();
      renderMachineTabs(machinesList);
      showToast('设置与设备别名已保存生效！', 'success');
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
