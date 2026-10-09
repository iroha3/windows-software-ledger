// client/vault.js
// 通用文件保管箱前端组件：可复用于软件（kind=soft）、浏览器（kind=browser）、
// 扩展附件（kind=ext），id 一律是对应实体的 uuid。
// 设计原则：
//   - 只搬运用户手动放入的文件，绝不自动采集敏感内容
//   - 默认收起，点 pill 就地展开；也可无 pill，由外部调 expand()
//   - 保存 / 删除用与全站一致的线性图标，文件名完整显示不截断

// ---------- 机器配色（全局，供主页/卡片/浏览器页共用） ----------
// 每台机器按 machine_id 哈希到固定色相，同一台机器到哪都是同一个颜色。
function machineColor(id) {
  const s = String(id == null ? '' : id);
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
  return `hsl(${h % 360} 62% 48%)`;
}

function machineChipDot(id) {
  return `<i class="machine-chip-dot" style="--mc:${machineColor(id)}"></i>`;
}

// 带机器色的电脑图标（页签等需要机器标识的地方用）。
function machineDeviceIcon(id) {
  return `<span class="machine-icon" style="--mc:${machineColor(id)}"><svg class="i sm" viewBox="0 0 24 24"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg></span>`;
}

// 把 [{machine_id,...}] 渲染成带色标签；labelOf(machine) 决定显示名。
function machineChips(machines, labelOf) {
  const list = machines || [];
  if (!list.length) return '<span class="machine-chip-none">无机器记录</span>';
  const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
  return list.map((m) => {
    const id = typeof m === 'string' ? m : (m.machine_id || '');
    const label = typeof m === 'string' ? m : (labelOf ? labelOf(m) : (m.machine_id || ''));
    return `<span class="machine-chip" style="--mc:${machineColor(id)}">${machineChipDot(id)}${esc(label)}</span>`;
  }).join('');
}

(function () {
  const $ = (id) => document.getElementById(id);

  const ICONS = {
    save: '<svg class="i sm" viewBox="0 0 24 24"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>',
    del: '<svg class="i sm" viewBox="0 0 24 24"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>',
    run: '<svg class="i sm" viewBox="0 0 24 24"><polygon points="6 4 20 12 6 20 6 4"></polygon></svg>',
  };

  async function api(path, payload) {
    const res = await fetch(path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload || {}),
    });
    return res.json();
  }

  function esc(s) {
    return String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({
      '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
    }[c]));
  }

  function toast(msg) {
    if (typeof window.showToast === 'function') window.showToast(msg, 'info');
  }

  function mount(opts) {
    const pill = opts.pill ? $(opts.pill) : null;
    const panel = $(opts.panel);
    const list = $(opts.list);
    const addBtn = $(opts.add);
    const countEl = opts.count ? $(opts.count) : null;
    const subEl = opts.sub ? $(opts.sub) : null;
    if (!panel || !list || !addBtn) return null;

    const kind = opts.kind;
    let id = opts.id || null;
    let files = [];
    let dropOff = null;

    function setExpanded(expanded) {
      panel.hidden = !expanded;
      if (pill) pill.classList.toggle('active', expanded);
      // 仅在展开时监听拖入，收起即解除，避免多个保管箱同时抢事件
      if (expanded && !dropOff && typeof window.onFileDrop === 'function') {
        dropOff = window.onFileDrop((paths) => { addFiles(paths); });
      } else if (!expanded && dropOff) {
        dropOff();
        dropOff = null;
      }
    }

    function renderSub() {
      if (!subEl) return;
      // 归档目录以实体 uuid 为键，不再分主机名。
      subEl.textContent = id ? `data/vault/${kind}/${id}/` : '';
    }

    function render() {
      if (countEl) {
        countEl.hidden = files.length === 0;
        countEl.textContent = String(files.length);
      }
      if (pill) pill.classList.toggle('has-files', files.length > 0);
      renderSub();
      if (!files.length) {
        list.innerHTML = '<li class="vault-empty">暂无归档文件</li>';
        return;
      }
      list.innerHTML = files.map((f) => `
        <li class="vault-file">
          <button type="button" class="vault-file-name" data-name="${esc(f.name)}" title="运行 / 用默认程序打开 ${esc(f.name)}">${esc(f.name)}</button>
          <button type="button" class="vault-file-btn vault-file-run" data-name="${esc(f.name)}" title="运行 / 用默认程序打开">${ICONS.run}</button>
          <button type="button" class="vault-file-btn vault-file-save" data-name="${esc(f.name)}" title="保存到…">${ICONS.save}</button>
          <button type="button" class="vault-file-btn vault-file-del" data-name="${esc(f.name)}" title="删除此归档文件">${ICONS.del}</button>
        </li>`).join('');
    }

    async function refresh() {
      if (!id) { files = []; render(); return; }
      try {
        const res = await api('/api/vault/list', { kind, id });
        files = (res && res.files) || [];
      } catch (e) {
        files = [];
      }
      render();
    }

    async function addFiles(paths) {
      if (!id || !paths || !paths.length) return;
      try {
        const res = await api('/api/vault/add', { kind, id, paths });
        if (res && res.success) {
          const n = res.added || 0;
          toast(n ? `已归档 ${n} 个文件` : '没有可归档的文件');
        } else {
          toast((res && res.error) || '归档失败');
        }
      } catch (e) {
        toast('归档失败: ' + e.message);
      }
      await refresh();
    }

    async function pickFiles() {
      if (!id) return;
      if (typeof window.dialogOpen !== 'function') { toast('当前环境不支持文件选择'); return; }
      const picked = await window.dialogOpen({ multiple: true, title: '选择要归档的配置文件' });
      if (!picked) return;
      await addFiles(typeof picked === 'string' ? [picked] : picked);
    }

    async function saveFile(name) {
      if (typeof window.dialogSave !== 'function') { toast('当前环境不支持另存为'); return; }
      const dest = await window.dialogSave({ defaultPath: name, title: '保存归档文件到…' });
      if (!dest) return;
      try {
        const res = await api('/api/vault/export', { kind, id, name, dest });
        toast(res && res.success ? '已保存' : ((res && res.error) || '保存失败'));
      } catch (e) {
        toast('保存失败: ' + e.message);
      }
    }

    async function openFile(name) {
      try {
        const res = await api('/api/vault/open', { kind, id, name });
        if (!res || !res.success) toast((res && res.error) || '打开失败');
      } catch (e) {
        toast('打开失败: ' + e.message);
      }
    }

    async function deleteFile(name) {
      try {
        const res = await api('/api/vault/delete', { kind, id, name });
        toast(res && res.success ? '已删除' : ((res && res.error) || '删除失败'));
      } catch (e) {
        toast('删除失败: ' + e.message);
      }
      await refresh();
    }

    if (pill) pill.addEventListener('click', () => setExpanded(panel.hidden));
    addBtn.addEventListener('click', pickFiles);
    list.addEventListener('click', (e) => {
      const del = e.target.closest('.vault-file-del');
      if (del) { deleteFile(del.getAttribute('data-name')); return; }
      const save = e.target.closest('.vault-file-save');
      if (save) { saveFile(save.getAttribute('data-name')); return; }
      const run = e.target.closest('.vault-file-run, .vault-file-name');
      if (run) { openFile(run.getAttribute('data-name')); }
    });

    render();

    return {
      async setTarget(newId) {
        id = newId || null;
        setExpanded(false);
        await refresh();
      },
      expand() { setExpanded(true); },
      collapse() { setExpanded(false); },
      refresh,
    };
  }

  window.Vault = { mount };
})();
