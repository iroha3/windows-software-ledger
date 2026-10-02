// client/card_review.js
// 独立卡片速审工作台：高密度专注决策流、连续切卡、实时无感自动保存

let softwareList = [];
let currentFiltered = [];
let currentIndex = 0;
let activeItem = null;
let machineAliases = {};
let autoSaveTimer = null;
let pendingSave = false;
let lastDeleteTime = 0; // Backspace / 删除按钮的二次确认窗口

function getMachineDisplayName(id) {
  if (!id) return '未知设备';
  return machineAliases[id] || id;
}

// 绿色/便携软件：处置方式走确定性规则，不让 LLM 猜测。
function isPortableItem(item) {
  if (!item) return false;
  if (item.type === 'portable') return true;
  return (item.machines || []).some(m => m.form === 'portable');
}

// 统一 SVG 图标库
const ICONS = {
  sun: `<svg class="i sm" viewBox="0 0 24 24"><circle cx="12" cy="12" r="5"></circle><line x1="12" y1="1" x2="12" y2="3"></line><line x1="12" y1="21" x2="12" y2="23"></line><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line><line x1="1" y1="12" x2="3" y2="12"></line><line x1="21" y1="12" x2="23" y2="12"></line><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line></svg>`,
  moon: `<svg class="i sm" viewBox="0 0 24 24"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path></svg>`,
  check: `<svg class="i sm" viewBox="0 0 24 24"><polyline points="20 6 9 17 4 12"></polyline></svg>`,
  device: `<svg class="i sm" viewBox="0 0 24 24"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>`,
  sparkles: `<svg class="i sm" viewBox="0 0 24 24"><path d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83"></path></svg>`,
  info: `<svg class="i sm" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></line></svg>`,
  alert: `<svg class="i sm" viewBox="0 0 24 24"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>`
};

// DOM 元素
const toastContainer = document.getElementById('toastContainer');
const btnThemeToggle = document.getElementById('btnThemeToggle');
const themeIcon = document.getElementById('themeIcon');
const themeText = document.getElementById('themeText');
const btnExport = document.getElementById('btnExport');

const reviewSearchInput = document.getElementById('reviewSearchInput');
const reviewIntentFilter = document.getElementById('reviewIntentFilter');
const reviewCategoryFilter = document.getElementById('reviewCategoryFilter');
const reviewListItems = document.getElementById('reviewListItems');
const reviewProgressBar = document.getElementById('reviewProgressBar');
const reviewProgressText = document.getElementById('reviewProgressText');

const cardSoftwareId = document.getElementById('cardSoftwareId');
const cardSaveStatus = document.getElementById('cardSaveStatus');
const btnCardPrev = document.getElementById('btnCardPrev');
const btnCardNext = document.getElementById('btnCardNext');
const cardIndexBadge = document.getElementById('cardIndexBadge');
const btnCardLLM = document.getElementById('btnCardLLM');

const cardName = document.getElementById('cardName');
const cardCategory = document.getElementById('cardCategory');
const cardType = document.getElementById('cardType');
const cardVersion = document.getElementById('cardVersion');
const cardIntentSegmented = document.getElementById('cardIntentSegmented');
const cardIntent = document.getElementById('cardIntent');
const cardStrategy = document.getElementById('cardStrategy');
const cardPrepStatus = document.getElementById('cardPrepStatus');
const cardPrepToggle = document.getElementById('cardPrepToggle');
const cardPrepToggleDot = document.getElementById('cardPrepToggleDot');
const cardPrepToggleText = document.getElementById('cardPrepToggleText');
const cardHasConfigToggle = document.getElementById('cardHasConfigToggle');
const cardHasConfigDot = document.getElementById('cardHasConfigDot');
const cardHasConfigText = document.getElementById('cardHasConfigText');
const cardHasConfig = document.getElementById('cardHasConfig');
const cardUrl = document.getElementById('cardUrl');
const btnCardOpenUrl = document.getElementById('btnCardOpenUrl');
const cardNotes = document.getElementById('cardNotes');

const cardAwesomeBlock = document.getElementById('cardAwesomeBlock');
const cardAwesome = document.getElementById('cardAwesome');
const cardAwesomeExpand = document.getElementById('cardAwesomeExpand');
const cardAwesomeRole = document.getElementById('cardAwesomeRole');

