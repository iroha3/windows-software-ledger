// client/browsers.js
// 浏览器扩展台账：主页（软件台账）的同构副台账。
//   - 扫描字段（名称 / 版本 / ID / 状态 / 来源 / 设备 / 配置）只读
//   - 用户层：保留意愿 / 准备进度 / 精选 / 备注 / 附件，存 data/extensions.json + data/vault/<本机>/ext/<ID>/
// 绝不读取扩展存储数据、Cookie 或密码。

const REPO_URL = 'https://github.com/iroha3/windows-software-ledger';
const HOMEPAGE_URL = 'https://iroha3.github.io/windows-software-ledger/';

const ICONS = {
  info: '<svg class="i sm" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></line></svg>',
  check: '<svg class="i sm" viewBox="0 0 24 24"><polyline points="20 6 9 17 4 12"></polyline></svg>',
  alert: '<svg class="i sm" viewBox="0 0 24 24"><path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>',
  sun: '<svg class="i sm" viewBox="0 0 24 24"><circle cx="12" cy="12" r="5"></circle><line x1="12" y1="1" x2="12" y2="3"></line><line x1="12" y1="21" x2="12" y2="23"></line><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line><line x1="1" y1="12" x2="3" y2="12"></line><line x1="21" y1="12" x2="23" y2="12"></line><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line></svg>',
  moon: '<svg class="i sm" viewBox="0 0 24 24"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path></svg>',
  star: '<svg class="i sm" viewBox="0 0 24 24"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>',
  starFilled: '<svg class="i sm fill" viewBox="0 0 24 24" style="color:#c9a24a;"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>',
  external: '<svg class="i sm" viewBox="0 0 24 24"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path><polyline points="15 3 21 3 21 9"></polyline><line x1="10" y1="14" x2="21" y2="3"></line></svg>',
  paperclip: '<svg class="i sm" viewBox="0 0 24 24"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"></path></svg>',
};

const INTENT_LABEL = { must: '必须恢复', should: '建议恢复', on_demand: '用到再装', drop: '淘汰弃用', unreviewed: '待确认' };

const state = {
  machines: [],          // 原始证据
  aliases: {},
  extList: [],           // 扁平扩展 + 用户层
  filters: { machine: 'all', browser: 'all', intent: 'all', prep: 'all', enabled: 'all', awesome: false, attach: false, query: '' },
  selected: new Set(),
  activeKey: null,
};

let drawerVault = null;

// ---------- DOM ----------
const $ = (id) => document.getElementById(id);
const toastContainer = $('toastContainer');
const themeIcon = $('themeIcon');
const themeText = $('themeText');
const machineTabs = $('extMachineTabs');
const searchInput = $('extSearch');
const browserFilter = $('extBrowserFilter');
const intentFilter = $('extIntentFilter');
const prepFilter = $('extPrepFilter');
const awesomeFilter = $('extAwesomeFilter');
const attachFilter = $('extAttachFilter');
const tableBody = $('extTableBody');
const selectAll = $('selectAllExt');
const batchBar = $('extBatchBar');
const batchInfo = $('extBatchInfo');
const drawer = $('extDrawer');
const drawerOverlay = $('extDrawerOverlay');

function escapeHtml(s) {
  if (s === null || s === undefined) return '';
  return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}
function getMachineDisplayName(id) { return state.aliases[id] || id || '未知设备'; }

// Firefox 配置目录形如 xc8zepzv.default-release，前缀是随机串，展示时剥掉
function cleanProfile(p) {
  if (!p) return '默认配置';
  const m = String(p).match(/^[a-z0-9]{6,}\.(.+)$/i);
  return m ? m[1] : p;
}

