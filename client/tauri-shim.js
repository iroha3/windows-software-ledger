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

  const originalFetch = window.fetch.bind(window);

  // path -> (method, body) => invoke(name, args) | null
  function route(path, method, body) {
    switch (path) {
      case '/api/config':
        return method === 'POST' ? invoke('save_config', { payload: body }) : invoke('get_config');
      case '/api/status':
        return invoke('get_status');
      case '/api/software':
        return invoke('get_software');
      case '/api/dev-env':
        return invoke('get_dev_env');
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
})();