const cardMachinesList = document.getElementById('cardMachinesList');

const btnCardDelete = document.getElementById('btnCardDelete');
const btnCardReset = document.getElementById('btnCardReset');
const btnCardNextBottom = document.getElementById('btnCardNextBottom');

// Toast 提示
function showToast(message, type = 'info', duration = 2200) {
  const toast = document.createElement('div');
  toast.className = `toast toast-${type}`;
  let iconSvg = ICONS.info;
  if (type === 'success') iconSvg = ICONS.check;
  if (type === 'warning' || type === 'error') iconSvg = ICONS.alert;

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
  await loadSoftware();
  bindEvents();
  bindShortcuts();
  scanLocalIfEmpty();
}

// 首次进入 / 清单为空时自动扫描本机（直接导入全部新增，无需弹窗）
async function scanLocalIfEmpty() {
  if (softwareList.length > 0) return;
  showToast('清单为空，正在自动扫描本机...', 'info');
  try {
    const res = await fetch('/api/scan/preview', { method: 'POST' });
    const data = await res.json();
    if (!data.success) {
      showToast('扫描执行失败: ' + data.error, 'error');
      return;
    }
    const keys = (data.candidates || []).filter(c => c.kind === 'new').map(c => c.key);
    if (keys.length > 0) {
      await fetch('/api/scan/commit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ selectedKeys: keys })
      });
    }
    await loadSoftware();
    showToast(`本机扫描完成，已导入 ${keys.length} 个软件条目`, 'success');
  } catch (e) {
    showToast('扫描请求异常: ' + e.message, 'error');
  }
}