// 浏览器品牌图标（静态资源在 client/browser-icons/，Tauri 内嵌资源直接按路径加载）
const BROWSER_ICON_FILES = {
  edge: 'edge', chrome: 'chrome', brave: 'brave', vivaldi: 'vivaldi',
  chromium: 'chromium', opera: 'opera', opera_gx: 'opera-gx', firefox: 'firefox',
};
function browserIconHtml(id, label) {
  const alt = escapeHtml(label || id || '浏览器');
  const file = BROWSER_ICON_FILES[id];
  if (file) return `<img class="browser-ico" src="/browser-icons/${file}.svg" alt="${alt}" title="${alt}" loading="lazy">`;
  return `<svg class="i sm browser-ico-generic" viewBox="0 0 24 24" title="${alt}"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>`;
}

function showToast(message, type = 'info', duration = 2200) {
  const toast = document.createElement('div');
  toast.className = `toast toast-${type}`;
  let icon = ICONS.info;
  if (type === 'success') icon = ICONS.check;
  if (type === 'warning' || type === 'error') icon = ICONS.alert;
  toast.innerHTML = `<span style="display:inline-flex;align-items:center;">${icon}</span><span>${escapeHtml(message)}</span>`;
  toastContainer.appendChild(toast);
  setTimeout(() => { toast.style.opacity = '0'; toast.style.transform = 'translateY(-10px) scale(0.95)'; setTimeout(() => toast.remove(), 200); }, duration);
}

function applyTheme(theme) {
  if (theme === 'dark') { document.documentElement.setAttribute('data-theme', 'dark'); themeIcon.innerHTML = ICONS.sun; themeText.innerText = '日间'; }
  else { document.documentElement.removeAttribute('data-theme'); themeIcon.innerHTML = ICONS.moon; themeText.innerText = '夜间'; }
  localStorage.setItem('app-theme', theme);
}

// ---------- 数据 ----------
function flatten() {
  const list = [];
  for (const m of state.machines) {
    for (const b of m.browsers || []) {
      for (const p of b.profiles || []) {
        for (const ext of p.extensions || []) {
          list.push({
            key: `${m.machine_id}|${b.id || b.label}|${p.profile || ''}|${ext.id || ext.name}`,
            machineId: m.machine_id,
            browserId: b.id || b.label || '',
            browserLabel: b.label || b.id || '',
            profile: p.profile || '',
            id: ext.id || '',
            name: ext.name || '(未命名)',
            version: ext.version || '',
            enabled: ext.enabled,
            storeUrl: ext.store_url || '',
            notes: ext.notes || '',
            intent: ext.restore_intent || 'unreviewed',
            prep: ext.prep_status || 'todo',
            awesome: !!ext.is_awesome,
            attachCount: ext.vault_count || 0,
          });
        }
      }
    }
  }
  return list;
}

function uniqueBrowsers() {
  const map = new Map();
  for (const m of state.machines) for (const b of m.browsers || []) {
    const key = b.id || b.label;
    if (key && !map.has(key)) map.set(key, b.label || key);
  }
  return Array.from(map.entries()).sort((a, b) => String(a[1]).localeCompare(String(b[1])));
}

function getFiltered() {
  const f = state.filters;
  const q = f.query.toLowerCase();
  return state.extList.filter((e) => {
    if (f.machine !== 'all' && e.machineId !== f.machine) return false;
    if (f.browser !== 'all' && e.browserId !== f.browser) return false;
    if (f.intent !== 'all' && e.intent !== f.intent) return false;
    if (f.prep !== 'all' && e.prep !== f.prep) return false;
    if (f.enabled === 'true' && e.enabled !== true) return false;
    if (f.enabled === 'false' && e.enabled !== false) return false;
    if (f.awesome && !e.awesome) return false;
    if (f.attach && !e.attachCount) return false;
    if (q) {
      const hit = e.name.toLowerCase().includes(q) || String(e.id).toLowerCase().includes(q) || e.notes.toLowerCase().includes(q) || e.browserLabel.toLowerCase().includes(q) || String(e.storeUrl).toLowerCase().includes(q);
      if (!hit) return false;
    }
    return true;
  });
}

const getActive = () => state.extList.find((e) => e.key === state.activeKey) || null;

