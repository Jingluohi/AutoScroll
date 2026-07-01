/**
 * 自动滚屏工具 - 前端主逻辑
 * 负责与 Rust 后端通信、更新 UI、录制全局快捷键
 */

// Tauri 2 在前端暴露的核心 API
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// DOM 元素引用
const els = {
  statusBadge: document.getElementById('status-badge'),
  windowSelect: document.getElementById('window-select'),
  refreshBtn: document.getElementById('refresh-btn'),
  targetTitle: document.getElementById('target-title'),
  clearTargetBtn: document.getElementById('clear-target-btn'),
  speedSlider: document.getElementById('speed-slider'),
  speedValue: document.getElementById('speed-value'),
  directionValue: document.getElementById('direction-value'),
  directionButtons: document.querySelectorAll('.btn-direction'),
  hotkeyInput: document.getElementById('hotkey-input'),
  setHotkeyBtn: document.getElementById('set-hotkey-btn'),
  compatCheckbox: document.getElementById('compat-checkbox'),
  toggleBtn: document.getElementById('toggle-btn'),
  hotkeyModal: document.getElementById('hotkey-modal'),
  hotkeyPreview: document.getElementById('hotkey-preview'),
  hotkeyCancel: document.getElementById('hotkey-cancel'),
  hotkeySave: document.getElementById('hotkey-save'),
};

// 当前应用状态缓存
let currentStatus = {
  scrolling: false,
  targetHwnd: null,
  targetTitle: '',
  speed: 50,
  direction: 'down',
  hotkey: 'Ctrl+Alt+S',
  compatibleMode: false,
};

// 方向显示文本映射
const DIRECTION_LABELS = {
  up: '向上',
  down: '向下',
  left: '向左',
  right: '向右',
};

/// 窗口标题最大显示长度，超出时中间用省略号截断。
const WINDOW_TITLE_MAX_LEN = 40;

/**
 * 过长字符串中间省略号截断，保留头尾便于识别窗口。
 */
function truncateMiddle(text, maxLen) {
  if (text.length <= maxLen) return text;
  const side = Math.floor((maxLen - 1) / 2);
  return text.slice(0, side) + '…' + text.slice(text.length - side);
}

// 快捷键录制状态
let recordingHotkey = null;

/**
 * 初始化：绑定事件、加载状态、监听后端状态变化
 */
async function init() {
  bindEvents();
  await refreshStatus();
  await refreshWindows();

  // 监听 Rust 后端的状态变化事件
  listen('state-changed', () => {
    refreshStatus();
  });
}

/**
 * 绑定前端事件
 */
function bindEvents() {
  // 刷新窗口列表
  els.refreshBtn.addEventListener('click', refreshWindows);

  // 选择目标窗口
  els.windowSelect.addEventListener('change', async (e) => {
    const hwnd = e.target.value;
    if (!hwnd) return;
    try {
      await invoke('set_target', { hwnd: Number(hwnd) });
      await refreshStatus();
    } catch (err) {
      showError('选择窗口失败', err);
    }
  });

  // 清除目标窗口
  els.clearTargetBtn.addEventListener('click', async () => {
    try {
      await invoke('clear_target');
      await refreshStatus();
    } catch (err) {
      showError('清除目标失败', err);
    }
  });

  // 调节滚动速度
  els.speedSlider.addEventListener('input', (e) => {
    els.speedValue.textContent = e.target.value;
  });
  els.speedSlider.addEventListener('change', async (e) => {
    try {
      await invoke('set_speed', { speed: Number(e.target.value) });
    } catch (err) {
      showError('设置速度失败', err);
    }
  });

  // 滚动方向按钮
  els.directionButtons.forEach((btn) => {
    btn.addEventListener('click', async () => {
      const direction = btn.dataset.direction;
      try {
        await invoke('set_direction', { direction });
        await refreshStatus();
      } catch (err) {
        showError('设置方向失败', err);
      }
    });
  });

  // 兼容模式开关
  els.compatCheckbox.addEventListener('change', async (e) => {
    try {
      await invoke('set_compatible_mode', { enabled: e.target.checked });
    } catch (err) {
      showError('设置兼容模式失败', err);
    }
  });

  // 主开关：开始 / 停止滚屏
  els.toggleBtn.addEventListener('click', async () => {
    try {
      await invoke('toggle_scroll');
      await refreshStatus();
    } catch (err) {
      showError('切换滚屏失败', err);
    }
  });

  // 修改快捷键
  els.setHotkeyBtn.addEventListener('click', openHotkeyModal);
  els.hotkeyCancel.addEventListener('click', closeHotkeyModal);
  els.hotkeySave.addEventListener('click', saveHotkey);

  // 快捷键录制事件
  document.addEventListener('keydown', onKeyDown);
}

/**
 * 从后端获取最新状态并刷新 UI
 */
async function refreshStatus() {
  try {
    const status = await invoke('get_status');
    currentStatus = status;
    renderStatus();
  } catch (err) {
    showError('获取状态失败', err);
  }
}

/**
 * 根据当前状态渲染界面
 */