function initTheme() {
  const saved = localStorage.getItem('app-theme') || 'light';
  applyTheme(saved);
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

async function loadSoftware() {
  try {
    const [swRes, cfgRes] = await Promise.all([
      fetch('/api/software'),
      fetch('/api/config')
    ]);
    softwareList = await swRes.json();
    const cfg = await cfgRes.json();
    machineAliases = cfg.machine_aliases || {};
    applyFilters();
    if (currentFiltered.length > 0) {
      loadCard(0);
    }
  } catch (e) {
    showToast('加载软件清单失败: ' + e.message, 'error');
  }
}

// 过滤与侧边栏渲染
function applyFilters() {
  const q = reviewSearchInput.value.toLowerCase().trim();
  const intent = reviewIntentFilter.value;
  const cat = reviewCategoryFilter.value;

  currentFiltered = softwareList.filter(item => {
    if (intent !== 'all') {
      const it = item.restore_intent || 'unreviewed';
      if (it !== intent) return false;
    }
    if (cat !== 'all' && item.category !== cat) return false;
    if (q) {
      const nameMatch = item.name.toLowerCase().includes(q);
      const noteMatch = (item.config_notes || '').toLowerCase().includes(q);
      const urlMatch = (item.download_url || '').toLowerCase().includes(q);
      if (!nameMatch && !noteMatch && !urlMatch) return false;
    }
    return true;
  });

  renderSidebarList();
  updateProgress();
}

function renderSidebarList() {
  let html = '';
  currentFiltered.forEach((item, idx) => {
    const intent = item.restore_intent || 'unreviewed';
    const isActive = activeItem && activeItem.id === item.id;
    html += `
      <li class="review-list-item ${isActive ? 'active' : ''}" data-index="${idx}">
        <div style="display: flex; align-items: center; gap: 6px; min-width: 0;">
          <span class="status-dot dot-${intent}"></span>
          <span class="review-item-name" title="${escapeHtml(item.name)}">${escapeHtml(item.name)}</span>
        </div>
        <span style="font-size: 11px; color: var(--ink-3); font-family: var(--mono);">${item.category || ''}</span>
      </li>
    `;
  });
  reviewListItems.innerHTML = html || `<li style="padding: 24px; text-align: center; color: var(--ink-3); font-size: 12px;">无匹配软件</li>`;
}

function updateProgress() {
  const total = softwareList.length;
  const reviewed = softwareList.filter(s => s.restore_intent && s.restore_intent !== 'unreviewed').length;
  const percent = total > 0 ? Math.round((reviewed / total) * 100) : 0;
  reviewProgressText.innerText = `${reviewed} / ${total} (${percent}%)`;
  reviewProgressBar.style.width = `${percent}%`;
}

// 加载单张卡片
function loadCard(index) {
  if (index < 0 || index >= currentFiltered.length) return;
  flushSave();
  currentIndex = index;
  activeItem = currentFiltered[index];

  cardSoftwareId.innerText = activeItem.id;
  cardIndexBadge.innerText = `${currentIndex + 1} / ${currentFiltered.length}`;
  btnCardPrev.disabled = currentIndex <= 0;
  btnCardNext.disabled = currentIndex >= currentFiltered.length - 1;

  cardName.value = activeItem.name;
  cardCategory.value = activeItem.category || '开发工具';
  cardType.value = activeItem.type || 'desktop';
  cardVersion.value = activeItem.version || '';

  // 意愿分段按钮激活态
  const currentIntent = activeItem.restore_intent || 'unreviewed';
  cardIntent.value = currentIntent;
  updateIntentButtons(currentIntent);

  cardStrategy.value = activeItem.backup_strategy || 'none';
  renderPrepToggle(activeItem.prep_status || 'todo');
  renderHasConfigToggle(activeItem.has_config);
  cardUrl.value = activeItem.download_url || '';
  cardNotes.value = activeItem.config_notes || '';

  // 紧凑精选开关
  cardAwesome.checked = !!activeItem.is_awesome;
  const pill = document.getElementById('cardAwesomeToggle');
  if (pill) pill.classList.toggle('active', !!activeItem.is_awesome);
  cardAwesomeExpand.style.display = activeItem.is_awesome ? 'flex' : 'none';
  cardAwesomeRole.value = activeItem.awesome_role || '';

  // 机器线索 (精美卡片式呈现，展示友好别名)
  cardMachinesList.innerHTML = (activeItem.machines || []).map(m => `
    <div class="machine-item-card">
      <div style="font-weight: 600; color: var(--accent); display: inline-flex; align-items: center; gap: 5px;">
        ${ICONS.device} ${escapeHtml(getMachineDisplayName(m.machine_id))} <span class="machine-raw-tag">(${m.machine_id}) · ${m.form}</span>
      </div>
      <div>路径: <code>${m.install_location || m.path || '未记录路径'}</code></div>
      ${m.version ? `<div>版本: <code>${m.version}</code></div>` : ''}
      ${m.publisher ? `<div>发布者: ${m.publisher}</div>` : ''}
    </div>
  `).join('') || '<div style="color:var(--ink-3);">暂无关联机器信息</div>';

  cardSaveStatus.className = 'save-status';
  cardSaveStatus.innerHTML = `${ICONS.check} 已同步`;

  // 高亮侧边栏选中项并自动滚入视口
  document.querySelectorAll('.review-list-item').forEach((el, i) => {
    el.classList.toggle('active', i === currentIndex);
    if (i === currentIndex) el.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  });
}

function updateIntentButtons(intent) {
  document.querySelectorAll('#cardIntentSegmented .intent-seg-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.intent === intent);
  });
}

// 状态切换按钮渲染 (配置状态 / 就绪进度)
function renderPrepToggle(status) {
  const isReady = status === 'ready';
  cardPrepStatus.value = isReady ? 'ready' : 'todo';
  cardPrepToggle.classList.toggle('prep-ready', isReady);
  cardPrepToggle.classList.toggle('prep-todo', !isReady);
  cardPrepToggleDot.className = `status-dot ${isReady ? 'dot-ready' : 'dot-unreviewed'}`;
  cardPrepToggleText.innerText = isReady ? '就绪' : '待办';
}

function renderHasConfigToggle(hasConfig) {
  const on = !!hasConfig;
  cardHasConfig.value = on ? 'true' : 'false';
  cardHasConfigToggle.classList.toggle('prep-hasconfig', on);
  cardHasConfigToggle.classList.toggle('prep-todo', !on);
  cardHasConfigDot.className = `status-dot ${on ? 'dot-ondemand' : 'dot-unreviewed'}`;
  cardHasConfigText.innerText = on ? '有配置' : '无配置';
}