function applyLocal(id, fields) {
  for (const e of state.extList) {
    if (e.id !== id) continue;
    if ('restore_intent' in fields) e.intent = fields.restore_intent;
    if ('prep_status' in fields) e.prep = fields.prep_status;
    if ('is_awesome' in fields) e.awesome = !!fields.is_awesome;
    if ('notes' in fields) e.notes = fields.notes;
    if ('store_url' in fields) e.storeUrl = fields.store_url;
  }
}

async function saveExt(id, fields) {
  applyLocal(id, fields);
  try {
    await fetch('/api/extension/update', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ id, fields }) });
  } catch (e) {
    showToast('保存失败: ' + e.message, 'error');
  }
}

// ---------- 渲染 ----------
function statusHtml(enabled) {
  if (enabled === true) return '<span class="ext-status on">启用</span>';
  if (enabled === false) return '<span class="ext-status off">已停用</span>';
  return '<span class="ext-status unknown">—</span>';
}

function buildRowHtml(e) {
  const sel = state.selected.has(e.key);
  const intentClass = `intent-${e.intent}`;
  const profile = cleanProfile(e.profile);
  const machineBadge = `<span class="badge-machine" title="设备ID: ${escapeHtml(e.machineId)}">${escapeHtml(getMachineDisplayName(e.machineId))}</span>`;
  const storeCell = `<div class="ext-store-cell"><input type="text" class="cell-input ext-store-input" data-field="store_url" data-id="${escapeHtml(e.id)}" value="${escapeHtml(e.storeUrl)}" placeholder="https://…"><button type="button" class="vault-icon-btn ext-store-open" data-action="open-url" data-url="${escapeHtml(e.storeUrl)}" title="打开商店页" ${e.storeUrl ? '' : 'disabled'}>${ICONS.external}</button></div>`;
  const attach = `<button type="button" class="vault-icon-btn ext-attach-btn ${e.attachCount > 0 ? 'has-files' : ''}" data-action="attach" data-key="${escapeHtml(e.key)}" title="附件归档（可拖入或选择文件）">${ICONS.paperclip}<span class="vault-count" ${e.attachCount > 0 ? '' : 'hidden'}>${e.attachCount}</span></button>`;
  const opts = ['must', 'should', 'on_demand', 'drop', 'unreviewed'].map((v) => `<option value="${v}" ${e.intent === v ? 'selected' : ''}>${INTENT_LABEL[v]}</option>`).join('');

  return `
    <tr class="${sel ? 'selected' : ''}" data-key="${escapeHtml(e.key)}">
      <td style="text-align:center;"><input type="checkbox" class="row-checkbox" data-key="${escapeHtml(e.key)}" ${sel ? 'checked' : ''}></td>
      <td style="text-align:center;">
        <span class="awesome-star ${e.awesome ? 'starred' : ''}" data-action="toggle-awesome" data-id="${escapeHtml(e.id)}" title="${e.awesome ? '取消精选' : '设为精选'}">${e.awesome ? ICONS.starFilled : ICONS.star}</span>
      </td>
      <td>
        <div class="software-name-cell">
          <span class="software-title" data-action="open-drawer" data-key="${escapeHtml(e.key)}">${escapeHtml(e.name)}</span>
          <div class="software-tags"><span class="tag-cat">${browserIconHtml(e.browserId, e.browserLabel)}${escapeHtml(e.browserLabel || '浏览器')}</span><span class="tag-form">${escapeHtml(profile)}</span></div>
        </div>
      </td>
      <td><span class="version-badge">${escapeHtml(e.version || '—')}</span></td>
      <td>${statusHtml(e.enabled)}</td>
      <td>${machineBadge}</td>
      <td><select class="badge-select ${intentClass}" data-field="restore_intent" data-id="${escapeHtml(e.id)}">${opts}</select></td>
      <td style="text-align:center;">
        <button type="button" class="prep-badge toggle-mini ${e.prep === 'ready' ? 'prep-ready' : 'prep-todo'}" data-action="toggle-prep" data-id="${escapeHtml(e.id)}" title="点击切换：待办 / 就绪">
          <span class="status-dot dot-${e.prep === 'ready' ? 'ready' : 'unreviewed'}"></span>${e.prep === 'ready' ? '就绪' : '待办'}
        </button>
      </td>
      <td>${storeCell}</td>
      <td><input type="text" class="cell-input" data-field="notes" data-id="${escapeHtml(e.id)}" value="${escapeHtml(e.notes)}" placeholder="备注..."></td>
      <td style="text-align:center;">${attach}</td>
    </tr>`;
}

