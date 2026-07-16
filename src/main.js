/**
 * 自动滚屏工具 - 前端主逻辑
 * 负责与 Rust 后端通信、更新 UI、录制全局快捷键、国际化。
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
  hotkeySpeedDownInput: document.getElementById('hotkey-speed-down-input'),
  setHotkeySpeedDownBtn: document.getElementById('set-hotkey-speed-down-btn'),
  hotkeySpeedUpInput: document.getElementById('hotkey-speed-up-input'),
  setHotkeySpeedUpBtn: document.getElementById('set-hotkey-speed-up-btn'),
  hotkeyModalTitle: document.getElementById('hotkey-modal-title'),
  compatCheckbox: document.getElementById('compat-checkbox'),
  toggleBtn: document.getElementById('toggle-btn'),
  hotkeyModal: document.getElementById('hotkey-modal'),
  hotkeyPreview: document.getElementById('hotkey-preview'),
  hotkeyCancel: document.getElementById('hotkey-cancel'),
  hotkeySave: document.getElementById('hotkey-save'),
  languageSelect: document.getElementById('language-select'),
};

// 当前应用状态缓存
let currentStatus = {
  scrolling: false,
  targetHwnd: null,
  targetTitle: '',
  speed: 50,
  direction: 'down',
  hotkey: 'Ctrl+Alt+S',
  speedDownHotkey: 'Ctrl+Alt+Left',
  speedUpHotkey: 'Ctrl+Alt+Right',
  compatibleMode: false,
  language: 'zh',
};

// 当前语言
let currentLang = 'zh';

// 方向显示文本映射（按语言）
const DIRECTION_LABELS = {
  zh: { up: '向上', down: '向下', left: '向左', right: '向右' },
  en: { up: 'Up', down: 'Down', left: 'Left', right: 'Right' },
};

/// 窗口标题最大显示长度，超出时中间用省略号截断。
const WINDOW_TITLE_MAX_LEN = 40;

/**
 * 国际化翻译表。
 */
const translations = {
  zh: {
    appTitle: '自动滚屏',
    appName: '🖱️ 自动滚屏',
    statusStopped: '已停止',
    statusScrolling: '滚屏中',
    targetWindow: '目标窗口',
    refresh: '刷新',
    refreshing: '刷新中…',
    selectWindow: '-- 选择一个窗口 --',
    currentTarget: '当前目标：',
    notSelected: '未选择',
    selectedWindow: '已选择窗口',
    clear: '清除',
    scrollSpeed: '滚动速度',
    slow: '慢',
    fast: '快',
    current: '当前：',
    scrollDirection: '滚动方向',
    dirUp: '向上',
    dirDown: '向下',
    dirLeft: '向左',
    dirRight: '向右',
    currentDirection: '当前方向：',
    globalHotkey: '全局快捷键',
    hotkeyToggle: '开启 / 关闭滚屏',
    hotkeySpeedDown: '速度减小一档',
    hotkeySpeedUp: '速度增大一档',
    modify: '修改',
    hotkeyTip: '按下快捷键即可随时调节滚屏',
    recordHotkeyFor: '录制快捷键：{{name}}',
    compatMode: '兼容模式（SendInput 模拟真实滚轮）',
    compatModeTip:
      '普通模式不抢夺焦点，适合浏览器、PDF 阅读器等现代程序；若窗口缩放/最小化后无法滚动，或某些旧程序不响应，可开启兼容模式。兼容模式通过 SendInput 模拟真实滚轮，需要把鼠标移到目标窗口上方（或让目标窗口处于激活状态）才生效。',
    startScroll: '开始滚屏',
    stopScroll: '停止滚屏',
    footerTip: '窗口关闭后自动停止 · 配置自动保存 · 支持系统托盘运行',
    recordHotkey: '录制快捷键',
    recordHotkeyTip: '请按下你想要的组合键（支持 Ctrl / Alt / Shift + 字母 / 数字 / F1-F24）',
    waitingKey: '等待按键…',
    cancel: '取消',
    save: '保存',
    mustIncludeModifier: '必须包含 Ctrl / Alt / Shift / Win 之一',
    error: {
      selectWindow: '选择窗口失败',
      clearTarget: '清除目标失败',
      setSpeed: '设置速度失败',
      setDirection: '设置方向失败',
      setCompatMode: '设置兼容模式失败',
      toggleScroll: '切换滚屏失败',
      setHotkey: '设置快捷键失败',
      getStatus: '获取状态失败',
      listWindows: '刷新窗口列表失败',
      setLanguage: '设置语言失败',
    },
  },
  en: {
    appTitle: 'Auto Scroll',
    appName: '🖱️ Auto Scroll',
    statusStopped: 'Stopped',
    statusScrolling: 'Scrolling',
    targetWindow: 'Target Window',
    refresh: 'Refresh',
    refreshing: 'Refreshing…',
    selectWindow: '-- Select a window --',
    currentTarget: 'Current target: ',
    notSelected: 'Not selected',
    selectedWindow: 'Selected window',
    clear: 'Clear',
    scrollSpeed: 'Scroll Speed',
    slow: 'Slow',
    fast: 'Fast',
    current: 'Current: ',
    scrollDirection: 'Scroll Direction',
    dirUp: 'Up',
    dirDown: 'Down',
    dirLeft: 'Left',
    dirRight: 'Right',
    currentDirection: 'Current direction: ',
    globalHotkey: 'Global Hotkeys',
    hotkeyToggle: 'Start / Stop Scrolling',
    hotkeySpeedDown: 'Decrease Speed',
    hotkeySpeedUp: 'Increase Speed',
    modify: 'Modify',
    hotkeyTip: 'Press the hotkeys to control auto-scroll anytime',
    recordHotkeyFor: 'Record Hotkey: {{name}}',
    compatMode: 'Compatibility Mode (SendInput simulates real wheel)',
    compatModeTip:
      'Normal mode does not steal focus and works well with browsers, PDF readers, etc. If scrolling stops after resizing/minimizing, or some legacy apps do not respond, enable compatibility mode. Compatibility mode uses SendInput to simulate a real wheel; you need to move the mouse over the target window (or make it active) for it to work.',
    startScroll: 'Start Scrolling',
    stopScroll: 'Stop Scrolling',
    footerTip: 'Auto-stop on window close · Auto-save settings · System tray support',
    recordHotkey: 'Record Hotkey',
    recordHotkeyTip: 'Press your desired combination (supports Ctrl / Alt / Shift + letter / number / F1-F24)',
    waitingKey: 'Waiting for key…',
    cancel: 'Cancel',
    save: 'Save',
    mustIncludeModifier: 'Must include Ctrl / Alt / Shift / Win',
    error: {
      selectWindow: 'Failed to select window',
      clearTarget: 'Failed to clear target',
      setSpeed: 'Failed to set speed',
      setDirection: 'Failed to set direction',
      setCompatMode: 'Failed to set compatibility mode',
      toggleScroll: 'Failed to toggle scrolling',
      setHotkey: 'Failed to set hotkey',
      getStatus: 'Failed to get status',
      listWindows: 'Failed to refresh window list',
      setLanguage: 'Failed to set language',
    },
  },
};