// 实时无感自动保存
function markSaving() {
  pendingSave = true;
  cardSaveStatus.className = 'save-status saving';
  cardSaveStatus.innerHTML = `<span class="spinner"></span> 保存中...`;
  clearTimeout(autoSaveTimer);
  autoSaveTimer = setTimeout(performAutoSave, 350);
}

async function performAutoSave() {
  if (!activeItem) return;
  clearTimeout(autoSaveTimer);
  pendingSave = false;

  const updates = {
    name: cardName.value.trim() || activeItem.name,
    category: cardCategory.value,
    type: cardType.value,
    version: cardVersion.value.trim(),
    restore_intent: cardIntent.value,
    backup_strategy: cardStrategy.value,
    prep_status: cardPrepStatus.value,
    has_config: cardHasConfig.value === 'true',
    download_url: cardUrl.value.trim(),
    config_notes: cardNotes.value.trim(),
    is_awesome: cardAwesome.checked,
    awesome_role: cardAwesomeRole.value.trim()
  };

  Object.assign(activeItem, updates);

  try {
    await fetch('/api/software/update', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: activeItem.id, updates })
    });
    cardSaveStatus.className = 'save-status';
    cardSaveStatus.innerHTML = `${ICONS.check} 已同步`;
    updateProgress();
    // 更新侧边栏状态点
    const activeSidebarEl = document.querySelector(`.review-list-item[data-index="${currentIndex}"]`);
    if (activeSidebarEl) {
      const dot = activeSidebarEl.querySelector('.status-dot');
      if (dot) dot.className = `status-dot dot-${updates.restore_intent}`;
      const title = activeSidebarEl.querySelector('.review-item-name');
      if (title) title.innerText = updates.name;
    }
  } catch (e) {
    cardSaveStatus.className = 'save-status';
    cardSaveStatus.innerHTML = `⚠️ 保存失败`;
  }
}

function flushSave() {
  if (pendingSave) performAutoSave();
}

// 恢复默认：清空决策状态与官网/描述等内容字段
function resetCard() {
  if (!activeItem) return;
  cardIntent.value = 'unreviewed';
  updateIntentButtons('unreviewed');
  cardStrategy.value = 'none';
  cardUrl.value = '';
  cardNotes.value = '';
  renderPrepToggle('todo');
  renderHasConfigToggle(false);
  markSaving();
  showToast('已恢复默认（清空决策、官网与描述）', 'info');
}

// 删除当前卡片：2 秒内再按一次 Backspace / 再点一次删除确认（替换原生 confirm）
async function requestDeleteCurrent() {
  if (!activeItem) return;
  const now = Date.now();
  if (now - lastDeleteTime < 2000) {
    lastDeleteTime = 0;
    await performDeleteCurrent();
  } else {
    lastDeleteTime = now;
    showToast('2 秒内再按一次 Backspace 或再点一次删除以确认', 'warning', 2000);
  }
}

async function performDeleteCurrent() {
  if (!activeItem) return;
  const name = activeItem.name;
  const id = activeItem.id;
  softwareList = softwareList.filter(s => s.id !== id);
  await fetch('/api/software/delete', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids: [id] })
  });
  showToast(`已删除软件【${name}】`, 'info');
  applyFilters();
  if (currentFiltered.length > 0) {
    // 删除后停在原索引，即自动前进到下一张（配合 ] 浏览）
    loadCard(Math.min(currentIndex, currentFiltered.length - 1));
  } else {
    activeItem = null;
  }
}

// 切卡导航
function navigateCard(delta) {
  const target = currentIndex + delta;
  if (target >= 0 && target < currentFiltered.length) {
    loadCard(target);
  }
}

// 检查是否正在打字
function isTypingInField() {
  const el = document.activeElement;
  if (!el) return false;
  if (el.isContentEditable) return true;
  const tag = (el.tagName || '').toLowerCase();
  if (tag === 'textarea') return true;
  if (tag === 'input') {
    const type = (el.type || 'text').toLowerCase();
    return ['text', 'search', 'url', 'email', 'password', 'number'].includes(type);
  }
  return false;
}

