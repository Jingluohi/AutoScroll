//! 全局热键模块
//!
//! 负责解析热键字符串、管理全局热键管理器、注册/注销热键，
//! 并在后台线程中监听热键事件，触发滚屏开关。

use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::scroll::toggle_scroll_internal;
use crate::state::AppState;

/// 全局热键管理器包装。
///
/// `GlobalHotKeyManager` 内部包含原始指针，未实现 `Send`/`Sync`，
/// 无法直接放入 static Mutex。这里用原始指针包装，并手动实现
/// `Send`/`Sync`（实际只在主线程访问），从而全局保存。
struct GlobalHotkeyManagerWrapper(*const GlobalHotKeyManager);
unsafe impl Send for GlobalHotkeyManagerWrapper {}
unsafe impl Sync for GlobalHotkeyManagerWrapper {}

/// 全局热键管理器静态引用（通过原始指针）。
static HOTKEY_MANAGER: Mutex<Option<GlobalHotkeyManagerWrapper>> = Mutex::new(None);

/// 当前已注册的全局热键，用于修改热键时先注销旧热键，避免 AlreadyRegistered 错误。
static CURRENT_HOTKEY: Mutex<Option<HotKey>> = Mutex::new(None);

/// 设置全局快捷键。
#[tauri::command]
pub fn set_hotkey(
    hotkey_str: String,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    // 先解析验证新热键是否合法。
    let _new_hotkey = parse_hotkey(&hotkey_str)?;

    // 注册到系统全局热键。
    init_hotkey_manager(&hotkey_str)?;

    // 保存配置。
    state.config.lock().hotkey = hotkey_str;
    state.save_config(&app_handle)?;

    let _ = app_handle.emit("state-changed", ());
    Ok(())
}

/// 初始化/更新全局热键。
///
/// 首次调用时创建管理器；后续调用时先注销旧热键，再注册新热键。
pub fn init_hotkey_manager(hotkey_str: &str) -> Result<(), String> {
    let hotkey = parse_hotkey(hotkey_str)?;

    // 获取或创建全局唯一的热键管理器（通过原始指针保持 'static 生命周期）。
    let manager_ptr: *const GlobalHotKeyManager = {
        let mut guard = HOTKEY_MANAGER.lock().unwrap();
        if let Some(wrapper) = guard.as_ref() {
            wrapper.0
        } else {
            let manager = Box::new(
                GlobalHotKeyManager::new()
                    .map_err(|e| format!("热键管理器初始化失败: {:?}", e))?,
            );
            let raw = Box::into_raw(manager);
            *guard = Some(GlobalHotkeyManagerWrapper(raw));
            raw
        }
    };

    // 安全：manager 由 Box::into_raw 产生，拥有 'static 生命周期，且不会被释放。
    let manager = unsafe { &*manager_ptr };

    // 若已有旧热键，先注销，避免重复注册导致 AlreadyRegistered 错误。
    if let Some(old_hotkey) = CURRENT_HOTKEY.lock().unwrap().take() {
        let _ = manager.unregister(old_hotkey);
    }

    manager
        .register(hotkey)
        .map_err(|e| format!("注册热键 [{}] 失败: {:?}", hotkey_str, e))?;

    // 保存当前热键，供后续修改时使用。
    *CURRENT_HOTKEY.lock().unwrap() = Some(hotkey);

    Ok(())
}