/**
 * 获取当前语言的翻译文本。
 */
function t(key, fallback = '', replacements = {}) {
  const lang = translations[currentLang] || translations.zh;
  const keys = key.split('.');
  let value = lang;
  for (const k of keys) {
    value = value?.[k];
  }
  if (typeof value !== 'string') return fallback;
  return value.replace(/\{\{(\w+)\}\}/g, (_, name) =>
    replacements[name] !== undefined ? replacements[name] : ''
  );
}

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

/// 当前正在录制的快捷键目标：toggle / speedDown / speedUp。
let currentHotkeyTarget = 'toggle';

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
  // 语言切换
  els.languageSelect.addEventListener('change', async (e) => {
    const lang = e.target.value;
    try {
      await invoke('set_language', { language: lang });
      currentLang = lang;
      applyLanguage();
      await refreshStatus();
    } catch (err) {
      showError(t('error.setLanguage'), err);
    }
  });

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
      showError(t('error.selectWindow'), err);
    }
  });

  // 清除目标窗口
  els.clearTargetBtn.addEventListener('click', async () => {
    try {
      await invoke('clear_target');
      await refreshStatus();
    } catch (err) {
      showError(t('error.clearTarget'), err);
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
      showError(t('error.setSpeed'), err);
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
        showError(t('error.setDirection'), err);
      }
    });
  });

  // 兼容模式开关
  els.compatCheckbox.addEventListener('change', async (e) => {
    try {
      await invoke('set_compatible_mode', { enabled: e.target.checked });
    } catch (err) {
      showError(t('error.setCompatMode'), err);
    }
  });

  // 主开关：开始 / 停止滚屏
  els.toggleBtn.addEventListener('click', async () => {
    try {
      await invoke('toggle_scroll');
      await refreshStatus();
    } catch (err) {
      showError(t('error.toggleScroll'), err);
    }
  });

  // 修改快捷键
  els.setHotkeyBtn.addEventListener('click', () => openHotkeyModal('toggle'));
  els.setHotkeySpeedDownBtn.addEventListener('click', () => openHotkeyModal('speedDown'));
  els.setHotkeySpeedUpBtn.addEventListener('click', () => openHotkeyModal('speedUp'));
  els.hotkeyCancel.addEventListener('click', closeHotkeyModal);
  els.hotkeySave.addEventListener('click', saveHotkey);

  // 快捷键录制事件
  document.addEventListener('keydown', onKeyDown);
}

/**
 * 应用当前语言到所有带有 data-i18n 属性的元素。
 */
function applyLanguage() {
  document.documentElement.lang = currentLang === 'zh' ? 'zh-CN' : 'en';
  document.title = t('appTitle');

  document.querySelectorAll('[data-i18n]').forEach((el) => {
    const key = el.dataset.i18n;
    const text = t(key);
    if (text) {
      // 如果元素包含子元素（如方向按钮里的箭头），只替换文本节点，保留子元素。
      if (el.children.length > 0 && el.getAttribute('data-i18n-keep-children') !== 'false') {
        // 找到第一个文本节点并替换
        for (const node of el.childNodes) {
          if (node.nodeType === Node.TEXT_NODE && node.textContent.trim()) {
            node.textContent = text;
            break;
          }
        }
      } else {
        el.textContent = text;
      }
    }
  });

  // 动态更新状态相关文本
  renderStatus();
}

