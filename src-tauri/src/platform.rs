use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, Default)]
pub struct Origin {
    pub window: isize,
    pub process: u32,
}

#[cfg(windows)]
pub fn remember_origin() -> Origin {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        Origin { window: hwnd.0 as isize, process: pid }
    }
}

#[cfg(windows)]
pub fn restore_origin(origin: Origin) -> bool {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow, SetForegroundWindow}};
    if origin.window == 0 || origin.process == std::process::id() { return false; }
    unsafe {
        let hwnd = HWND(origin.window as *mut _);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if !IsWindow(Some(hwnd)).as_bool() || pid != origin.process { return false; }
        SetForegroundWindow(hwnd).as_bool()
    }
}

#[cfg(windows)]
pub fn cursor() -> AppResult<(i32, i32)> {
    use windows::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.map_err(|_| AppError::new("cursor", "无法读取鼠标位置。", true))?;
    Ok((point.x, point.y))
}

#[cfg(windows)]
pub fn ensure_capture_permission() -> AppResult<()> { Ok(()) }

#[cfg(target_os = "macos")]
pub fn remember_origin() -> Origin {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::Object;
    objc::rc::autoreleasepool(|| unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let application: *mut Object = msg_send![workspace, frontmostApplication];
        if application.is_null() { return Origin::default(); }
        let process: i32 = msg_send![application, processIdentifier];
        Origin { window: 0, process: process as u32 }
    })
}

#[cfg(target_os = "macos")]
pub fn restore_origin(origin: Origin) -> bool {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::{Object, BOOL, YES};
    if origin.process == 0 || origin.process == std::process::id() { return false; }
    unsafe {
        let application: *mut Object = msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: origin.process as i32];
        if application.is_null() { return false; }
        let activated: BOOL = msg_send![application, activateWithOptions: 2usize];
        activated == YES
    }
}

#[cfg(target_os = "macos")]
pub fn cursor() -> AppResult<(i32, i32)> {
    use core_graphics::{event::CGEvent, event_source::{CGEventSource, CGEventSourceStateID}};
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| AppError::new("cursor", "无法读取鼠标位置。", true))?;
    let event = CGEvent::new(source).map_err(|_| AppError::new("cursor", "无法读取鼠标位置。", true))?;
    let point = event.location();
    Ok((point.x.round() as i32, point.y.round() as i32))
}

#[cfg(target_os = "macos")]
pub fn ensure_capture_permission() -> AppResult<()> {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    unsafe {
        if !CGPreflightScreenCaptureAccess() && !CGRequestScreenCaptureAccess() {
            return Err(AppError::new("screen_permission", "请在系统设置 → 隐私与安全性 → 屏幕录制中允许 My Snip，然后重新截图。", true));
        }
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn remember_origin() -> Origin { Origin::default() }
#[cfg(not(any(windows, target_os = "macos")))]
pub fn restore_origin(_: Origin) -> bool { false }
#[cfg(not(any(windows, target_os = "macos")))]
pub fn cursor() -> AppResult<(i32, i32)> { Err(AppError::new("platform", "首版支持 Windows 和 macOS。", false)) }
#[cfg(not(any(windows, target_os = "macos")))]
pub fn ensure_capture_permission() -> AppResult<()> { Ok(()) }
