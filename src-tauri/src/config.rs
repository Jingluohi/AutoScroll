//! 应用配置模块
//!
//! 负责定义配置结构、配置文件路径、加载与保存。
//! 配置文件保存在 `%APPDATA%/com.yjh.auto-scroll/config.json`，
//! 这样程序复制到任何地方运行时都能自动保留用户设置。

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// 界面语言。
///
/// 支持中文与英文，序列化时保存为小写字符串。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// 简体中文。
    Zh,
    /// 英文。
    En,
}

impl Default for Language {
    fn default() -> Self {
        Self::Zh
    }
}

/// 滚动方向。
///
/// 支持四个方向：上下左右。序列化时保存为小写字符串。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScrollDirection {
    /// 向上滚屏。
    Up,
    /// 向下滚屏（默认）。
    Down,
    /// 向左滚屏。
    Left,
    /// 向右滚屏。
    Right,
}

impl Default for ScrollDirection {
    fn default() -> Self {
        Self::Down
    }
}

/// 应用持久化配置。
///
/// 所有字段都会被序列化到 JSON 配置文件，应用启动时自动加载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 目标窗口句柄，None 表示尚未选择。
    pub target_hwnd: Option<isize>,

    /// 滚动速度，范围 1~100。
    /// 第 1 档为原 1 档的 90%，第 100 档为原 50 档，1~100 档严格线性均匀递增。
    pub speed: i32,

    /// 滚动方向。
    pub direction: ScrollDirection,

    /// 全局快捷键字符串，例如 "Ctrl+Alt+S"。
    pub hotkey: String,

    /// 是否使用兼容模式（SendInput 模拟真实滚轮），默认使用 PostMessage。
    pub compatible_mode: bool,

    /// 界面语言，默认中文。
    pub language: Language,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            target_hwnd: None,
            speed: 50,
            direction: ScrollDirection::Down,
            hotkey: "Ctrl+Alt+S".to_string(),
            compatible_mode: false,
            language: Language::Zh,
        }
    }
}

/// 获取配置文件完整路径。
///
/// 使用 Tauri 提供的 `app_data_dir`，确保在不同 Windows 用户/路径下都能正确定位。
pub fn config_path(app_handle: &AppHandle) -> Result<std::path::PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    Ok(app_data_dir.join("config.json"))
}

/// 从磁盘加载配置。
///
/// 如果配置文件不存在或解析失败，返回默认配置，避免启动失败。
pub fn load_config(app_handle: &AppHandle) -> AppConfig {
    if let Ok(path) = config_path(app_handle) {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<AppConfig>(&content) {
                    return cfg;
                }
            }
        }
    }
    AppConfig::default()
}

/// 将配置写入磁盘。
///
/// 先创建父目录，再将 JSON 写入，供 `AppState::save_config` 调用。
pub fn write_config(app_handle: &AppHandle, cfg: &AppConfig) -> Result<(), String> {
    let path = config_path(app_handle)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }
    let content = serde_json::to_string_pretty(cfg).map_err(|e| format!("序列化配置失败: {}", e))?;
    std::fs::write(&path, content).map_err(|e| format!("写入配置失败: {}", e))?;
    Ok(())
}