function renderTable() {
  const rows = getFiltered();
  const active = getActive();
  if (!rows.length) {
    tableBody.innerHTML = `<tr><td colspan="11" class="ext-empty">没有匹配的扩展。点右上角「刷新」，或先回主页面「扫描本机」。</td></tr>`;
  } else {
    tableBody.innerHTML = rows.map(buildRowHtml).join('');
  }
  if (active && !rows.some((e) => e.key === active.key)) closeDrawer();
}

function renderStats() {
  const all = state.extList;
  const c = (fn) => all.filter(fn).length;
  $('extStatTotal').innerText = all.length;
  $('extStatOn').innerText = c((e) => e.enabled === true);
  $('extStatOff').innerText = c((e) => e.enabled === false);
  $('extStatMust').innerText = c((e) => e.intent === 'must');
  $('extStatShould').innerText = c((e) => e.intent === 'should');
  $('extStatOnDemand').innerText = c((e) => e.intent === 'on_demand');
  $('extStatDrop').innerText = c((e) => e.intent === 'drop');
  $('extStatUnreviewed').innerText = c((e) => e.intent === 'unreviewed');
  $('extStatAttach').innerText = c((e) => e.attachCount > 0);
}

function renderMachineTabs() {
  const counts = new Map();
  for (const e of state.extList) counts.set(e.machineId, (counts.get(e.machineId) || 0) + 1);
  const tabs = [`<button class="tab-btn ${state.filters.machine === 'all' ? 'active' : ''}" data-machine="all">全部设备 (${state.extList.length})</button>`];
  for (const m of state.machines) {
    const mid = m.machine_id;
    tabs.push(`<button class="tab-btn ${state.filters.machine === mid ? 'active' : ''}" data-machine="${escapeHtml(mid)}">${escapeHtml(getMachineDisplayName(mid))} (${counts.get(mid) || 0})</button>`);
  }
  machineTabs.innerHTML = tabs.join('');
}

function renderBrowserFilter() {
  const opts = ['<option value="all">全部浏览器</option>'];
  for (const [id, label] of uniqueBrowsers()) opts.push(`<option value="${escapeHtml(id)}">${escapeHtml(label)}</option>`);
  browserFilter.innerHTML = opts.join('');
  browserFilter.value = state.filters.browser;
}

function render() {
  renderMachineTabs();
  renderBrowserFilter();
  renderStats();
  renderTable();
  updateBatchBar();
}

// ---------- 选择 / 批量 ----------
function selectedIds() {
  const ids = new Set();
  for (const k of state.selected) { const e = state.extList.find((x) => x.key === k); if (e && e.id) ids.add(e.id); }
  return Array.from(ids);
}

function updateBatchBar() {
  const n = state.selected.size;
  batchInfo.innerText = `已选 ${n} 项`;
  batchBar.classList.toggle('show', n > 0);
  const rows = getFiltered();
  selectAll.checked = rows.length > 0 && rows.every((e) => state.selected.has(e.key));
}

function clearSelection() { state.selected.clear(); renderTable(); updateBatchBar(); }

async function batchUpdate(fields) {
  const ids = selectedIds();
  if (!ids.length) return;
  for (const id of ids) await saveExt(id, fields);
  renderTable(); renderStats(); updateBatchBar();
}

// ---------- 抽屉 ----------
function openDrawer(key) {
  const e = state.extList.find((x) => x.key === key);
  if (!e) return;
  state.activeKey = key;
  renderDrawer();
  drawer.classList.add('show');
  drawerOverlay.classList.add('show');
  if (drawerVault) drawerVault.setTarget(e.id);
}

