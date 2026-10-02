// client/dev_env.js
// 开发环境复现页：只读展示各机器采集到的开发环境清单与恢复命令。

const ICONS = {
  info: '<svg class="i sm" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></line></svg>',
  check: '<svg class="i sm" viewBox="0 0 24 24"><polyline points="20 6 9 17 4 12"></polyline></svg>',
  alert: '<svg class="i sm" viewBox="0 0 24 24"><path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>',
  sun: '<svg class="i sm" viewBox="0 0 24 24"><circle cx="12" cy="12" r="5"></circle><line x1="12" y1="1" x2="12" y2="3"></line><line x1="12" y1="21" x2="12" y2="23"></line><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line><line x1="1" y1="12" x2="3" y2="12"></line><line x1="21" y1="12" x2="23" y2="12"></line><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line></svg>',
  moon: '<svg class="i sm" viewBox="0 0 24 24"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path></svg>',
  chevron: '<svg class="i sm provider-chevron" viewBox="0 0 24 24"><polyline points="6 9 12 15 18 9"></polyline></svg>'
};

const toastContainer = document.getElementById('toastContainer');
const btnThemeToggle = document.getElementById('btnThemeToggle');
const themeIcon = document.getElementById('themeIcon');
const themeText = document.getElementById('themeText');
const devMachineFilter = document.getElementById('devMachineFilter');
const devEnvContent = document.getElementById('devEnvContent');
const devProviderCount = document.getElementById('devProviderCount');
const btnDevRefresh = document.getElementById('btnDevRefresh');

let machines = [];
let machineAliases = {};
let activeMachine = 'all';
const CMD_STORE = {};
let cmdSeq = 0;

