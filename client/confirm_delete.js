// client/confirm_delete.js
// 统一全站删除按钮的交互（表格批量删除 / 抽屉删除 / 卡片速审删除）：
//   - 第一次点击 / 按键：进入 armed（按钮文案变「确认删除」，样式变警示）
//   - 期限内再次点击：真正执行删除
//   - 超时 / Esc / 点击别处：自动撤销 armed
// 所有删除入口共用这一套行为，不再各自实现。
(function () {
  const TIMEOUT = 2400;
  const states = new WeakMap();
  const DEFAULT_ARMED = '<span class="del-armed-mark">?</span><span>确认删除</span>';

  function disarm(btn) {
    const st = states.get(btn);
    if (!st) return;
    clearTimeout(st.timer);
    btn.classList.remove('armed');
    btn.innerHTML = st.original;
    states.delete(btn);
  }

  function disarmAll() {
    document.querySelectorAll('.armed[data-armed-delete]').forEach((b) => disarm(b));
  }

  function arm(btn) {
    if (states.has(btn) || btn.disabled) return;
    const st = {
      original: btn.innerHTML,
      armed: btn.getAttribute('data-armed-html') || DEFAULT_ARMED,
      onConfirm: btn._armedConfirm || null,
      timer: null,
    };
    states.set(btn, st);
    btn.classList.add('armed');
    btn.innerHTML = st.armed;
    st.timer = setTimeout(() => disarm(btn), TIMEOUT);
  }

  function fire(btn) {
    const st = states.get(btn);
    if (!st) return;
    const cb = st.onConfirm;
    disarm(btn);
    if (typeof cb === 'function') cb();
  }

  // 注册一个删除按钮；返回的 trigger() 供键盘等外部入口复用同一 armed 状态。
  function register(btn, onConfirm) {
    if (!btn || btn.dataset.armedDelete === '1') return;
    btn.dataset.armedDelete = '1';
    btn._armedConfirm = onConfirm;
    btn.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (states.has(btn)) fire(btn);
      else arm(btn);
    });
  }

  function trigger(btn) {
    if (!btn || btn.disabled) return;
    if (states.has(btn)) fire(btn);
    else arm(btn);
  }

  // 点击别处 / Esc 撤销
  document.addEventListener('click', (e) => {
    if (e.target.closest && e.target.closest('.armed[data-armed-delete]')) return;
    disarmAll();
  }, true);
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') disarmAll();
  }, true);

  window.DeleteConfirm = { register, trigger, disarmAll, arm };
})();