/// 启动全局热键监听线程，只应在应用初始化时调用一次。
pub fn start_hotkey_listener(app_handle: &AppHandle) {
    let app_handle_clone = app_handle.clone();
    thread::spawn(move || {
        let receiver = GlobalHotKeyEvent::receiver();
        loop {
            if let Ok(event) = receiver.try_recv() {
                if event.state == HotKeyState::Pressed {
                    // 直接调用切换逻辑。
                    let state = app_handle_clone.state::<AppState>();
                    let _ = toggle_scroll_internal(&*state, &app_handle_clone);
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
}

/// 解析热键字符串，例如 "Ctrl+Alt+S"。
fn parse_hotkey(s: &str) -> Result<HotKey, String> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return Err("热键格式不能为空".to_string());
    }

    let mut modifiers = Modifiers::empty();
    let mut key_part = "";

    for part in &parts {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "alt" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            "cmd" | "command" | "win" | "meta" => modifiers |= Modifiers::SUPER,
            _ => key_part = part,
        }
    }

    if key_part.is_empty() {
        return Err("热键必须包含一个主键".to_string());
    }

    let code = parse_code(key_part)?;
    Ok(HotKey::new(Some(modifiers), code))
}

/// 将主键字符串解析为 global-hotkey 的 Code。
fn parse_code(s: &str) -> Result<Code, String> {
    let upper = s.to_uppercase();

    // 功能键 F1-F24。
    if upper.starts_with('F') && upper.len() > 1 {
        if let Ok(n) = upper[1..].parse::<u8>() {
            return match n {
                1 => Ok(Code::F1),
                2 => Ok(Code::F2),
                3 => Ok(Code::F3),
                4 => Ok(Code::F4),
                5 => Ok(Code::F5),
                6 => Ok(Code::F6),
                7 => Ok(Code::F7),
                8 => Ok(Code::F8),
                9 => Ok(Code::F9),
                10 => Ok(Code::F10),
                11 => Ok(Code::F11),
                12 => Ok(Code::F12),
                13 => Ok(Code::F13),
                14 => Ok(Code::F14),
                15 => Ok(Code::F15),
                16 => Ok(Code::F16),
                17 => Ok(Code::F17),
                18 => Ok(Code::F18),
                19 => Ok(Code::F19),
                20 => Ok(Code::F20),
                21 => Ok(Code::F21),
                22 => Ok(Code::F22),
                23 => Ok(Code::F23),
                24 => Ok(Code::F24),
                _ => Err(format!("不支持的功能键: {}", s)),
            };
        }
    }

    // 字母键、数字键与常用控制键。
    match upper.as_str() {
        "A" => Ok(Code::KeyA),
        "B" => Ok(Code::KeyB),
        "C" => Ok(Code::KeyC),
        "D" => Ok(Code::KeyD),
        "E" => Ok(Code::KeyE),
        "F" => Ok(Code::KeyF),
        "G" => Ok(Code::KeyG),
        "H" => Ok(Code::KeyH),
        "I" => Ok(Code::KeyI),
        "J" => Ok(Code::KeyJ),
        "K" => Ok(Code::KeyK),
        "L" => Ok(Code::KeyL),
        "M" => Ok(Code::KeyM),
        "N" => Ok(Code::KeyN),
        "O" => Ok(Code::KeyO),
        "P" => Ok(Code::KeyP),
        "Q" => Ok(Code::KeyQ),
        "R" => Ok(Code::KeyR),
        "S" => Ok(Code::KeyS),
        "T" => Ok(Code::KeyT),
        "U" => Ok(Code::KeyU),
        "V" => Ok(Code::KeyV),
        "W" => Ok(Code::KeyW),
        "X" => Ok(Code::KeyX),
        "Y" => Ok(Code::KeyY),
        "Z" => Ok(Code::KeyZ),
        "0" => Ok(Code::Digit0),
        "1" => Ok(Code::Digit1),
        "2" => Ok(Code::Digit2),
        "3" => Ok(Code::Digit3),
        "4" => Ok(Code::Digit4),
        "5" => Ok(Code::Digit5),
        "6" => Ok(Code::Digit6),
        "7" => Ok(Code::Digit7),
        "8" => Ok(Code::Digit8),
        "9" => Ok(Code::Digit9),
        "SPACE" | " " => Ok(Code::Space),
        "ENTER" | "RETURN" => Ok(Code::Enter),
        "ESC" | "ESCAPE" => Ok(Code::Escape),
        "TAB" => Ok(Code::Tab),
        "UP" => Ok(Code::ArrowUp),
        "DOWN" => Ok(Code::ArrowDown),
        "LEFT" => Ok(Code::ArrowLeft),
        "RIGHT" => Ok(Code::ArrowRight),
        _ => Err(format!("不支持的按键: {}", s)),
    }
}
