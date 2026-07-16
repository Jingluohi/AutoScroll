//! 自动滚屏控制模块
//!
//! 负责启动/停止滚动工作线程、向目标窗口发送滚轮事件，
//! 以及处理前端的速度、方向、兼容模式和开关命令。
//!
//! 滚动逻辑采用"显示器刷新率同步"：
//! - 查询目标窗口所在显示器的刷新率（60Hz~300Hz）
//! - 用 `timeBeginPeriod(1)` 将 Windows 定时器精度提升到 1ms，
//!   并通过自旋等待精确对齐每一帧的显示刷新边界
//! - 每帧只发送一次滚轮事件，避免一帧内多次发送被目标程序合并导致的顿挫
//! - 普通模式使用子 WHEEL_DELTA 发送，让现代应用（浏览器、PDF 阅读器等）实现像素级平滑滚动
//! - 兼容模式使用完整 WHEEL_DELTA 整数倍发送，适配只识别标准滚轮刻度的旧程序
//! - 方向、速度、兼容模式均支持热切换，工作线程实时读取最新值
//!
//! 这种设计与显示器刷新同步，在 60Hz 屏幕上每 16.67ms 均匀输出一次滚动量，
//! 从根本上消除 "一帧多、一帧少" 的顿挫感。

use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, State};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplaySettingsW, GetMonitorInfoW, MonitorFromWindow, ENUM_CURRENT_SETTINGS,
    MONITOR_DEFAULTTONEAREST, MONITORINFOEXW,
};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, IsIconic, IsWindow, PostMessageW, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WHEEL_DELTA,
};

use crate::config::{AppConfig, ScrollDirection};
use crate::state::AppState;

/// 旧版固定 5ms 步长对应的参考频率（200 Hz）。
const REFERENCE_HZ: f64 = 200.0;

/// 第 1 档 = 原 100 档系统第 1 档的 90%。
/// 原 100 档系统第 1 档为 0.6（每 5ms），换算成每秒滚动量。
const LEVEL_1_UNITS_PER_SEC: f64 = 0.6 * 1.2 * 0.9 * REFERENCE_HZ;

/// 第 100 档 = 原 100 档系统第 50 档。
/// 原 100 档系统 step = 0.6 + (speed - 1) * (600 - 0.6) / 99。
const LEVEL_100_UNITS_PER_SEC: f64 = {
    let old_level_50 = 0.6 + 49.0 * (600.0 - 0.6) / 99.0;
    old_level_50 * REFERENCE_HZ
};

/// 默认刷新率（Hz），当无法获取显示器刷新率时使用。
const DEFAULT_REFRESH_HZ: u32 = 60;

/// 最小刷新率（Hz），避免低刷新率屏幕循环周期过长。
const MIN_REFRESH_HZ: u32 = 60;

/// 最大刷新率（Hz），避免高刷新率屏幕占用过多 CPU。
const MAX_REFRESH_HZ: u32 = 300;

/// 浮点形式的 WHEEL_DELTA，便于子 delta 计算。
const WHEEL_DELTA_F: f64 = WHEEL_DELTA as f64;

/// 计算给定档位的每秒滚动量（单位：滚轮增量单位）。
///
/// 第 1 档为 `LEVEL_1_UNITS_PER_SEC`，第 100 档为 `LEVEL_100_UNITS_PER_SEC`，
/// 中间严格线性均匀递增。
fn units_per_second_for_speed(speed: i32) -> f64 {
    let speed = speed.clamp(1, 100) as f64;
    LEVEL_1_UNITS_PER_SEC
        + (speed - 1.0) * (LEVEL_100_UNITS_PER_SEC - LEVEL_1_UNITS_PER_SEC) / 99.0
}

/// 设置滚动速度。
///
/// 如果当前正在滚屏，会同步更新工作线程中的速度原子，实现热切换。
#[tauri::command]
pub fn set_speed(
    speed: i32,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let speed = speed.clamp(1, 100);
    state.config.lock().speed = speed;
    state.worker.speed.store(speed, Ordering::Relaxed);
    state.save_config(&app_handle)?;
    let _ = app_handle.emit("state-changed", ());
    Ok(())
}

/// 内部调节滚动速度，供全局热键调用。
///
/// `delta` 为正表示加速，为负表示减速，结果会被限制在 1~100 档范围内。
pub fn adjust_speed_internal(
    delta: i32,
    state: &AppState,
    app_handle: &AppHandle,
) -> Result<(), String> {
    let new_speed = {
        let mut cfg = state.config.lock();
        let current = cfg.speed;
        let new = (current + delta).clamp(1, 100);
        cfg.speed = new;
        new
    };
    state.worker.speed.store(new_speed, Ordering::Relaxed);
    state.save_config(app_handle)?;
    let _ = app_handle.emit("state-changed", ());
    Ok(())
}