function renderStatus() {
  // 状态徽章
  if (currentStatus.scrolling) {
    els.statusBadge.textContent = '滚屏中';
    els.statusBadge.className = 'status-badge scrolling';
    els.toggleBtn.textContent = '停止滚屏';
    els.toggleBtn.classList.add('danger');
  } else {
    els.statusBadge.textContent = '已停止';
    els.statusBadge.className = 'status-badge stopped';
    els.toggleBtn.textContent = '开始滚屏';
    els.toggleBtn.classList.remove('danger');
  }

  // 目标窗口
  if (currentStatus.targetHwnd) {
    els.targetTitle.textContent = currentStatus.targetTitle || '已选择窗口';
    els.targetTitle.style.color = '#2c3e50';
  } else {
    els.targetTitle.textContent = '未选择';
    els.targetTitle.style.color = '#999';
  }

  // 速度
  els.speedSlider.value = currentStatus.speed;
  els.speedValue.textContent = currentStatus.speed;

  // 方向
  const dir = currentStatus.direction || 'down';
  els.directionValue.textContent = DIRECTION_LABELS[dir] || '向下';
  els.directionButtons.forEach((btn) => {
    if (btn.dataset.direction === dir) {
      btn.classList.add('active');
    } else {
      btn.classList.remove('active');
    }
  });

  // 快捷键
  els.hotkeyInput.value = currentStatus.hotkey;

  // 兼容模式
  els.compatCheckbox.checked = currentStatus.compatibleMode;
}

/**
 * 刷新窗口下拉列表
 */
async function refreshWindows() {
  try {
    els.windowSelect.disabled = true;
    els.refreshBtn.textContent = '刷新中…';

    const windows = await invoke('list_windows');
    els.windowSelect.innerHTML = '<option value="">-- 选择一个窗口 --</option>';

    windows.forEach((win) => {
      const option = document.createElement('option');
      option.value = String(win.hwnd);
      option.textContent = truncateMiddle(win.title, WINDOW_TITLE_MAX_LEN);
      option.title = win.title; // 鼠标悬停显示完整标题
      // 如果与当前目标窗口匹配，设为选中
      if (currentStatus.targetHwnd && String(win.hwnd) === String(currentStatus.targetHwnd)) {
        option.selected = true;
      }
      els.windowSelect.appendChild(option);
    });
  } catch (err) {
    showError('刷新窗口列表失败', err);
  } finally {
    els.windowSelect.disabled = false;
    els.refreshBtn.textContent = '刷新';
  }
}

/**
 * 打开快捷键录制弹窗
 */
function openHotkeyModal() {
  recordingHotkey = { modifiers: new Set(), key: '' };
  els.hotkeyPreview.textContent = '等待按键…';
  els.hotkeySave.disabled = true;
  els.hotkeyModal.classList.remove('hidden');
}

/**
 * 关闭快捷键录制弹窗
 */
function closeHotkeyModal() {
  recordingHotkey = null;
  els.hotkeyModal.classList.add('hidden');
}

/**
 * 处理键盘按下事件：
 * - 录制快捷键时，捕获组合键；
 * - 未录制时，左右方向键微调滚动速度。
 */
function onKeyDown(e) {
  if (recordingHotkey) {
    e.preventDefault();
    e.stopPropagation();

    // 忽略单独按下修饰键的情况
    if (['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) return;

    const modifiers = [];
    if (e.ctrlKey) modifiers.push('Ctrl');
    if (e.altKey) modifiers.push('Alt');
    if (e.shiftKey) modifiers.push('Shift');
    if (e.metaKey) modifiers.push('Win');

    // 快捷键必须包含至少一个修饰键，避免与普通输入冲突
    if (modifiers.length === 0) {
      els.hotkeyPreview.textContent = '必须包含 Ctrl / Alt / Shift / Win 之一';
      els.hotkeySave.disabled = true;
      return;
    }

    // 格式化主键
    let key = e.key;
    if (key.length === 1) {
      key = key.toUpperCase();
    } else if (key.startsWith('F') && /^F\d+$/.test(key)) {
      key = key.toUpperCase();
    }

    const combo = [...modifiers, key].join('+');
    recordingHotkey.combo = combo;
    els.hotkeyPreview.textContent = combo;
    els.hotkeySave.disabled = false;
    return;
  }

  // 左右方向键调节速度（窗口聚焦时生效）。
  if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
    e.preventDefault();
    const delta = e.key === 'ArrowLeft' ? -1 : 1;
    const current = Number(els.speedSlider.value);
    const newSpeed = Math.max(1, Math.min(100, current + delta));
    if (newSpeed !== current) {
      els.speedSlider.value = newSpeed;
      els.speedValue.textContent = newSpeed;
      invoke('set_speed', { speed: newSpeed }).catch((err) =>
        showError('设置速度失败', err)
      );
    }
  }
}

/**
 * 保存录制的快捷键到后端
 */
async function saveHotkey() {
  if (!recordingHotkey || !recordingHotkey.combo) return;
  try {
    await invoke('set_hotkey', { hotkeyStr: recordingHotkey.combo });
    closeHotkeyModal();
    await refreshStatus();
  } catch (err) {
    showError('设置快捷键失败', err);
  }
}

/**
 * 简单错误提示
 */
function showError(title, err) {
  const message = typeof err === 'string' ? err : err?.message || String(err);
  console.error(title, message);
  alert(`${title}：${message}`);
}

// 启动应用
init();
