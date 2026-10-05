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
      case '/api/sync/session-start':
        return invoke('sync_session_start');
      case '/api/sync/session-end':
        return invoke('sync_session_end');
      case '/api/sync/heartbeat':
        return invoke('sync_heartbeat');
      case '/api/sync/now':
        return invoke('sync_now', { mode: body.mode || 'auto' });
      case '/api/sync/unlock':
        return invoke('sync_unlock');
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

  // 只读镜像提示：另一台机器持锁时，本机进入只读（后端也会拒绝所有写命令）。
  // 主页由 app.js 自己管理横幅，这里只负责卡片速审 / 浏览器等子页面。
  function syncReadOnlyCheck() {
    if (document.getElementById('syncBanner')) return; // 主页已被 app.js 接管
    fetch('/api/sync/status')
      .then((r) => r.json())
      .then((st) => {
        if (st && st.enabled && st.we_hold === false) {
          document.body.classList.add('sync-readonly');
          if (!document.getElementById('syncReadonlyNotice')) {
            const d = document.createElement('div');
            d.id = 'syncReadonlyNotice';
            d.className = 'sync-banner';
            d.textContent = (st.host ? st.host + ' 正在编辑，' : '') + '本机只读，已禁用编辑';
            document.body.insertBefore(d, document.body.firstChild);
          }
        }
      })
      .catch(() => {});
  }
  // 关于弹窗的版本号：从后端读（唯一来源 src-tauri/Cargo.toml），避免各页面硬编码漂移。
  // 三个页面（主页 / 卡片速审 / 浏览器）都有 .about-version。
  function fillAppVersion() {
    const els = document.querySelectorAll('.about-version');
    if (!els.length) return;
    fetch('/api/version')
      .then((r) => r.json())
      .then((d) => {
        if (d && d.version) els.forEach((el) => { el.textContent = 'v' + d.version; });
      })
      .catch(() => {});
  }

  function bootPageState() {
    syncReadOnlyCheck();
    fillAppVersion();
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', bootPageState);
  } else {
    bootPageState();
  }
})();
