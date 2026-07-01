// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 先检查是否已有实例在运行。若有，则激活已有窗口并退出，避免多开冲突。
    if !auto_scroll_lib::single_instance::ensure_single_instance() {
        return;
    }

    auto_scroll_lib::run()
}