// 快捷键绑定 (特别防御 Firefox Quick Find，并支持 PageUp / PageDown)
function bindShortcuts() {
  window.addEventListener('keydown', e => {
    // 1. PageUp / PageDown 与 Alt+Left / Alt+Right 始终切卡，即便在输入框中打字也无缝响应
    if (e.key === 'PageUp' || (e.altKey && e.key === 'ArrowLeft')) {
      e.preventDefault();
      navigateCard(-1);
      return;
    }
    if (e.key === 'PageDown' || (e.altKey && e.key === 'ArrowRight')) {
      e.preventDefault();
      navigateCard(1);
      return;
    }

    // 2. 按 '/' 键聚焦左侧搜索框 (必须 preventDefault 阻止 Firefox 的快速查找 Quick Find)
    if (!isTypingInField() && e.key === '/') {
      e.preventDefault();
      reviewSearchInput.focus();
      reviewSearchInput.select();
      return;
    }

    // 3. 非打字状态下的快捷操作
    if (!isTypingInField()) {
      if (e.key === '[' || e.key === 'BracketLeft') {
        e.preventDefault();
        navigateCard(-1);
        return;
      }
      if (e.key === ']' || e.key === 'BracketRight') {
        e.preventDefault();
        navigateCard(1);
        return;
      }

      // Backspace / Delete：删除当前卡片（二次确认后自动前进）
      if (e.key === 'Backspace' || e.key === 'Delete') {
        e.preventDefault();
        requestDeleteCurrent();
        return;
      }

      // 数字键 1~5 秒切意愿
      const intentMap = { '1': 'must', '2': 'should', '3': 'on_demand', '4': 'drop', '5': 'unreviewed' };
      if (intentMap[e.key]) {
        e.preventDefault();
        const nextIntent = intentMap[e.key];
        cardIntent.value = nextIntent;
        updateIntentButtons(nextIntent);
        markSaving();
        return;
      }

      // 6 键切换准备状态
      if (e.key === '6') {
        e.preventDefault();
        const nextPrep = cardPrepStatus.value === 'ready' ? 'todo' : 'ready';
        renderPrepToggle(nextPrep);
        markSaving();
        showToast(`已标记为: ${nextPrep === 'ready' ? '已就绪' : '待办'}`);
        return;
      }
    }
  });
}

