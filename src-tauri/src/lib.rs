//! 自动滚屏工具 - Rust 后端入口
//!
//! 本文件只负责组合各个功能模块、初始化应用状态、注册 Tauri 命令。
//! 具体功能已拆分到以下模块：
//! - `config`：配置加载与保存
//! - `window`：窗口枚举与目标窗口管理
//! - `state`：应用运行时状态
//! - `scroll`：滚屏控制与工作线程
//! - `hotkey`：全局热键解析、注册与监听
//! - `tray`：系统托盘
//! - `single_instance`：单实例互斥量

pub mod config;
pub mod hotkey;
pub mod scroll;
pub mod single_instance;
pub mod state;
pub mod tray;
pub mod window;

use tauri::{AppHandle, Manager};

use crate::config::load_config;
use crate::hotkey::{init_hotkey_manager, start_hotkey_listener};
use crate::state::AppState;
use crate::tray::setup_tray;

/// Tauri 应用入口。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            // 加载配置并初始化状态。
            let config = load_config(&app.handle());
            let hotkey_str = config.hotkey.clone();
            app.manage(AppState::new(config));

            // 确保主窗口显示并聚焦。
            show_main_window(&app.handle());

            // 拦截窗口关闭请求，改为隐藏到系统托盘，避免误关闭。
            if let Some(window) = app.get_webview_window("main") {
                let window_for_close = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_for_close.hide();
                    }
                });
            }

            // 设置全局热键并启动监听线程。
            // 热键注册失败（例如被其他程序占用）不应导致整个应用退出，
            // 因此只记录错误，用户仍可在界面上看到并修改热键。
            if let Err(e) = init_hotkey_manager(&hotkey_str) {
                eprintln!("全局热键初始化失败: {}", e);
            }
            start_hotkey_listener(&app.handle());

            // 设置系统托盘。
            if let Err(e) = setup_tray(&app.handle()) {
                eprintln!("系统托盘初始化失败: {}", e);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            window::list_windows,
            window::set_target,
            window::clear_target,
            scroll::set_speed,
            scroll::set_direction,
            scroll::set_compatible_mode,
            hotkey::set_hotkey,
            scroll::toggle_scroll,
            state::get_status,
            state::set_language,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // 运行事件循环，在程序退出前主动隐藏托盘图标，避免 Windows 资源管理器残留。
    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(tray) = app_handle.tray_by_id("main") {
                let _ = tray.set_visible(false);
            }
        }
    });
}

/// 显示并聚焦主窗口，确保启动后窗口正常可见。
fn show_main_window(app_handle: &AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
