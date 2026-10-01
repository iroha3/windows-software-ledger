// client/app.js

let softwareList = [];
let machinesList = [];
let availablePaths = [];
let selectedIds = new Set();
let activeMachine = 'all';
let activeItem = null;
let lastDeleteTime = 0; // 用于双击 Delete 防误触

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
const btnSaveDrawer = document.getElementById('btnSaveDrawer');
const btnDeleteCurrent = document.getElementById('btnDeleteCurrent');
const btnDrawerLLM = document.getElementById('btnDrawerLLM');

// 批量添加弹窗
const batchAddModal = document.getElementById('batchAddModal');
const batchAddInput = document.getElementById('batchAddInput');
const batchAddIntent = document.getElementById('batchAddIntent');
const btnConfirmBatchAdd = document.getElementById('btnConfirmBatchAdd');

// 配置弹窗
const configModal = document.getElementById('configModal');
const configLlmUrl = document.getElementById('configLlmUrl');
const configLlmModel = document.getElementById('configLlmModel');
const btnSaveConfig = document.getElementById('btnSaveConfig');

// Toast 非阻塞消息提示系统
function showToast(message, type = 'info', duration = 2500) {
  const toast = document.createElement('div');
  toast.className = `toast toast-${type}`;
  let icon = 'ℹ️';
  if (type === 'success') icon = '✅';
  if (type === 'warning') icon = '⚠️';
  if (type === 'error') icon = '❌';

  toast.innerHTML = `<span>${icon}</span><span>${escapeHtml(message)}</span>`;
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
  await loadSoftware();
  await fetchStatus();
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
    themeIcon.innerText = '☀️';
    themeText.innerText = '日间';
  } else {
    document.documentElement.removeAttribute('data-theme');
    themeIcon.innerText = '🌙';
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
    availablePaths = data.availablePaths || [];
    renderStats(data.stats);
    renderMachineTabs(data.machines);
    renderPathFilter(availablePaths);
  } catch (e) {
    console.error('Failed to fetch status:', e);
  }
}

function renderPathFilter(paths) {
  const currentVal = pathFilter.value;
  let html = `<option value="all">📂 全部安装路径</option>`;
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
    html += `<button class="tab-btn ${activeMachine === m ? 'active' : ''}" data-machine="${m}">${m} (${count})</button>`;
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
      return `<span class="machine-badge" title="${m.install_location || m.path || ''}">${m.machine_id}</span>`;
    }).join('');

    html += `
      <tr class="${isSelected ? 'selected' : ''} ${isNewClass}" data-id="${item.id}">
        <td>
          <input type="checkbox" class="row-checkbox" data-id="${item.id}" ${isSelected ? 'checked' : ''}>
        </td>
        <td style="text-align: center;">
          <span class="awesome-star ${item.is_awesome ? 'starred' : ''}" data-action="toggle-awesome" data-id="${item.id}">
            ${item.is_awesome ? '★' : '☆'}
          </span>
        </td>
        <td>
          <div class="software-name-cell">
            <span class="software-title" data-action="open-drawer" data-id="${item.id}">${escapeHtml(item.name)}</span>
            <div class="software-tags">
              <span class="tag-cat">${item.category || '未分类'}</span>
              <span class="tag-form">${item.type || 'desktop'}</span>
              ${item.is_new ? '<span style="background:#dbeafe;color:#1e40af;padding:1px 5px;border-radius:4px;font-size:10px;">新添加</span>' : ''}
            </div>
          </div>
        </td>
        <td>
          <input type="text" class="cell-input" data-field="version" data-id="${item.id}" value="${escapeHtml(item.version || '')}" placeholder="—" style="font-family: monospace;">
        </td>
        <td>${machineBadges}</td>
        <td>
          <select class="badge-select ${intentClass}" data-field="restore_intent" data-id="${item.id}">
            <option value="must" ${item.restore_intent === 'must' ? 'selected' : ''}>🔴 必须恢复</option>
            <option value="should" ${item.restore_intent === 'should' ? 'selected' : ''}>🟡 建议恢复</option>
            <option value="on_demand" ${item.restore_intent === 'on_demand' ? 'selected' : ''}>🔵 用到再装</option>
            <option value="drop" ${item.restore_intent === 'drop' ? 'selected' : ''}>⚫ 淘汰弃用</option>
            <option value="unreviewed" ${(!item.restore_intent || item.restore_intent === 'unreviewed') ? 'selected' : ''}>⚪ 待确认</option>
          </select>
        </td>
        <td>
          <select class="strategy-select" data-field="backup_strategy" data-id="${item.id}">
            <option value="none" ${(!item.backup_strategy || item.backup_strategy === 'none') ? 'selected' : ''}>➖ 无需操作</option>
            <option value="copy_dir" ${item.backup_strategy === 'copy_dir' ? 'selected' : ''}>📦 保留/压缩目录</option>
            <option value="copy_config" ${item.backup_strategy === 'copy_config' ? 'selected' : ''}>⚙️ 导出/备份配置</option>
            <option value="redownload" ${item.backup_strategy === 'redownload' ? 'selected' : ''}>🌐 重新下载</option>
            <option value="sync_account" ${item.backup_strategy === 'sync_account' ? 'selected' : ''}>☁️ 账号同步</option>
          </select>
        </td>
        <td style="text-align: center;">
          <span class="prep-badge prep-${prepStatus}" data-action="toggle-prep" data-id="${item.id}">
            ${prepStatus === 'ready' ? '✅ 已就绪' : '⏳ 待办'}
          </span>
        </td>
        <td>
          <div style="display: flex; align-items: center; gap: 4px;">
            <input type="text" class="cell-input" data-field="download_url" data-id="${item.id}" value="${escapeHtml(item.download_url || '')}" placeholder="官网或下载网址...">
            ${item.download_url ? `<a href="${escapeHtml(item.download_url)}" target="_blank" style="text-decoration:none;" title="打开链接">🔗</a>` : ''}
          </div>
        </td>
        <td>
          <input type="text" class="cell-input" data-field="config_notes" data-id="${item.id}" value="${escapeHtml(item.config_notes || '')}" placeholder="配置路径、备忘或注意事项...">
        </td>
      </tr>
    `;
  }

  tableBody.innerHTML = html || `<tr><td colspan="10" style="text-align: center; padding: 40px; color: var(--text-dim);">没有匹配的软件项</td></tr>`;
  updateBatchBar();
}