// 事件监听绑定
function bindEvents() {
  btnThemeToggle.addEventListener('click', () => {
    const isDark = document.documentElement.getAttribute('data-theme') === 'dark';
    applyTheme(isDark ? 'light' : 'dark');
  });

  // 侧边栏搜索与过滤
  reviewSearchInput.addEventListener('input', applyFilters);
  reviewIntentFilter.addEventListener('change', applyFilters);
  reviewCategoryFilter.addEventListener('change', applyFilters);

  // 侧边栏条目点击
  reviewListItems.addEventListener('click', e => {
    const itemEl = e.target.closest('.review-list-item');
    if (!itemEl) return;
    const idx = parseInt(itemEl.dataset.index, 10);
    if (!isNaN(idx)) loadCard(idx);
  });

  // 切卡按钮
  btnCardPrev.addEventListener('click', () => navigateCard(-1));
  btnCardNext.addEventListener('click', () => navigateCard(1));
  btnCardNextBottom.addEventListener('click', () => navigateCard(1));

  // 配置状态 / 就绪进度切换按钮
  cardPrepToggle.addEventListener('click', () => {
    renderPrepToggle(cardPrepStatus.value === 'ready' ? 'todo' : 'ready');
    markSaving();
  });
  cardHasConfigToggle.addEventListener('click', () => {
    renderHasConfigToggle(cardHasConfig.value !== 'true');
    markSaving();
  });

  // 意愿大胶囊点击
  cardIntentSegmented.addEventListener('click', e => {
    const btn = e.target.closest('.intent-seg-btn');
    if (!btn) return;
    const chosen = btn.dataset.intent;
    cardIntent.value = chosen;
    updateIntentButtons(chosen);
    markSaving();
  });

  // 输入监听触发实时自动保存
  [cardName, cardVersion, cardUrl, cardNotes, cardAwesomeRole].forEach(el => {
    el.addEventListener('input', markSaving);
  });

  [cardCategory, cardType, cardStrategy, cardPrepStatus].forEach(el => {
    el.addEventListener('change', markSaving);
  });

  // 紧凑精选切换
  const cardAwesomeToggle = document.getElementById('cardAwesomeToggle');
  if (cardAwesomeToggle) {
    cardAwesomeToggle.addEventListener('click', (e) => {
      e.preventDefault();
      cardAwesome.checked = !cardAwesome.checked;
      const checked = cardAwesome.checked;
      cardAwesomeToggle.classList.toggle('active', checked);
      cardAwesomeExpand.style.display = checked ? 'flex' : 'none';
      if (checked) cardAwesomeRole.focus();
      markSaving();
    });
  }

  // 打开外部链接
  btnCardOpenUrl.addEventListener('click', () => {
    const url = cardUrl.value.trim();
    if (url) window.openExternal(url);
    else showToast('当前尚未录入官网或下载链接', 'warning');
  });

  // AI 预填单条
  btnCardLLM.addEventListener('click', async () => {
    if (!activeItem) return;
    btnCardLLM.disabled = true;
    btnCardLLM.innerHTML = '<span class="spinner"></span> 思考中...';

    const paths = (activeItem.machines || []).map(m => m.install_location || m.path).filter(Boolean).join('; ');
    try {
      const res = await fetch('/api/llm/analyze', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id: activeItem.id, name: activeItem.name, paths, category: activeItem.category })
      });
      const data = await res.json();
      if (data.success && data.suggestion) {
        const sug = data.suggestion;
        if (sug.category) cardCategory.value = sug.category;
        if (sug.type) cardType.value = sug.type;
        if (sug.restore_intent) {
          cardIntent.value = sug.restore_intent;
          updateIntentButtons(sug.restore_intent);
        }
        if (sug.backup_strategy && !isPortableItem(activeItem)) cardStrategy.value = sug.backup_strategy;
        if (sug.download_url && !cardUrl.value) cardUrl.value = sug.download_url;
        if (sug.config_notes && !cardNotes.value) cardNotes.value = sug.config_notes;

        markSaving();
        showToast('AI 预填完成，已自动保存！', 'success');
      } else {
        showToast('AI 建议生成异常: ' + (data.error || '未知错误'), 'warning');
      }
    } catch (e) {
      showToast('调用失败: ' + e.message, 'error');
    } finally {
      btnCardLLM.disabled = false;
      btnCardLLM.innerHTML = `${ICONS.sparkles} AI预填`;
    }
  });

  // 恢复默认
  btnCardReset.addEventListener('click', resetCard);

  // 删除软件
  btnCardDelete.addEventListener('click', requestDeleteCurrent);
  const btnCardDeleteTop = document.getElementById('btnCardDeleteTop');
  if (btnCardDeleteTop) btnCardDeleteTop.addEventListener('click', requestDeleteCurrent);

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

  // 导出 Markdown 清单
  btnExport.addEventListener('click', async () => {
    btnExport.disabled = true;
    try {
      const res = await fetch('/api/export', { method: 'POST' });
      const data = await res.json();
      if (data.success) {
        if (data.checklistContent) {
          downloadMarkdownFile(data.checklistContent, data.checklistFilename || 'RECOVERY_CHECKLIST.md');
        }
        if (data.awesomeContent) {
          setTimeout(() => {
            downloadMarkdownFile(data.awesomeContent, data.awesomeFilename || 'AWESOME_LIST.md');
          }, 300);
        }
      } else {
        showToast('导出失败: ' + (data.error || data.message), 'error');
      }
    } catch (e) {
      showToast('导出异常: ' + e.message, 'error');
    } finally {
      btnExport.disabled = false;
    }
  });

  // 中台设置弹窗
  const btnConfig = document.getElementById('btnConfig');
  const btnSaveConfig = document.getElementById('btnSaveConfig');
  if (btnConfig) btnConfig.addEventListener('click', openConfigModal);
  if (btnSaveConfig) btnSaveConfig.addEventListener('click', saveConfigModal);

  const btnAbout = document.getElementById('btnAbout');
  if (btnAbout) btnAbout.addEventListener('click', openAboutModal);

  bindPreset('presetLocal', () => {
    document.getElementById('configLlmUrl').value = 'http://127.0.0.1:1234/v1/chat/completions';
    document.getElementById('configLlmModel').value = 'qwen3.5-4b';
    document.getElementById('configLlmKey').value = '';
    showToast('已填入本地 LM Studio 预设', 'info');
  });
  bindPreset('presetDeepseek', () => {
    document.getElementById('configLlmUrl').value = 'https://api.deepseek.com';
    document.getElementById('configLlmModel').value = 'deepseek-flash';
    document.getElementById('configLlmKey').focus();
    showToast('已填入 DeepSeek 预设，请填入 API Key', 'info');
  });
  bindPreset('presetOpenai', () => {
    document.getElementById('configLlmUrl').value = 'https://api.openai.com/v1/chat/completions';
    document.getElementById('configLlmModel').value = 'gpt-4o-mini';
    document.getElementById('configLlmKey').focus();
    showToast('已填入 OpenAI 预设，请填入 API Key', 'info');
  });
  bindPreset('presetSilicon', () => {
    document.getElementById('configLlmUrl').value = 'https://api.siliconflow.cn/v1/chat/completions';
    document.getElementById('configLlmModel').value = 'Qwen/Qwen2.5-7B-Instruct';
    document.getElementById('configLlmKey').focus();
    showToast('已填入硅基流动预设，请填入 API Key', 'info');
  });
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