/// 设置滚动方向。
///
/// 如果当前正在滚屏，会同步更新工作线程中的方向原子，实现热切换。
#[tauri::command]
pub fn set_direction(
    direction: ScrollDirection,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    state.config.lock().direction = direction;
    state
        .worker
        .direction
        .store(direction.to_u8(), Ordering::Relaxed);
    state.save_config(&app_handle)?;
    let _ = app_handle.emit("state-changed", ());
    Ok(())
}

/// 设置兼容模式。
///
/// 普通模式使用 `PostMessage` + 子 WHEEL_DELTA，不抢夺焦点，适合浏览器等现代应用。
/// 兼容模式使用 `SendInput` + 完整 WHEEL_DELTA 整数倍，需要目标窗口为当前激活窗口
/// 或鼠标位于其上方，但能被更多旧程序识别。
#[tauri::command]
pub fn set_compatible_mode(
    enabled: bool,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    state.config.lock().compatible_mode = enabled;
    state.worker.compatible_mode.store(enabled, Ordering::Relaxed);
    state.save_config(&app_handle)?;
    let _ = app_handle.emit("state-changed", ());
    Ok(())
}

/// 切换自动滚屏状态（前端按钮调用）。
#[tauri::command]
pub fn toggle_scroll(
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<bool, String> {
    toggle_scroll_internal(&*state, &app_handle)
}

/// 内部切换滚动状态，供热键监听线程与托盘菜单调用。
pub fn toggle_scroll_internal(
    state: &AppState,
    app_handle: &AppHandle,
) -> Result<bool, String> {
    let should_scroll = !state.scrolling.load(Ordering::Relaxed);

    if should_scroll {
        // 检查是否有目标窗口。
        let cfg = state.config.lock().clone();
        if cfg.target_hwnd.is_none() {
            return Err("请先选择要滚屏的目标窗口".to_string());
        }
        let hwnd = HWND(cfg.target_hwnd.unwrap() as *mut std::ffi::c_void);
        unsafe {
            if !IsWindow(hwnd).as_bool() {
                return Err("目标窗口已关闭或无效，请重新选择".to_string());
            }
        }

        // 启动滚动线程，同时把 scrolling 标志共享给线程，
        // 这样目标窗口关闭时线程能自动把状态置回 false。
        let scrolling_arc = Arc::clone(&state.scrolling);
        start_scroll_worker(state, cfg, app_handle.clone(), scrolling_arc);
        state.scrolling.store(true, Ordering::Relaxed);
    } else {
        stop_scroll_worker(state);
        state.scrolling.store(false, Ordering::Relaxed);
    }

    let _ = app_handle.emit("state-changed", ());
    Ok(should_scroll)
}

/// 启动滚动工作线程。
///
/// 线程内只保存句柄数值和配置，HWND 本身不能跨线程传递。
/// 方向、速度、兼容模式均通过 Arc<Atomic> 共享，以支持热切换。
fn start_scroll_worker(
    state: &AppState,
    cfg: AppConfig,
    app_handle: AppHandle,
    scrolling_arc: Arc<std::sync::atomic::AtomicBool>,
) {
    let (tx, rx) = mpsc::channel::<()>();
    *state.worker.stop_tx.lock() = Some(tx);

    let target_hwnd = cfg.target_hwnd.unwrap();

    // 初始化共享状态为当前配置值。
    state
        .worker
        .direction
        .store(cfg.direction.to_u8(), Ordering::Relaxed);
    state.worker.speed.store(cfg.speed, Ordering::Relaxed);
    state
        .worker
        .compatible_mode
        .store(cfg.compatible_mode, Ordering::Relaxed);

    let direction_arc: Arc<std::sync::atomic::AtomicU8> = Arc::clone(&state.worker.direction);
    let speed_arc: Arc<std::sync::atomic::AtomicI32> = Arc::clone(&state.worker.speed);
    let compat_arc: Arc<std::sync::atomic::AtomicBool> =
        Arc::clone(&state.worker.compatible_mode);

    thread::spawn(move || {
        // 提升 Windows 定时器精度到 1ms，否则 sleep 实际精度只有约 15ms，
        // 在高刷新率屏幕上会导致无法对齐显示帧边界。
        unsafe {
            let _ = timeBeginPeriod(1);
        }

        let timer = QpcTimer::new();
        let hwnd = HWND(target_hwnd as *mut std::ffi::c_void);

        // 根据目标窗口所在显示器刷新率决定帧间隔。
        let refresh_hz = unsafe { get_monitor_refresh_rate(hwnd).clamp(MIN_REFRESH_HZ, MAX_REFRESH_HZ) };
        let frame_interval = 1.0 / refresh_hz as f64;

        // 滚动量累积器（带符号），保留帧间小数部分，保证长期速度精确。
        // 带符号后，方向热切换时会先减速到 0 再反向加速，避免突兀跳动。
        let mut accumulator: f64 = 0.0;
        // 借鉴 HTML 阅读器的平滑思想：实际速度不要瞬间跳变到目标速度，
        // 而是每帧向目标靠近一定比例，减少启动/换挡/反向时的顿挫感。
        let mut smoothed_units_per_sec: f64 = 0.0;
        const SPEED_SMOOTHING_ALPHA: f64 = 0.5;
        // 下一帧应该发送滚动事件的时间点。
        let mut next_frame_time = timer.elapsed_secs() + frame_interval;

        loop {
            // --- 精确等待到下一帧边界 ---
            let now = timer.elapsed_secs();
            let wait = next_frame_time - now;
            if wait > 0.0 {
                // 剩余时间大于 1ms 时先 sleep，避免空转浪费 CPU。
                if wait > 0.001 {
                    thread::sleep(Duration::from_secs_f64(wait - 0.000_5));
                }
                // 小余量自旋到帧边界，精度可达微秒级。
                while timer.elapsed_secs() < next_frame_time {}
            }
            next_frame_time += frame_interval;

            // 检查停止信号。
            match rx.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => break,
                Err(TryRecvError::Empty) => {}
            }

            // 目标窗口若已关闭则自动停止，并通知前端刷新状态。
            unsafe {
                if !IsWindow(hwnd).as_bool() {
                    scrolling_arc.store(false, Ordering::Relaxed);
                    let _ = app_handle.emit("state-changed", ());
                    break;
                }
            }

            // 窗口最小化时跳过本帧，避免向不可见区域发送消息。
            unsafe {
                if IsIconic(hwnd).as_bool() {
                    continue;
                }
            }

            // 读取最新速度、方向、兼容模式（全部支持热切换）。
            let speed = speed_arc.load(Ordering::Relaxed).clamp(1, 100);
            let target_units_per_sec = units_per_second_for_speed(speed);
            let direction = ScrollDirection::from_u8(direction_arc.load(Ordering::Relaxed));
            let compatible_mode = compat_arc.load(Ordering::Relaxed);
            let (msg, sign) = match direction {
                ScrollDirection::Up => (WM_MOUSEWHEEL, 1),
                ScrollDirection::Down => (WM_MOUSEWHEEL, -1),
                ScrollDirection::Right => (WM_MOUSEHWHEEL, 1),
                ScrollDirection::Left => (WM_MOUSEHWHEEL, -1),
            };

            // 借鉴 HTML 阅读器：对目标速度做低通平滑，避免瞬间跳变。
            smoothed_units_per_sec = smoothed_units_per_sec * (1.0 - SPEED_SMOOTHING_ALPHA)
                + target_units_per_sec * SPEED_SMOOTHING_ALPHA;

            // 按每帧应滚动的量累积（带符号）。
            accumulator += sign as f64 * smoothed_units_per_sec / refresh_hz as f64;

            if compatible_mode {
                // 兼容模式：只发送完整 WHEEL_DELTA 整数倍，确保旧程序能识别。
                while accumulator >= WHEEL_DELTA_F {
                    let delta = sign * WHEEL_DELTA as i32;
                    accumulator -= WHEEL_DELTA_F;
                    unsafe {
                        send_scroll_input(msg, delta);
                    }
                }
                while accumulator <= -WHEEL_DELTA_F {
                    let delta = sign * WHEEL_DELTA as i32;
                    accumulator += WHEEL_DELTA_F;
                    unsafe {
                        send_scroll_input(msg, delta);
                    }
                }
            } else {
                // 普通模式：每帧发送一次子 delta，实现最平滑的视觉效果。
                // 当累积量绝对值不足 1 个单位时跳过，避免发送无效消息。
                if accumulator >= 1.0 {
                    let delta = accumulator as i32;
                    // 减去实际发送的整数单位，保留小数部分到下一帧。
                    accumulator -= delta as f64;
                    unsafe {
                        send_scroll_message(hwnd, msg, delta);
                    }
                } else if accumulator <= -1.0 {
                    let delta = accumulator as i32;
                    accumulator -= delta as f64;
                    unsafe {
                        send_scroll_message(hwnd, msg, delta);
                    }
                }
            }
        }

        // 恢复定时器精度，避免影响系统其他程序。
        unsafe {
            let _ = timeEndPeriod(1);
        }
    });
}