function updateBatchBar() {
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
    // 只有文本类的 input 会阻断快捷键，复选框与单选框不阻断！
    return ['text', 'search', 'url', 'email', 'password', 'number'].includes(type);
  }
  return false;
}

// 绑定全局快捷键 (支持主键盘数字与小键盘数字 1~8、Esc、双击 Delete)
function bindKeyboardShortcuts() {
  window.addEventListener('keydown', e => {
    // 如果正在输入文字，或侧栏抽屉打开，或弹窗打开，则不触发快捷键
    if (isTypingInField()) return;
    if (sideDrawer && sideDrawer.classList.contains('show')) return;
    if (document.querySelector('.modal-overlay.show')) return;

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
      handleBatchLLM();
    } else if (key === '8' || code === 'Digit8' || code === 'Numpad8') {
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
        showToast('⚠️ 2秒内再次按 Delete 确认删除', 'warning', 2000);
      }
    }
  });
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
        star.innerText = item.is_awesome ? '★' : '☆';
        await updateItemField(id, { is_awesome: item.is_awesome });
        await fetchStatus();
        showToast(item.is_awesome ? '已加入 Awesome 精选库 ★' : '已取消精选标记');
      }
      return;
    }

    const prepBtn = e.target.closest('[data-action="toggle-prep"]');
    if (prepBtn) {
      const id = prepBtn.dataset.id;
      const item = softwareList.find(s => s.id === id);
      if (item) {
        const nextStatus = item.prep_status === 'ready' ? 'todo' : 'ready';
        item.prep_status = nextStatus;
        prepBtn.className = `prep-badge prep-${nextStatus}`;
        prepBtn.innerHTML = nextStatus === 'ready' ? '✅ 已就绪' : '⏳ 待办';
        await updateItemField(id, { prep_status: nextStatus });
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
  });

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
  btnBatchDelete.addEventListener('click', () => {
    const now = Date.now();
    if (now - lastDeleteTime < 2000) {
      lastDeleteTime = 0;
      executeBatchDelete();
    } else {
      lastDeleteTime = now;
      showToast('⚠️ 2秒内再次点击删除确认', 'warning', 2000);
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

  // 配置按钮
  btnConfig.addEventListener('click', openConfigModal);
  btnSaveConfig.addEventListener('click', saveConfigModal);

  // 侧边抽屉
  btnCloseDrawer.addEventListener('click', closeDrawer);
  drawerOverlay.addEventListener('click', closeDrawer);
  btnSaveDrawer.addEventListener('click', saveDrawer);
  btnDeleteCurrent.addEventListener('click', deleteCurrentItem);
  btnDrawerLLM.addEventListener('click', handleDrawerLLM);
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

  selectedIds.clear();
  await fetchStatus();
  renderTable();
}

// 恢复默认 / 重置
function batchResetDefault() {
  if (selectedIds.size === 0) return;
  batchUpdate({
    restore_intent: 'unreviewed',
    backup_strategy: 'none',
    prep_status: 'todo'
  });
  showToast(`已重置 ${selectedIds.size} 项为待定状态`, 'info');
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

// 本地 LLM 批量辅助推断
async function handleBatchLLM() {
  if (selectedIds.size === 0) return;
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
          if (sug.download_url && !item.download_url) updates.download_url = sug.download_url;
          if (sug.config_notes && !item.config_notes) updates.config_notes = sug.config_notes;

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
    showToast(`本地 LLM 已为 ${successCount} 款软件补全了建议！`, 'success');
  } finally {
    btnBatchLLM.disabled = false;
    btnBatchLLM.innerHTML = originalHtml;
  }
}

// 抽屉内部单条 AI 推断
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
      if (sug.restore_intent) document.getElementById('drawerIntent').value = sug.restore_intent;
      if (sug.backup_strategy) document.getElementById('drawerStrategy').value = sug.backup_strategy;
      if (sug.download_url) document.getElementById('drawerUrl').value = sug.download_url;
      if (sug.config_notes) document.getElementById('drawerNotes').value = sug.config_notes;
      showToast('已自动预填推荐字段，核对无误后请点击保存', 'success');
    } else {
      showToast('LLM 未返回有效结果: ' + (data.error || ''), 'error');
    }
  } catch (err) {
    showToast('请求失败，请检查 LLM 服务是否就绪: ' + err.message, 'error');
  } finally {
    btnDrawerLLM.disabled = false;
    btnDrawerLLM.innerText = '🤖 AI 预填';
  }
}

