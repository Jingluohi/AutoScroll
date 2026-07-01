//! 单实例模块
//!
//! 通过 Windows 命名互斥量实现“同时只能运行一个程序实例”。
//! 当用户重复点击 exe 时，若已有实例在运行，则激活已有窗口并退出新进程，
//! 避免任务栏出现多个隐藏图标或多个窗口冲突。
//!
//! 注意：互斥量句柄必须保持打开直到进程退出，因此保存到全局原子指针中。

use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_SHOW,
};

/// 保存单实例互斥量句柄（原始指针形式），确保进程退出前不会释放。
static SINGLE_INSTANCE_MUTEX: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(ptr::null_mut());

/// 尝试成为唯一运行实例。
///
/// 返回 `true` 表示当前进程是首个实例，可以继续运行；
/// 返回 `false` 表示已有实例在运行，此时会尝试激活已有窗口，然后当前进程应退出。
pub fn ensure_single_instance() -> bool {
    unsafe {
        // 创建一个命名互斥量。若已存在同名互斥量，则 GetLastError 会返回 ERROR_ALREADY_EXISTS。
        let mutex = match CreateMutexW(None, true, windows::core::w!("com.yjh.auto-scroll.single-instance")) {
            Ok(handle) => handle,
            Err(_) => {
                // 连互斥量都创建失败，保守地允许当前实例运行。
                return true;
            }
        };

        if GetLastError() == ERROR_ALREADY_EXISTS {
            // 已有实例在运行，尝试找到并显示它的主窗口。
            activate_existing_window();
            return false;
        }

        // 首个实例：把互斥量句柄保存到全局原子指针，保持打开直到进程退出。
        // 若句柄被释放，第二个实例又能创建同名互斥量，导致任务栏出现多个隐藏图标。
        SINGLE_INSTANCE_MUTEX.store(mutex.0, Ordering::SeqCst);
        true
    }
}

/// 查找并激活已运行实例的主窗口。
fn activate_existing_window() {
    unsafe {
        let title = windows::core::w!("自动滚屏");
        if let Ok(hwnd) = FindWindowW(None, title) {
            if hwnd != HWND(std::ptr::null_mut()) {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}