function closeDrawer() {
  state.activeKey = null;
  drawer.classList.remove('show');
  drawerOverlay.classList.remove('show');
  if (drawerVault) drawerVault.collapse();
}

function renderDrawer() {
  const e = getActive();
  if (!e) return;
  $('extDrawerChip').innerText = (e.browserLabel || 'EXT').slice(0, 6);
  $('extDrawerName').value = e.name;
  $('extDrawerBrowser').value = e.browserLabel || '—';
  $('extDrawerProfile').value = cleanProfile(e.profile);
  $('extDrawerVersion').value = e.version || '—';
  $('extDrawerEnabled').value = e.enabled === true ? '启用' : (e.enabled === false ? '已停用' : '未知');
  $('extDrawerAwesome').checked = e.awesome;
  $('extDrawerAwesomePill').classList.toggle('active', e.awesome);
  document.querySelectorAll('#extDrawerIntentSegmented .intent-seg-btn').forEach((b) => b.classList.toggle('active', b.dataset.intent === e.intent));
  const prepReady = e.prep === 'ready';
  $('extDrawerPrepToggle').className = `prep-badge toggle-pill-btn ${prepReady ? 'prep-ready' : 'prep-todo'}`;
  $('extDrawerPrepDot').className = `status-dot dot-${prepReady ? 'ready' : 'unreviewed'}`;
  $('extDrawerPrepText').innerText = prepReady ? '就绪' : '待办';
  $('extDrawerNotes').value = e.notes;
  $('extDrawerFullId').innerText = e.id || '—';
  $('extDrawerMachine').innerText = getMachineDisplayName(e.machineId);
  const src = $('extDrawerSource');
  if (e.storeUrl) { src.innerText = e.storeUrl; src.dataset.url = e.storeUrl; $('extDrawerSourceRow').style.display = ''; }
  else { $('extDrawerSourceRow').style.display = 'none'; }
}

function drawerNav(dir) {
  const rows = getFiltered();
  if (!rows.length) return;
  let i = rows.findIndex((e) => e.key === state.activeKey);
  if (i === -1) i = 0;
  i = (i + dir + rows.length) % rows.length;
  openDrawer(rows[i].key);
}

// ---------- 配置弹窗 ----------
function renderMachineAliases(cfg) {
  const wrap = $('configMachineAliasesList');
  const ids = state.machines.map((m) => m.machine_id).filter(Boolean);
  if (!ids.length) { wrap.innerHTML = '<span class="hint">尚未扫描到任何设备。</span>'; return; }
  wrap.innerHTML = ids.map((id) => `
    <div class="machine-alias-row" style="display:flex;align-items:center;gap:8px;margin-bottom:6px;">
      <span style="font-family:var(--mono);font-size:11.5px;color:var(--ink-3);flex:0 0 160px;overflow-wrap:anywhere;">${escapeHtml(id)}</span>
      <input type="text" class="cell-input" data-alias-id="${escapeHtml(id)}" value="${escapeHtml(state.aliases[id] || '')}" placeholder="友好别名">
    </div>`).join('');
}

async function openConfigModal() {
  try {
    const cfg = await (await fetch('/api/config')).json();
    $('configScanDirs').value = (cfg.scan_directories || []).join('\n');
    $('configLlmUrl').value = cfg.llm_url || '';
    $('configLlmModel').value = cfg.llm_model || '';
    $('configLlmKey').value = cfg.llm_api_key || '';
    state.aliases = cfg.machine_aliases || {};
    renderMachineAliases(cfg);
  } catch (e) { showToast('读取设置失败: ' + e.message, 'error'); }
  $('configModal').classList.add('show');
}

