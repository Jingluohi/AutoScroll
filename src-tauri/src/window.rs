//! 窗口枚举与目标窗口管理模块
//!
//! 负责枚举系统中的可见窗口，以及读取/设置用户选择的目标窗口。

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindow,
    IsIconic, IsWindowVisible, GW_OWNER, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

use crate::state::AppState;

/// 窗口信息，返回给前端供用户选择目标窗口。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    /// 窗口句柄，以 isize 形式跨进程/跨语言传递。
    pub hwnd: isize,
    /// 窗口标题，用于用户识别。
    pub title: String,
}

/// 枚举所有可见窗口，返回窗口句柄和标题列表。
///
/// 过滤条件：
/// - 窗口可见（`IsWindowVisible`）
/// - 窗口未最小化（`IsIconic`）
/// - 窗口没有所有者（`GW_OWNER`），避免把对话框、浮动工具栏等误当作主窗口
/// - 窗口不是工具窗口（`WS_EX_TOOLWINDOW`）
/// - 窗口类名不是系统外壳窗口（任务栏、桌面等）
/// - 窗口有实际可见区域（宽、高均 ≥ 10 像素）
/// - 标题非空
///
/// 这样可以排除“只有任务栏图标/托盘图标但无实际窗口”的幽灵窗口。
#[tauri::command]
pub fn list_windows() -> Vec<WindowInfo> {
    let mut windows: Vec<WindowInfo> = Vec::new();
    let user_data: *mut Vec<WindowInfo> = &mut windows;

    unsafe {
        // EnumWindows 会遍历所有顶层窗口，通过回调函数收集信息。
        let _ = EnumWindows(Some(enum_window_callback), LPARAM(user_data as isize));
    }

    // 按标题排序，方便前端展示。
    windows.sort_by(|a, b| a.title.cmp(&b.title));
    windows
}

/// EnumWindows 回调函数：过滤出有实际界面的可见顶层窗口并收集标题。
unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // 1. 只保留可见窗口。
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }

    // 2. 排除最小化窗口（最小化后通常缩到任务栏，没有实际可见区域）。
    if IsIconic(hwnd).as_bool() {
        return BOOL(1);
    }

    // 3. 排除有所有者窗口（如对话框、浮动工具条等）。
    match GetWindow(hwnd, GW_OWNER) {
        Ok(owner) if !owner.is_invalid() => return BOOL(1),
        _ => {}
    }

    // 4. 排除工具窗口（WS_EX_TOOLWINDOW），这类窗口通常只在托盘/任务栏存在图标，
    //    没有标准标题栏和可见界面。
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
        return BOOL(1);
    }

    // 5. 排除已知系统外壳窗口类名。
    let mut class_buf = [0u16; 256];
    let class_len = GetClassNameW(hwnd, &mut class_buf);
    if class_len > 0 {
        let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);
        let class_name_lower = class_name.to_lowercase();
        // Shell_TrayWnd: 任务栏本身；Progman/WorkerW: 桌面外壳窗口；
        // CoreWindow: 部分仅挂任务栏的后台 UWP 容器窗口。
        if matches!(
            class_name_lower.as_str(),
            "shell_traywnd" | "progman" | "workerw" | "windows.ui.core.corewindow"
        ) {
            return BOOL(1);
        }
    }

    // 6. 排除没有实际可见区域的窗口（例如某些只挂在任务栏的后台窗口）。
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        return BOOL(1);
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    // 宽、高均至少 10 像素，避免 1×1 的幽灵占位窗口混进来。
    if width < 10 || height < 10 {
        return BOOL(1);
    }

    // 7. 读取窗口标题，过滤掉空标题窗口。
    let mut buffer = [0u16; 512];
    let len = GetWindowTextW(hwnd, &mut buffer);
    if len > 0 {
        let title = String::from_utf16_lossy(&buffer[..len as usize]);
        if !title.trim().is_empty() {
            let windows = &mut *(lparam.0 as *mut Vec<WindowInfo>);
            windows.push(WindowInfo {
                hwnd: hwnd.0 as isize,
                title: title.trim().to_string(),
            });
        }
    }

    // 返回 TRUE 继续枚举。
    BOOL(1)
}

/// 获取指定窗口的标题。
pub fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let mut buffer = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buffer);
        String::from_utf16_lossy(&buffer[..len as usize])
            .trim()
            .to_string()
    }
}

/// 设置目标窗口。
#[tauri::command]
pub fn set_target(
    hwnd: isize,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    state.config.lock().target_hwnd = Some(hwnd);
    state.save_config(&app_handle)?;
    // 通知前端状态已更新。
    let _ = app_handle.emit("state-changed", ());
    Ok(())
}

/// 清除目标窗口。
#[tauri::command]
pub fn clear_target(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    state.config.lock().target_hwnd = None;
    state.save_config(&app_handle)?;
    let _ = app_handle.emit("state-changed", ());
    Ok(())
}