// 设置弹窗逻辑
async function openConfigModal() {
  try {
    const res = await fetch('/api/config');
    const cfg = await res.json();
    document.getElementById('configLlmUrl').value = cfg.llm_url || '';
    document.getElementById('configLlmModel').value = cfg.llm_model || '';
    document.getElementById('configLlmKey').value = cfg.llm_api_key || '';
    const scanDirsEl = document.getElementById('configScanDirs');
    if (scanDirsEl) {
      scanDirsEl.value = (cfg.scan_directories || []).join('\n');
    }

    const aliasesListEl = document.getElementById('configMachineAliasesList');
    if (aliasesListEl) {
      const aliases = cfg.machine_aliases || {};
      machineAliases = aliases;
      const allKnownMachines = Array.from(new Set([
        ...softwareList.flatMap(s => (s.machines || []).map(m => m.machine_id)),
        ...Object.keys(aliases)
      ])).filter(Boolean);

      aliasesListEl.innerHTML = allKnownMachines.map(mid => `
        <div class="machine-alias-row">
          <span class="machine-alias-id">${ICONS.device} ${escapeHtml(mid)}</span>
          <input type="text" class="machine-alias-input" data-mid="${escapeHtml(mid)}" value="${escapeHtml(aliases[mid] || '')}" placeholder="设置友好别名 (如：主力台式机 / 便携本)">
        </div>
      `).join('');
    }

    document.getElementById('configModal').classList.add('show');
  } catch (e) {
    showToast('读取配置失败', 'error');
  }
}

async function saveConfigModal() {
  const llm_url = document.getElementById('configLlmUrl').value.trim();
  const llm_model = document.getElementById('configLlmModel').value.trim();
  const llm_api_key = document.getElementById('configLlmKey').value.trim();
  const scanDirsEl = document.getElementById('configScanDirs');
  const scan_directories = scanDirsEl ? scanDirsEl.value.split('\n').map(s => s.trim()).filter(Boolean) : [];
  
  const machine_aliases = {};
  document.querySelectorAll('#configMachineAliasesList .machine-alias-input').forEach(input => {
    const mid = input.dataset.mid;
    const val = input.value.trim();
    if (mid && val) machine_aliases[mid] = val;
  });

  try {
    const res = await fetch('/api/config', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ llm_url, llm_model, llm_api_key, scan_directories, machine_aliases })
    });
    const data = await res.json();
    if (data.success) {
      machineAliases = machine_aliases;
      document.getElementById('configModal').classList.remove('show');
      if (currentFiltered.length > 0) loadCard(currentIndex);
      showToast('设置与设备别名已保存生效！', 'success');
    }
  } catch (e) {
    showToast('保存失败: ' + e.message, 'error');
  }
}

// 安全绑定预设按钮 (元素可能不存在)
function bindPreset(id, handler) {
  const el = document.getElementById(id);
  if (el) el.addEventListener('click', handler);
}

function escapeHtml(str) {
  if (!str) return '';
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

window.addEventListener('DOMContentLoaded', init);
