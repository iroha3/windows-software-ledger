// client/welcome.js
// 首次运行的新手引导覆盖层：纯本地展示，可直接点「扫描本机」开始。
// 只在「没看过 + 台账为空」时自动弹出；之后可从「关于」里重新打开。
(function () {
  const overlay = document.getElementById('welcomeOverlay');
  if (!overlay) return;

  const dontShow = document.getElementById('welcomeDontShow');
  let lastFocus = null;

  function persistSeen() {
    if (dontShow && dontShow.checked) {
      try { localStorage.setItem('welcomeSeen', '1'); } catch (e) { /* 隐私模式等忽略 */ }
    }
  }

  function close() {
    persistSeen();
    overlay.classList.remove('show');
    if (lastFocus && typeof lastFocus.focus === 'function') lastFocus.focus();
  }

  function open() {
    lastFocus = document.activeElement;
    overlay.classList.add('show');
    const scan = document.getElementById('welcomeScan');
    if (scan) scan.focus();
  }

  const bind = (id, fn) => {
    const el = document.getElementById(id);
    if (el) el.addEventListener('click', fn);
  };

  bind('welcomeClose', close);
  bind('welcomeLater', close);
  overlay.addEventListener('click', (e) => { if (e.target === overlay) close(); });

  // 主操作：开始扫描
  bind('welcomeScan', () => {
    persistSeen();
    overlay.classList.remove('show');
    if (typeof window.handleScanLocal === 'function') window.handleScanLocal();
  });
  bind('welcomeOpenSettings', () => {
    close();
    if (typeof window.openConfigModal === 'function') window.openConfigModal();
  });
  // 「关于」里的重新打开入口
  bind('btnShowWelcome', () => {
    const about = document.getElementById('aboutModal');
    if (about) about.classList.remove('show');
    open();
  });

  window.Welcome = {
    // hasData = 台账是否已有数据；返回是否弹出了引导。
    maybeShow(hasData) {
      let seen = false;
      try { seen = localStorage.getItem('welcomeSeen') === '1'; } catch (e) { /* 忽略 */ }
      if (!seen && !hasData) {
        open();
        return true;
      }
      return false;
    },
    open,
    close,
  };
})();