// 扫描本机
async function handleScanLocal() {
  btnScanLocal.disabled = true;
  scanIcon.innerHTML = '<span class="spinner"></span>';
  showToast('开始扫描本机软件与环境...', 'info', 3000);
  try {
    const res = await fetch('/api/scan', { method: 'POST' });
    const data = await res.json();
    if (data.success) {
      showToast('本机扫描已完成并自动归并入库！', 'success');
      await loadSoftware();
      await fetchStatus();
    } else {
      showToast('扫描出错: ' + (data.error || '未知错误'), 'error');
    }
  } catch (e) {
    showToast('请求失败: ' + e.message, 'error');
  } finally {
    btnScanLocal.disabled = false;
    scanIcon.innerText = '🔄';
  }
}

// 导出 Markdown
async function handleExport() {
  btnExport.disabled = true;
  try {
    const res = await fetch('/api/export', { method: 'POST' });
    const data = await res.json();
    if (data.success) {
      const modal = document.getElementById('exportModal');
      const body = document.getElementById('exportModalBody');
      body.innerHTML = `
        <p style="margin-bottom: 12px;">已成功生成两份高价值资产文档：</p>
        <div style="background: var(--bg-hover); padding: 12px; border-radius: 6px; font-family: monospace; font-size: 13px; margin-bottom: 12px; border: 1px solid var(--border-color);">
          <div>✅ <b>恢复清单</b>: ${data.checklistPath}</div>
          <div>✅ <b>精选清单</b>: ${data.awesomePath}</div>
        </div>
        <p><b>数据概览</b>：软件总数 <b>${data.stats.total}</b>，必须恢复 <b>${data.stats.must}</b>，建议恢复 <b>${data.stats.should}</b>，已就绪 <b>${data.stats.ready || 0}</b>，待打包备份资产 <b>${data.stats.backupTasks}</b>，精选工具 <b>${data.stats.awesome}</b> 款。</p>
      `;
      modal.classList.add('show');
    }
  } catch (e) {
    showToast('导出失败: ' + e.message, 'error');
  } finally {
    btnExport.disabled = false;
  }
}