async function saveConfigModal() {
  const aliases = {};
  document.querySelectorAll('#configMachineAliasesList input[data-alias-id]').forEach((inp) => {
    const v = inp.value.trim();
    if (v) aliases[inp.dataset.aliasId] = v;
  });
  const payload = {
    scan_directories: $('configScanDirs').value.split('\n').map((s) => s.trim()).filter(Boolean),
    llm_url: $('configLlmUrl').value.trim(),
    llm_model: $('configLlmModel').value.trim(),
    llm_api_key: $('configLlmKey').value.trim(),
    machine_aliases: aliases,
  };
  try {
    await fetch('/api/config', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
    state.aliases = aliases;
    render(); if (getActive()) renderDrawer();
    showToast('设置已保存', 'success');
    $('configModal').classList.remove('show');
  } catch (e) { showToast('保存设置失败: ' + e.message, 'error'); }
}

// ---------- 加载 ----------
async function loadAll(notify) {
  try {
    const [extRes, cfgRes] = await Promise.all([fetch('/api/browser-extensions'), fetch('/api/config')]);
    const ext = await extRes.json();
    const cfg = await cfgRes.json();
    state.machines = ext.machines || [];
    state.aliases = cfg.machine_aliases || {};
    state.extList = flatten();
    // 保活抽屉
    if (state.activeKey && !state.extList.some((e) => e.key === state.activeKey)) state.activeKey = null;
    for (const k of Array.from(state.selected)) if (!state.extList.some((e) => e.key === k)) state.selected.delete(k);
    render();
    if (getActive()) renderDrawer();
    if (notify) showToast('浏览器扩展清单已刷新', 'success');
  } catch (e) {
    showToast('加载浏览器扩展清单失败: ' + e.message, 'error');
  }
}

// ---------- 事件 ----------
function bindEvents() {
  $('btnThemeToggle').addEventListener('click', () => applyTheme((localStorage.getItem('app-theme') || 'light') === 'dark' ? 'light' : 'dark'));
  $('btnExtRefresh').addEventListener('click', () => loadAll(true));
  $('btnConfig').addEventListener('click', openConfigModal);
  $('btnSaveConfig').addEventListener('click', saveConfigModal);
  $('presetLocal').addEventListener('click', () => { $('configLlmUrl').value = 'http://127.0.0.1:1234/v1/chat/completions'; $('configLlmModel').value = 'qwen3.5-4b'; });
  $('presetDeepseek').addEventListener('click', () => { $('configLlmUrl').value = 'https://api.deepseek.com/v1/chat/completions'; $('configLlmModel').value = 'deepseek-chat'; });
  $('btnAbout').addEventListener('click', () => $('aboutModal').classList.add('show'));
  $('aboutGitHub').addEventListener('click', (ev) => { ev.preventDefault(); window.openExternal(REPO_URL); });
  $('aboutHomepage').addEventListener('click', (ev) => { ev.preventDefault(); window.openExternal(HOMEPAGE_URL); });

  machineTabs.addEventListener('click', (ev) => {
    const b = ev.target.closest('.tab-btn');
    if (!b) return;
    state.filters.machine = b.dataset.machine;
    render();
  });
  searchInput.addEventListener('input', () => { state.filters.query = searchInput.value.trim(); renderTable(); updateBatchBar(); });
  browserFilter.addEventListener('change', () => { state.filters.browser = browserFilter.value; renderTable(); updateBatchBar(); });
  intentFilter.addEventListener('change', () => { state.filters.intent = intentFilter.value; renderTable(); updateBatchBar(); });
  prepFilter.addEventListener('change', () => { state.filters.prep = prepFilter.value; renderTable(); updateBatchBar(); });
  awesomeFilter.addEventListener('change', () => { state.filters.awesome = awesomeFilter.checked; renderTable(); updateBatchBar(); });
  attachFilter.addEventListener('change', () => { state.filters.attach = attachFilter.checked; renderTable(); updateBatchBar(); });

  // 统计条点击即筛选
  $('extStatsBar').addEventListener('click', (ev) => {
    const item = ev.target.closest('.stat-item');
    if (!item) return;
    if (item.dataset.filterIntent !== undefined) {
      state.filters.intent = item.dataset.filterIntent;
      intentFilter.value = state.filters.intent;
      state.filters.enabled = 'all';
    }
    if (item.dataset.filterEnabled !== undefined) {
      state.filters.enabled = item.dataset.filterEnabled;
      state.filters.intent = 'all';
      intentFilter.value = 'all';
    }
    if (item.dataset.filterAttach !== undefined) {
      state.filters.attach = true;
      attachFilter.checked = true;
    }
    renderTable(); updateBatchBar();
  });

  // 表格交互
  tableBody.addEventListener('click', async (ev) => {
    const toggle = ev.target.closest('[data-action="toggle-awesome"]');
    if (toggle) { const id = toggle.dataset.id; const e = state.extList.find((x) => x.id === id); if (e) await saveExt(id, { is_awesome: !e.awesome }); renderTable(); renderStats(); if (getActive() && getActive().id === id) renderDrawer(); return; }
    const prep = ev.target.closest('[data-action="toggle-prep"]');
    if (prep) { const id = prep.dataset.id; const e = state.extList.find((x) => x.id === id); if (e) await saveExt(id, { prep_status: e.prep === 'ready' ? 'todo' : 'ready' }); renderTable(); renderStats(); if (getActive() && getActive().id === id) renderDrawer(); return; }
    const open = ev.target.closest('[data-action="open-drawer"]');
    if (open) { openDrawer(open.dataset.key); return; }
    const attach = ev.target.closest('[data-action="attach"]');
    if (attach) { openDrawer(attach.dataset.key); if (drawerVault) drawerVault.expand(); return; }
    const url = ev.target.closest('[data-action="open-url"]');
    if (url) { ev.preventDefault(); window.openExternal(url.dataset.url); }
  });
  tableBody.addEventListener('change', async (ev) => {
    const cb = ev.target.closest('.row-checkbox');
    if (cb) { if (cb.checked) state.selected.add(cb.dataset.key); else state.selected.delete(cb.dataset.key); updateBatchBar(); cb.closest('tr').classList.toggle('selected', cb.checked); return; }
    const sel = ev.target.closest('select[data-field="restore_intent"]');
    if (sel) { await saveExt(sel.dataset.id, { restore_intent: sel.value }); renderTable(); renderStats(); if (getActive() && getActive().id === sel.dataset.id) renderDrawer(); return; }
    const note = ev.target.closest('input[data-field="notes"]');
    if (note) { await saveExt(note.dataset.id, { notes: note.value.trim() }); showToast('备注已保存', 'success', 1400); return; }
    const store = ev.target.closest('input[data-field="store_url"]');
    if (store) {
      const v = store.value.trim();
      await saveExt(store.dataset.id, { store_url: v });
      const btn = store.closest('.ext-store-cell') && store.closest('.ext-store-cell').querySelector('.ext-store-open');
      if (btn) { btn.dataset.url = v; if (v) btn.removeAttribute('disabled'); else btn.setAttribute('disabled', ''); }
      showToast('商店链接已保存', 'success', 1400);
      return;
    }
  });
  tableBody.addEventListener('keydown', (ev) => { if (ev.key === 'Enter' && ev.target.classList.contains('cell-input')) ev.target.blur(); });

  selectAll.addEventListener('change', () => {
    const rows = getFiltered();
    if (selectAll.checked) rows.forEach((e) => state.selected.add(e.key));
    else rows.forEach((e) => state.selected.delete(e.key));
    renderTable(); updateBatchBar();
  });

  // 批量条
  batchBar.addEventListener('click', async (ev) => {
    const intentBtn = ev.target.closest('[data-batch-intent]');
    if (intentBtn) { await batchUpdate({ restore_intent: intentBtn.dataset.batchIntent }); return; }
    if (ev.target.closest('#btnExtBatchReady')) { await batchUpdate({ prep_status: 'ready' }); return; }
    if (ev.target.closest('#btnExtBatchReset')) { await batchUpdate({ restore_intent: 'unreviewed', prep_status: 'todo', is_awesome: false, notes: '' }); return; }
    if (ev.target.closest('#btnExtBatchClear')) clearSelection();
  });

  // 抽屉
  $('extDrawerClose').addEventListener('click', closeDrawer);
  $('btnExtDrawerDone').addEventListener('click', closeDrawer);
  drawerOverlay.addEventListener('click', closeDrawer);
  $('extDrawerIntentSegmented').addEventListener('click', async (ev) => {
    const b = ev.target.closest('.intent-seg-btn'); const e = getActive();
    if (!b || !e) return;
    e.intent = b.dataset.intent;
    renderDrawer(); renderTable(); renderStats();
    await saveExt(e.id, { restore_intent: b.dataset.intent });
  });
  $('extDrawerPrepToggle').addEventListener('click', async () => {
    const e = getActive(); if (!e) return;
    e.prep = e.prep === 'ready' ? 'todo' : 'ready';
    renderDrawer(); renderTable(); renderStats();
    await saveExt(e.id, { prep_status: e.prep });
  });
  $('extDrawerAwesome').addEventListener('change', async () => {
    const e = getActive(); if (!e) return;
    e.awesome = $('extDrawerAwesome').checked;
    renderDrawer(); renderTable(); renderStats();
    await saveExt(e.id, { is_awesome: e.awesome });
  });
  $('extDrawerNotes').addEventListener('change', async () => {
    const e = getActive(); if (!e) return;
    e.notes = $('extDrawerNotes').value.trim();
    renderTable();
    await saveExt(e.id, { notes: e.notes });
    showToast('备注已保存', 'success', 1400);
  });
  $('btnExtReset').addEventListener('click', async () => {
    const e = getActive(); if (!e) return;
    Object.assign(e, { intent: 'unreviewed', prep: 'todo', awesome: false, notes: '' });
    renderDrawer(); renderTable(); renderStats();
    await saveExt(e.id, { restore_intent: 'unreviewed', prep_status: 'todo', is_awesome: false, notes: '' });
    showToast('已恢复默认', 'info');
  });
  $('extDrawerSource').addEventListener('click', (ev) => { ev.preventDefault(); const u = ev.currentTarget.dataset.url; if (u) window.openExternal(u); });

  document.addEventListener('keydown', (ev) => {
    const typing = /^(input|textarea|select)$/i.test(document.activeElement && document.activeElement.tagName);
    if (ev.key === 'Escape') {
      if (document.querySelector('.modal-overlay.show')) { document.querySelectorAll('.modal-overlay.show').forEach((m) => m.classList.remove('show')); return; }
      if (drawer.classList.contains('show')) { closeDrawer(); return; }
      if (state.selected.size) { clearSelection(); return; }
    }
    if (typing) return;
    if (ev.key === '/') { ev.preventDefault(); searchInput.focus(); return; }
    if (drawer.classList.contains('show') && ['1', '2', '3', '4', '5'].includes(ev.key)) {
      const map = { 1: 'must', 2: 'should', 3: 'on_demand', 4: 'drop', 5: 'unreviewed' };
      const e = getActive(); if (e) { e.intent = map[ev.key]; renderDrawer(); renderTable(); renderStats(); saveExt(e.id, { restore_intent: e.intent }); }
      return;
    }
    if (ev.key === 'PageUp') { ev.preventDefault(); drawerNav(-1); }
    if (ev.key === 'PageDown') { ev.preventDefault(); drawerNav(1); }
  });
}

function init() {
  applyTheme(localStorage.getItem('app-theme') || 'light');
  drawerVault = window.Vault ? window.Vault.mount({
    pill: 'extDrawerVaultPill', panel: 'extDrawerVaultPanel', list: 'extDrawerVaultList',
    add: 'extDrawerVaultAdd', count: 'extDrawerVaultCount', sub: 'extDrawerVaultSub', kind: 'ext', id: null,
  }) : null;
  bindEvents();
  loadAll(false);
}

init();