function escapeHtml(str) {
  if (str === null || str === undefined) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function getMachineDisplayName(id) {
  return machineAliases[id] || id || '未知设备';
}

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

function initTheme() {
  applyTheme(localStorage.getItem('app-theme') || 'light');
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

function formatTime(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}

async function loadAll(notify) {
  try {
    const [devRes, cfgRes] = await Promise.all([
      fetch('/api/dev-env'),
      fetch('/api/config')
    ]);
    const dev = await devRes.json();
    const cfg = await cfgRes.json();
    machines = dev.machines || [];
    machineAliases = cfg.machine_aliases || {};
    renderMachineFilter();
    render();
    if (notify) showToast('开发环境清单已刷新', 'success');
  } catch (e) {
    showToast('加载开发环境清单失败: ' + e.message, 'error');
  }
}

function renderMachineFilter() {
  const opts = ['<option value="all">全部设备</option>'];
  for (const m of machines) {
    opts.push(`<option value="${escapeHtml(m.machine_id)}">${escapeHtml(getMachineDisplayName(m.machine_id))}</option>`);
  }
  devMachineFilter.innerHTML = opts.join('');
  if (activeMachine !== 'all' && machines.some(m => m.machine_id === activeMachine)) {
    devMachineFilter.value = activeMachine;
  } else {
    activeMachine = 'all';
    devMachineFilter.value = 'all';
  }
}

function renderProvider(p, evidenceBase) {
  const items = p.items || [];
  const files = p.files || [];
  const cmds = (p.restore_commands || []).map(c => String(c).replace(/\{\{EVIDENCE\}\}/g, evidenceBase));
  const disabled = p.available ? '' : ' provider-card-disabled';
  const badge = p.available
    ? `<span class="provider-badge">${items.length} 项</span>`
    : `<span class="provider-badge provider-badge-off">未检测到</span>`;

  const filesHtml = files.length
    ? `<div class="provider-files">${files.map(f => {
        const rel = `${evidenceBase}/dev-env/${f.name}`;
        const count = (f.count === undefined || f.count === null) ? '' : ` · ${f.count} 行`;
        return `<span class="provider-file" title="${escapeHtml(rel)}">${escapeHtml(f.name)}${count}</span>`;
      }).join('')}</div>`
    : '';

  const itemsHtml = items.length
    ? `<div class="provider-items">${items.map(it => `
        <div class="provider-item-row">
          <span class="provider-item-name" title="${escapeHtml(it.name)}">${escapeHtml(it.name)}</span>
          <span class="provider-item-ver" title="${escapeHtml(it.version || '')}">${escapeHtml(it.version || '')}</span>
        </div>`).join('')}</div>`
    : '';

  let cmdHtml = '';
  if (cmds.length) {
    const id = 'cmd-' + (++cmdSeq);
    CMD_STORE[id] = cmds.join('\n');
    cmdHtml = `
      <div class="cmd-wrap">
        <div class="cmd-head">
          <span>恢复命令</span>
          <button type="button" class="cmd-copy" data-cmd-id="${id}">复制</button>
        </div>
        <pre class="cmd-block">${escapeHtml(cmds.join('\n'))}</pre>
      </div>`;
  } else if (p.available) {
    cmdHtml = '<div class="cmd-none">该工具链无需额外恢复命令</div>';
  }

  return `
    <details class="provider-card${disabled}">
      <summary class="provider-summary">
        <span class="provider-label">${escapeHtml(p.label || p.id)}</span>
        <span class="provider-summary-text">${escapeHtml(p.summary || '')}</span>
        ${badge}
        ${ICONS.chevron}
      </summary>
      <div class="provider-body">
        ${filesHtml}
        ${itemsHtml}
        ${cmdHtml}
      </div>
    </details>`;
}

function render() {
  cmdSeq = 0;
  Object.keys(CMD_STORE).forEach(k => delete CMD_STORE[k]);

  if (machines.length === 0) {
    devEnvContent.innerHTML = `<div class="dev-empty">还没有采集到开发环境清单。<br>回到主表格点「扫描本机」，采集脚本会一并生成 <code>evidence/&lt;机器&gt;/dev-env.json</code>。</div>`;
    devProviderCount.innerText = '';
    return;
  }

  const visible = activeMachine === 'all'
    ? machines
    : machines.filter(m => m.machine_id === activeMachine);

  let availableCount = 0;
  const blocks = visible.map(m => {
    const providers = m.providers || [];
    availableCount += providers.filter(p => p.available).length;
    const evidenceBase = `data/evidence/${m.dir || m.machine_id}`;
    const time = m.collected_at ? `<span class="dev-machine-time">采集于 ${escapeHtml(formatTime(m.collected_at))}</span>` : '';
    const list = providers.length
      ? `<div class="provider-list">${providers.map(p => renderProvider(p, evidenceBase)).join('')}</div>`
      : '<div class="dev-empty-inline">该设备未采集到开发环境信息</div>';
    return `
      <section class="dev-machine-block">
        <div class="dev-machine-head">
          <span class="dev-machine-name">${escapeHtml(getMachineDisplayName(m.machine_id))}</span>
          <span class="dev-machine-id">${escapeHtml(m.machine_id)}</span>
          ${time}
        </div>
        ${list}
      </section>`;
  }).join('');

  devEnvContent.innerHTML = blocks;
  devProviderCount.innerText = `${availableCount} 个可用工具链`;
}

async function copyText(text, btn) {
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
  const old = btn.innerText;
  btn.innerText = ok ? '已复制' : '复制失败';
  setTimeout(() => { btn.innerText = old; }, 1200);
}

function bindEvents() {
  btnThemeToggle.addEventListener('click', () => {
    const dark = document.documentElement.getAttribute('data-theme') === 'dark';
    applyTheme(dark ? 'light' : 'dark');
  });

  devMachineFilter.addEventListener('change', () => {
    activeMachine = devMachineFilter.value;
    render();
  });

  btnDevRefresh.addEventListener('click', () => loadAll(true));

  devEnvContent.addEventListener('click', (e) => {
    const btn = e.target.closest('.cmd-copy');
    if (!btn) return;
    const text = CMD_STORE[btn.dataset.cmdId];
    if (text) copyText(text, btn);
  });
}

initTheme();
render();
bindEvents();
loadAll(false);