// 批量添加软件
async function handleConfirmBatchAdd() {
  const text = batchAddInput.value.trim();
  if (!text) {
    showToast('请输入至少一个软件名称', 'warning');
    return;
  }

  const rawNames = text.split(/[,，\n\r;；]+/).map(n => n.trim()).filter(Boolean);
  if (rawNames.length === 0) return;

  const defaultIntent = batchAddIntent.value;
  btnConfirmBatchAdd.disabled = true;

  try {
    const res = await fetch('/api/software/batch-add', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ names: rawNames, restore_intent: defaultIntent })
    });
    const data = await res.json();
    if (data.success) {
      batchAddModal.classList.remove('show');
      for (const it of (data.items || []).reverse()) {
        softwareList.unshift(it);
      }
      
      searchInput.value = '';
      activeMachine = 'all';
      intentFilter.value = 'all';
      strategyFilter.value = 'all';
      categoryFilter.value = 'all';
      pathFilter.value = 'all';
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

// 配置弹窗管理
async function openConfigModal() {
  try {
    const res = await fetch('/api/config');
    const cfg = await res.json();
    configLlmUrl.value = cfg.llm_url || 'http://127.0.0.1:1234/v1/chat/completions';
    configLlmModel.value = cfg.llm_model || 'qwen3.5-4b';
    configModal.classList.add('show');
  } catch (e) {
    showToast('读取配置失败', 'error');
  }
}

async function saveConfigModal() {
  const llm_url = configLlmUrl.value.trim();
  const llm_model = configLlmModel.value.trim();
  if (!llm_url) {
    showToast('请输入有效的 LLM API URL', 'warning');
    return;
  }

  try {
    const res = await fetch('/api/config', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ llm_url, llm_model })
    });
    const data = await res.json();
    if (data.success) {
      configModal.classList.remove('show');
      showToast('LLM 端点配置已保存！', 'success');
    }
  } catch (e) {
    showToast('保存失败: ' + e.message, 'error');
  }
}

// 侧边抽屉管理 (加宽并一屏全览)
function openDrawer(id) {
  activeItem = softwareList.find(s => s.id === id);
  if (!activeItem) return;

  document.getElementById('drawerTitle').innerText = activeItem.name;
  document.getElementById('drawerName').value = activeItem.name;
  document.getElementById('drawerVersion').value = activeItem.version || '';
  document.getElementById('drawerCategory').value = activeItem.category || '开发工具';
  document.getElementById('drawerType').value = activeItem.type || 'desktop';
  document.getElementById('drawerPrepStatus').value = activeItem.prep_status || 'todo';
  document.getElementById('drawerUrl').value = activeItem.download_url || '';
  document.getElementById('drawerIntent').value = activeItem.restore_intent || 'unreviewed';
  document.getElementById('drawerStrategy').value = activeItem.backup_strategy || 'none';
  document.getElementById('drawerNotes').value = activeItem.config_notes || '';
  document.getElementById('drawerAwesome').checked = !!activeItem.is_awesome;
  document.getElementById('drawerAwesomeRole').value = activeItem.awesome_role || '';

  const listEl = document.getElementById('drawerMachinesList');
  listEl.innerHTML = activeItem.machines.map(m => `
    <div class="machine-item-card">
      <div style="font-weight: 600; color: var(--accent-blue);">🖥️ ${m.machine_id} (${m.form})</div>
      <div>路径: <code>${m.install_location || m.path || '未记录路径'}</code></div>
      ${m.version ? `<div>版本: <code>${m.version}</code></div>` : ''}
      ${m.publisher ? `<div>发布者: ${m.publisher}</div>` : ''}
    </div>
  `).join('') || '<div style="color: var(--text-dim)">暂无关联机器信息</div>';

  sideDrawer.classList.add('show');
  drawerOverlay.classList.add('show');
}

function closeDrawer() {
  sideDrawer.classList.remove('show');
  drawerOverlay.classList.remove('show');
  activeItem = null;
}

async function saveDrawer() {
  if (!activeItem) return;
  const updates = {
    name: document.getElementById('drawerName').value.trim(),
    version: document.getElementById('drawerVersion').value.trim(),
    category: document.getElementById('drawerCategory').value,
    type: document.getElementById('drawerType').value,
    prep_status: document.getElementById('drawerPrepStatus').value,
    download_url: document.getElementById('drawerUrl').value.trim(),
    restore_intent: document.getElementById('drawerIntent').value,
    backup_strategy: document.getElementById('drawerStrategy').value,
    config_notes: document.getElementById('drawerNotes').value.trim(),
    is_awesome: document.getElementById('drawerAwesome').checked,
    awesome_role: document.getElementById('drawerAwesomeRole').value.trim()
  };

  Object.assign(activeItem, updates);
  await updateItemField(activeItem.id, updates);
  closeDrawer();
  renderTable();
  await fetchStatus();
  showToast(`已保存【${activeItem.name}】的修改`, 'success');
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

function escapeHtml(str) {
  if (!str) return '';
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

window.addEventListener('DOMContentLoaded', init);
