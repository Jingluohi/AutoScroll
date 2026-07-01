//! 系统托盘模块
//!
//! 负责创建托盘图标、托盘右键菜单，以及处理托盘点击事件。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

/// 构建系统托盘菜单与行为。
pub fn setup_tray(app_handle: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // 创建托盘菜单项：仅保留用户要求的【显示主窗口】和【退出程序】。
    let show_i = MenuItem::with_id(app_handle, "show", "显示主窗口", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app_handle, "quit", "退出程序", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app_handle)?;

    let menu = Menu::with_items(app_handle, &[&show_i, &separator, &quit_i])?;

    TrayIconBuilder::with_id("main")
        .icon(app_handle.default_window_icon().unwrap().clone())
        .menu(&menu)
        // 左键点击不要弹出菜单，右键由下面的事件处理显式显示菜单。
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                show_main_window(app.app_handle());
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            match event {
                // 左键点击或双击托盘图标：显示主窗口。
                tauri::tray::TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Left,
                    ..
                }
                | tauri::tray::TrayIconEvent::DoubleClick { .. } => {
                    show_main_window(tray.app_handle());
                }
                // 右键点击不拦截，让 Tauri 自动弹出关联的右键菜单。
                _ => {}
            }
        })
        .build(app_handle)?;

    Ok(())
}

/// 显示并聚焦主窗口。
fn show_main_window(app_handle: &AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