/// 停止滚动工作线程。
fn stop_scroll_worker(state: &AppState) {
    if let Some(tx) = state.worker.stop_tx.lock().take() {
        let _ = tx.send(());
    }
}

/// 方式一：通过 PostMessage 直接发送滚轮消息到目标窗口。
///
/// 优点：不抢夺焦点、不移动鼠标；缺点：部分自定义 UI 程序可能不响应，
/// 或者只识别 WHEEL_DELTA 整数倍。
unsafe fn send_scroll_message(hwnd: HWND, msg: u32, delta: i32) {
    let mut rect = RECT::default();
    if GetClientRect(hwnd, &mut rect).is_ok() {
        // 取客户区中心作为滚轮事件发生位置。
        let x = ((rect.left + rect.right) / 2) as u16 as u32;
        let y = ((rect.top + rect.bottom) / 2) as u16 as u32;
        // wParam：高 16 位是滚轮增量，低 16 位是按键标志。
        let wparam = ((delta as u32) << 16) as usize;
        // lParam：高 16 位 y，低 16 位 x。
        let lparam = ((y << 16) | x) as isize;
        let _ = PostMessageW(
            hwnd,
            msg,
            windows::Win32::Foundation::WPARAM(wparam),
            windows::Win32::Foundation::LPARAM(lparam),
        );
    }
}

