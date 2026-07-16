//! 应用状态模块
//!
//! 定义运行时的共享状态（配置、是否滚屏、工作线程句柄），
//! 以及返回给前端的状态结构。

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use parking_lot::Mutex as PLMutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::config::{write_config, AppConfig, Language, ScrollDirection};
use crate::window::get_window_title;
use windows::Win32::Foundation::HWND;

/// 将滚动方向编码为 u8，便于工作线程原子读写。
impl ScrollDirection {
    pub fn to_u8(self) -> u8 {
        match self {
            ScrollDirection::Up => 0,
            ScrollDirection::Down => 1,
            ScrollDirection::Left => 2,
            ScrollDirection::Right => 3,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Up,
            2 => Self::Left,
            3 => Self::Right,
            _ => Self::Down,
        }
    }
}

/// 当前运行状态，返回给前端同步 UI。
#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    /// 是否正在自动滚屏。
    pub scrolling: bool,
    /// 目标窗口句柄。
    pub target_hwnd: Option<isize>,
    /// 目标窗口标题。
    pub target_title: String,
    /// 当前速度。
    pub speed: i32,
    /// 当前滚动方向。
    pub direction: ScrollDirection,
    /// 当前热键。
    pub hotkey: String,
    /// 减小速度的热键。
    pub speed_down_hotkey: String,
    /// 增大速度的热键。
    pub speed_up_hotkey: String,
    /// 是否兼容模式。
    pub compatible_mode: bool,
    /// 当前界面语言。
    pub language: Language,
}

/// 滚动工作线程句柄，用于停止当前滚动。
pub struct ScrollWorker {
    pub stop_tx: PLMutex<Option<Sender<()>>>,
    /// 当前滚动方向，工作线程每帧读取以支持热切换。
    pub direction: Arc<AtomicU8>,
    /// 当前滚动速度，工作线程每帧读取以支持热切换。
    pub speed: Arc<AtomicI32>,
    /// 当前兼容模式，工作线程每帧读取以支持热切换。
    pub compatible_mode: Arc<AtomicBool>,
}

impl Default for ScrollWorker {
    fn default() -> Self {
        Self {
            stop_tx: PLMutex::new(None),
            direction: Arc::new(AtomicU8::new(ScrollDirection::Down.to_u8())),
            speed: Arc::new(AtomicI32::new(50)),
            compatible_mode: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// 全局应用状态（必须实现 Send + Sync，才能被 Tauri State 管理）。
///
/// 注意：GlobalHotKeyManager 和 HWND 都不是 Send，因此不能放在本结构体中。
pub struct AppState {
    pub config: PLMutex<AppConfig>,
    /// 是否正在滚屏，使用 Arc<AtomicBool> 以便工作线程在目标窗口关闭时也能同步状态。
    pub scrolling: Arc<AtomicBool>,
    pub worker: ScrollWorker,
}

impl AppState {
    /// 使用已加载的配置创建状态。
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: PLMutex::new(config),
            scrolling: Arc::new(AtomicBool::new(false)),
            worker: ScrollWorker::default(),
        }
    }

    /// 保存当前配置到磁盘。
    pub fn save_config(&self, app_handle: &AppHandle) -> Result<(), String> {
        let cfg = self.config.lock().clone();
        write_config(app_handle, &cfg)
    }
}

/// 获取当前应用状态，供前端渲染。
#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> AppStatus {
    let cfg = state.config.lock();
    let target_title = cfg
        .target_hwnd
        .map(|h| get_window_title(HWND(h as *mut std::ffi::c_void)))
        .unwrap_or_default();

    AppStatus {
        scrolling: state.scrolling.load(Ordering::Relaxed),
        target_hwnd: cfg.target_hwnd,
        target_title,
        speed: cfg.speed,
        direction: cfg.direction,
        hotkey: cfg.hotkey.clone(),
        speed_down_hotkey: cfg.speed_down_hotkey.clone(),
        speed_up_hotkey: cfg.speed_up_hotkey.clone(),
        compatible_mode: cfg.compatible_mode,
        language: cfg.language,
    }
}

/// 设置界面语言。
///
/// 修改后会保存到配置文件，并通知前端刷新 UI。
#[tauri::command]
pub fn set_language(
    language: Language,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    state.config.lock().language = language;
    state.save_config(&app_handle)?;
    let _ = app_handle.emit("state-changed", ());
    Ok(())
}