/**
 * 从后端获取最新状态并刷新 UI
 */
async function refreshStatus() {
  try {
    const status = await invoke('get_status');
    // 将后端的 snake_case 字段映射到前端的 camelCase 状态。
    currentStatus = {
      scrolling: status.scrolling,
      targetHwnd: status.target_hwnd,
      targetTitle: status.target_title,
      speed: status.speed,
      direction: status.direction,
      hotkey: status.hotkey,
      speedDownHotkey: status.speed_down_hotkey,
      speedUpHotkey: status.speed_up_hotkey,
      compatibleMode: status.compatible_mode,
      language: status.language,
    };
    currentLang = status.language || 'zh';
    els.languageSelect.value = currentLang;
    applyLanguage();
  } catch (err) {
    showError(t('error.getStatus'), err);
  }
}

/**
 * 根据当前状态渲染界面
 */
function renderStatus() {
  // 状态徽章
  if (currentStatus.scrolling) {
    els.statusBadge.textContent = t('statusScrolling');
    els.statusBadge.className = 'status-badge scrolling';
    els.toggleBtn.textContent = t('stopScroll');
    els.toggleBtn.classList.add('danger');
  } else {
    els.statusBadge.textContent = t('statusStopped');
    els.statusBadge.className = 'status-badge stopped';
    els.toggleBtn.textContent = t('startScroll');
    els.toggleBtn.classList.remove('danger');
  }

  // 目标窗口
  if (currentStatus.targetHwnd) {
    els.targetTitle.textContent = currentStatus.targetTitle || t('selectedWindow');
    els.targetTitle.style.color = '#2c3e50';
  } else {
    els.targetTitle.textContent = t('notSelected');
    els.targetTitle.style.color = '#999';
  }

  // 速度
  els.speedSlider.value = currentStatus.speed;
  els.speedValue.textContent = currentStatus.speed;

  // 方向
  const dir = currentStatus.direction || 'down';
  els.directionValue.textContent = (DIRECTION_LABELS[currentLang] || DIRECTION_LABELS.zh)[dir];
  els.directionButtons.forEach((btn) => {
    if (btn.dataset.direction === dir) {
      btn.classList.add('active');
    } else {
      btn.classList.remove('active');
    }
  });

  // 快捷键
  els.hotkeyInput.value = currentStatus.hotkey || 'Ctrl+Alt+S';
  els.hotkeySpeedDownInput.value = currentStatus.speedDownHotkey || 'Ctrl+Alt+Left';
  els.hotkeySpeedUpInput.value = currentStatus.speedUpHotkey || 'Ctrl+Alt+Right';

  // 兼容模式
  els.compatCheckbox.checked = currentStatus.compatibleMode;
}

/**
 * 刷新窗口下拉列表
 */
async function refreshWindows() {
  try {
    els.windowSelect.disabled = true;
    const originalText = t('refresh');
    els.refreshBtn.textContent = t('refreshing');

    const windows = await invoke('list_windows');
    els.windowSelect.innerHTML = `<option value="">${t('selectWindow')}</option>`;

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
    showError(t('error.listWindows'), err);
  } finally {
    els.windowSelect.disabled = false;
    els.refreshBtn.textContent = t('refresh');
  }
}

/**>
 * 打开快捷键录制弹窗。
 *
 * @param {'toggle' | 'speedDown' | 'speedUp'} target 正在录制哪一项快捷键。
 */
function openHotkeyModal(target) {
  currentHotkeyTarget = target;
  recordingHotkey = { modifiers: new Set(), key: '' };

  // 根据目标更新弹窗标题。
  const nameKey =
    target === 'toggle'
      ? 'hotkeyToggle'
      : target === 'speedDown'
      ? 'hotkeySpeedDown'
      : 'hotkeySpeedUp';
  const title = t('recordHotkeyFor', '', { name: t(nameKey) });
  if (els.hotkeyModalTitle) {
    els.hotkeyModalTitle.textContent = title;
  }

  els.hotkeyPreview.textContent = t('waitingKey');
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
      els.hotkeyPreview.textContent = t('mustIncludeModifier');
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
        showError(t('error.setSpeed'), err)
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
    if (currentHotkeyTarget === 'toggle') {
      await invoke('set_hotkey', { hotkeyStr: recordingHotkey.combo });
    } else {
      const down =
        currentHotkeyTarget === 'speedDown'
          ? recordingHotkey.combo
          : currentStatus.speedDownHotkey;
      const up =
        currentHotkeyTarget === 'speedUp'
          ? recordingHotkey.combo
          : currentStatus.speedUpHotkey;
      await invoke('set_speed_hotkeys', {
        speedDownHotkey: down,
        speedUpHotkey: up,
      });
    }
    closeHotkeyModal();
    await refreshStatus();
  } catch (err) {
    showError(t('error.setHotkey'), err);
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