/// 方式二：通过 SendInput 模拟真实鼠标滚轮。
///
/// 需要目标窗口处于激活状态或鼠标位于窗口上方才有效。
/// 适合不支持 PostMessage 子 delta 的旧程序。
unsafe fn send_scroll_input(msg: u32, delta: i32) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_MOUSE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL, MOUSEINPUT,
    };

    // 根据消息类型选择垂直或水平滚轮标志。
    let dw_flags = if msg == WM_MOUSEHWHEEL {
        MOUSEEVENTF_HWHEEL
    } else {
        MOUSEEVENTF_WHEEL
    };

    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: delta as u32,
                dwFlags: dw_flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
}

/// 高精度计时器（基于 QueryPerformanceCounter）。
struct QpcTimer {
    freq: f64,
    start: i64,
}

impl QpcTimer {
    fn new() -> Self {
        let mut freq = 0i64;
        let mut start = 0i64;
        unsafe {
            let _ = QueryPerformanceFrequency(&mut freq);
            let _ = QueryPerformanceCounter(&mut start);
        }
        Self {
            freq: freq as f64,
            start,
        }
    }

    /// 返回从计时器创建以来经过的秒数。
    fn elapsed_secs(&self) -> f64 {
        let mut now = 0i64;
        unsafe {
            let _ = QueryPerformanceCounter(&mut now);
        }
        (now - self.start) as f64 / self.freq
    }
}

/// 获取指定窗口所在显示器的刷新率（Hz）。
///
/// 若无法获取则返回 `DEFAULT_REFRESH_HZ`。
unsafe fn get_monitor_refresh_rate(hwnd: HWND) -> u32 {
    // 获取窗口所在监视器句柄。
    let hmonitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    if hmonitor.is_invalid() {
        return DEFAULT_REFRESH_HZ;
    }

    // 获取监视器信息（含设备名）。
    let mut info = MONITORINFOEXW::default();
    // MONITORINFOEXW 的 cbSize 需要初始化。
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if !GetMonitorInfoW(
        hmonitor,
        &mut info as *mut MONITORINFOEXW as *mut windows::Win32::Graphics::Gdi::MONITORINFO,
    )
    .as_bool()
    {
        return DEFAULT_REFRESH_HZ;
    }

    // 通过 EnumDisplaySettingsW 读取当前显示设置的刷新率。
    let mut devmode = windows::Win32::Graphics::Gdi::DEVMODEW::default();
    devmode.dmSize = std::mem::size_of::<windows::Win32::Graphics::Gdi::DEVMODEW>() as u16;
    if EnumDisplaySettingsW(
        PCWSTR(info.szDevice.as_ptr()),
        ENUM_CURRENT_SETTINGS,
        &mut devmode,
    )
    .as_bool()
    {
        let hz = devmode.dmDisplayFrequency;
        if hz > 0 {
            return hz;
        }
    }

    DEFAULT_REFRESH_HZ
}
