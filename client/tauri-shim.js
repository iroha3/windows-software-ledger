// client/tauri-shim.js
// 在 Tauri 原生外壳中，把前端的 fetch('/api/...') 透明转发到 Rust #[tauri::command]。
// 在浏览器 / Bun 服务下运行时，window.__TAURI__ 不存在，本文件不产生任何影响。
(function () {
  const tauri = window.__TAURI__;
  const invoke = tauri && tauri.core && tauri.core.invoke;

  // 打开外部链接：Tauri 下走原生 open_url 命令（交给系统默认浏览器），
  // 浏览器 / Bun 下退回 window.open。始终挂载，两种环境都可用。
  window.openExternal = function (url) {
    if (!url) return;
    const full = /^https?:\/\//i.test(url) ? url : 'https://' + url;
    if (typeof invoke === 'function') {
      invoke('open_url', { url: full }).catch((err) => {
        console.error('open_url failed:', err);
        window.open(full, '_blank', 'noopener,noreferrer');
      });
    } else {
      window.open(full, '_blank', 'noopener,noreferrer');
    }
  };

  if (typeof invoke !== 'function') return;

  // 原生文件对话框（tauri-plugin-dialog，通过 plugin 命令直接调用）
  window.dialogOpen = function (options) {
    return invoke('plugin:dialog|open', { options: options || {} }).catch((err) => {
      console.error('dialog open failed:', err);
      return null;
    });
  };
  window.dialogSave = function (options) {
    return invoke('plugin:dialog|save', { options: options || {} }).catch((err) => {
      console.error('dialog save failed:', err);
      return null;
    });
  };

  // 原生拖入文件：返回绝对路径数组（WebView2 不支持拖出，只能拖入）
  window.onFileDrop = function (cb) {
    const ev = window.__TAURI__ && window.__TAURI__.event;
    if (!ev || typeof ev.listen !== 'function') return function () {};
    let unlisten = null;
    ev.listen('tauri://drag-drop', (e) => {
      const paths = (e && e.payload && e.payload.paths) || [];
      if (paths.length) cb(paths);
    }).then((u) => { unlisten = u; });
    return function () { if (unlisten) unlisten(); };
  };

  const originalFetch = window.fetch.bind(window);

  // path -> (method, body) => invoke(name, args) | null
  function route(path, method, body) {
    switch (path) {
      case '/api/config':
        return method === 'POST' ? invoke('save_config', { payload: body }) : invoke('get_config');
      case '/api/status':
        return invoke('get_status');
      case '/api/version':
        return invoke('get_version');
      case '/api/update/check':
        return invoke('check_update');
      case '/api/mcp':
        return invoke('get_mcp_info');
      case '/api/software':
        return invoke('get_software');
      case '/api/ledger-revision':
        return invoke('get_ledger_revision');
      case '/api/dev-env':
        return invoke('get_dev_env');
      case '/api/browser-extensions':
        return invoke('get_browser_extensions');
      case '/api/extension/update':
        return invoke('update_extension', { payload: body });
      case '/api/vault/list':
        return invoke('vault_list', { kind: body.kind, id: body.id });
      case '/api/vault/add':
        return invoke('vault_add', { kind: body.kind, id: body.id, paths: body.paths });
      case '/api/vault/delete':
        return invoke('vault_delete', { kind: body.kind, id: body.id, name: body.name });
      case '/api/vault/export':
        return invoke('vault_export', { kind: body.kind, id: body.id, name: body.name, dest: body.dest });
      case '/api/software/update':
        return invoke('update_software', { payload: body });
      case '/api/software/batch-update':
        return invoke('batch_update', { payload: body });
      case '/api/software/delete':
        return invoke('delete_software', { payload: body });
      case '/api/software/batch-add':
        return invoke('batch_add', { payload: body });
      case '/api/software/merge':
        return invoke('merge_software', { payload: body });
      case '/api/llm/analyze':
        return invoke('llm_analyze', { payload: body });
      case '/api/scan/preview':
        return invoke('scan_preview');
      case '/api/scan/commit':
        return invoke('scan_commit', { payload: body });
      case '/api/export':
        return invoke('export_markdown');
      case '/api/export/xlsx':
        return invoke('export_xlsx', { dest: body.dest });
      case '/api/export/save':
        return invoke('export_save', { which: body.which, dest: body.dest });
      case '/api/webdav/config':
        return method === 'POST' ? invoke('save_webdav_config', { payload: body }) : invoke('get_webdav_config');
      case '/api/webdav/test':
        return invoke('webdav_test');
      case '/api/sync/now':
        return invoke('sync_now', { mode: body.mode || 'auto' });
      case '/api/sync/status':
        return invoke('sync_status');
      default:
        return null;
    }
  }

  window.fetch = function (input, init) {
    const url = typeof input === 'string' ? input : (input && input.url) || '';
    const path = url.split('?')[0];
    const method = String((init && init.method) || (input && input.method) || 'GET').toUpperCase();

    let body = {};
    if (init && init.body) {
      try {
        body = typeof init.body === 'string' ? JSON.parse(init.body) : init.body;
      } catch (e) {
        body = {};
      }
    }

    const promise = route(path, method, body);
    if (!promise || typeof promise.then !== 'function') {
      return originalFetch(input, init);
    }

    return promise
      .then((data) => ({
        ok: true,
        status: 200,
        json: () => Promise.resolve(data),
        text: () => Promise.resolve(JSON.stringify(data))
      }))
      .catch((err) => ({
        ok: false,
        status: 500,
        json: () => Promise.resolve({ success: false, error: String(err) }),
        text: () => Promise.resolve(String(err))
      }));
  };

  // 关于弹窗的版本号：从后端读（唯一来源 src-tauri/Cargo.toml），避免各页面硬编码漂移。
  // 三个页面（主页 / 卡片速审 / 浏览器）都有 .about-version。
  // 同时在版本号下挂一个「检查更新」入口，复用 GitHub Releases。
  function mountUpdateChecker(versionEl) {
    if (versionEl.dataset.updateReady === '1') return;
    versionEl.dataset.updateReady = '1';
    const anchor = versionEl.closest('.about-hero') || versionEl.parentElement;
    if (!anchor) return;

    const box = document.createElement('div');
    box.className = 'about-update';
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'btn btn-secondary btn-sm';
    btn.innerHTML = '<svg class="i sm" viewBox="0 0 24 24"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg><span>检查更新</span>';
    const status = document.createElement('span');
    status.className = 'about-update-status';
    box.appendChild(btn);
    box.appendChild(status);
    versionEl.insertAdjacentElement('afterend', box);

    btn.addEventListener('click', async () => {
      // 按钮文案/尺寸保持不变，避免点击后位置或弹窗高度跳动；状态一律写在下方固定占位里
      btn.disabled = true;
      status.textContent = '检查中...';
      try {
        const res = await fetch('/api/update/check');
        const d = await res.json();
        if (!d || !d.success) {
          status.textContent = '检查失败' + (d && d.error ? '：' + d.error : '');
        } else if (d.has_update) {
          status.textContent = `发现新版本 v${d.latest} · `;
          const link = document.createElement('a');
          link.className = 'about-update-link';
          link.href = '#';
          link.textContent = '前往下载';
          link.addEventListener('click', (e) => {
            e.preventDefault();
            window.openExternal(d.download_url || d.url);
          });
          status.appendChild(link);
          if (typeof window.showToast === 'function') {
            window.showToast(`发现新版本 v${d.latest}，可前往下载`, 'success');
          }
        } else {
          status.textContent = `已是最新版本 v${d.current}`;
        }
      } catch (e) {
        status.textContent = '检查失败：' + e.message;
      } finally {
        btn.disabled = false;
      }
    });
  }

  function fillAppVersion() {
    const els = document.querySelectorAll('.about-version');
    if (!els.length) return;
    els.forEach((el) => mountUpdateChecker(el));
    fetch('/api/version')
      .then((r) => r.json())
      .then((d) => {
        if (d && d.version) els.forEach((el) => { el.textContent = 'v' + d.version; });
      })
      .catch(() => {});
  }

  function bootPageState() {
    fillAppVersion();
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', bootPageState);
  } else {
    bootPageState();
  }
})();
