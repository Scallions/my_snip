use crate::{capture::{self, FrozenCapture, MonitorBounds, Selection}, error::{AppError, AppResult}, format::{self, OutputFormat}, platform, settings::{SettingsInput, SettingsStore, SettingsView}, state::{stale_window, AppState, Page, Phase, Runtime, Session, ViewState, STATE_EVENT}, vision::{self, Recognition}};
use std::{sync::{Arc, Mutex}, time::Duration};
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use zeroize::Zeroizing;

fn id() -> String { Uuid::new_v4().to_string() }
fn busy() -> AppError { AppError::new("busy", "正在处理当前操作，请稍候。", true) }

fn local_navigation(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost")
        || (cfg!(debug_assertions) && matches!(url.host_str(), Some("localhost" | "127.0.0.1")) && url.port() == Some(1420))
}

pub fn emit_state(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let label = state.runtime.lock().ok().and_then(|r| r.window_label.clone());
    if let Some(label) = label {
        if let Ok(view) = state.snapshot(&label) { let _ = app.emit_to(label, STATE_EVENT, view); }
    }
}

pub fn register_shortcut(app: &tauri::AppHandle, value: &str) -> AppResult<()> {
    let shortcut: Shortcut = value.parse().map_err(|_| AppError::new("shortcut", "快捷键格式无效，例如 Ctrl+Shift+M；Mac 可使用 Command+Shift+M。", false))?;
    app.global_shortcut().register(shortcut)
        .map_err(|_| AppError::new("shortcut_conflict", "这个快捷键无法注册，可能已被其他应用占用，请更换组合键。", false))
}

fn current_origin(state: &AppState) -> platform::Origin {
    let current = platform::remember_origin();
    if current.process == std::process::id() {
        state.runtime.lock().map(|r| r.origin).unwrap_or_default()
    } else { current }
}

pub fn cleanup(app: &tauri::AppHandle, label: Option<&str>, restore: bool) -> AppResult<()> {
    let state = app.state::<AppState>();
    let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
    if label.is_some() && r.window_label.as_deref() != label { return Err(stale_window()); }
    let previous = r.window_label.take();
    let origin = r.origin;
    if let Some(session) = r.session.take() { session.cancel.cancel(); }
    if let Some(cancel) = r.test_cancel.take() { cancel.cancel(); }
    r.capture = None;
    r.error = None;
    r.notice = None;
    r.phase = Phase::Dormant;
    r.generation += 1;
    r.window_generation += 1;
    drop(r);
    if let Some(label) = previous {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.hide();
            window.destroy().map_err(|_| AppError::new("window_close", "窗口未能关闭，请从托盘退出后重新打开。", true))?;
        }
    }
    if restore {
        let _ = app.run_on_main_thread(move || { platform::restore_origin(origin); });
    }
    crate::diagnostic_log::record("cleanup", "ok", None);
    Ok(())
}

fn monitor_bounds(monitor: &tauri::Monitor) -> MonitorBounds {
    MonitorBounds { x: monitor.position().x, y: monitor.position().y, width: monitor.size().width,
        height: monitor.size().height, scale_factor: monitor.scale_factor() }
}

pub fn open_workbench(app: &tauri::AppHandle, page: Page) -> AppResult<()> {
    let state = app.state::<AppState>();
    let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
    if let Some(label) = r.window_label.as_ref().filter(|l| l.starts_with("workbench-")) {
        if let Some(window) = app.get_webview_window(label) {
            r.page = page;
            drop(r);
            resize_workbench(&window, page);
            emit_state(app);
            let _ = window.show();
            let _ = window.set_focus();
            return Ok(());
        }
    }
    if let Some(previous) = r.window_label.take() {
        if let Some(window) = app.get_webview_window(&previous) { let _ = window.hide(); let _ = window.destroy(); }
    }
    r.window_generation += 1;
    let label = format!("workbench-{}", id());
    r.window_label = Some(label.clone());
    r.page = page;
    let bounds = r.preferred_monitor.clone();
    drop(r);
    let height = if page == Page::Settings { 592.0 } else { 418.0 };
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("My Snip")
        .inner_size(820.0, height).min_inner_size(620.0, 360.0)
        .decorations(false).resizable(true).visible(false).shadow(true)
        .background_color(tauri::webview::Color(39,43,47,255))
        .on_navigation(local_navigation)
        .build().map_err(|_| AppError::new("window", "无法创建结果窗口，请重新启动应用。", true))?;
    if let Some(b) = bounds {
        let width = (820.0 * b.scale_factor).round() as i32;
        let physical_height = (height * b.scale_factor).round() as i32;
        let _ = window.set_position(PhysicalPosition::new(b.x + ((b.width as i32 - width) / 2).max(0), b.y + ((b.height as i32 - physical_height) / 2).max(0)));
    } else { let _ = window.center(); }
    Ok(())
}

fn resize_workbench(window: &WebviewWindow, page: Page) {
    let _ = window.set_size(tauri::LogicalSize::new(820.0, if page == Page::Settings { 592.0 } else { 418.0 }));
}

pub async fn show_settings(app: tauri::AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let _operation = state.operations.lock().await;
    let origin = current_origin(&state);
    {
        let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
        r.origin = origin;
    }
    open_workbench(&app, Page::Settings)
}

pub async fn begin_capture_impl(app: tauri::AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let _operation = state.operations.try_lock().map_err(|_| busy())?;
    let configured = (|| -> AppResult<bool> {
        let settings = state.settings.lock().map_err(|_| AppError::internal())?;
        Ok(!settings.value.base_url.is_empty() && !settings.value.model.is_empty() && settings.secret()?.is_some())
    })();
    let configured = match configured {
        Ok(configured) => configured,
        Err(error) => { state.runtime.lock().map_err(|_| AppError::internal())?.error = Some(error); return open_workbench(&app, Page::Settings); }
    };
    if !configured {
        state.runtime.lock().map_err(|_| AppError::internal())?.notice = Some("先填写模型接口，再按截图快捷键开始。".into());
        return open_workbench(&app, Page::Settings);
    }
    let origin = current_origin(&state);
    cleanup(&app, None, false)?;
    // CoreGraphics uses display points on macOS; Windows uses desktop pixels.
    // Feed the same frozen platform point to both monitor APIs.
    let capture_point = platform::cursor()?;
    let monitor = app.monitor_from_point(capture_point.0 as f64, capture_point.1 as f64).ok().flatten().or_else(|| app.primary_monitor().ok().flatten())
        .ok_or_else(|| AppError::new("monitor", "无法读取显示器信息。", true))?;
    let bounds = monitor_bounds(&monitor);
    // Freeze the xcap point too: moving the cursor during compositor delay must
    // not capture a different monitor from the one hosting the overlay.
    {
        let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
        r.origin = origin;
        r.preferred_monitor = Some(bounds.clone());
    }
    // Let the compositor remove our own windows before the frozen frame.
    tokio::time::sleep(Duration::from_millis(110)).await;
    let taken = tauri::async_runtime::spawn_blocking(move || {
        let image = capture::take_screenshot(capture_point)?;
        let png = capture::encode_png(&image)?;
        Ok::<_, AppError>((image, png))
    }).await.map_err(|_| AppError::internal())?;
    let (image, png) = match taken {
        Ok(value) => value,
        Err(error) => {
            state.runtime.lock().map_err(|_| AppError::internal())?.error = Some(error.clone());
            open_workbench(&app, Page::Settings)?;
            return Err(error);
        }
    };
    let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
    r.generation += 1;
    r.window_generation += 1;
    let generation = r.generation;
    let label = format!("capture-{}", id());
    r.capture = Some(FrozenCapture { generation, image, png: Arc::new(png), image_token: id(), bounds: bounds.clone() });
    r.window_label = Some(label.clone());
    r.phase = Phase::Selecting;
    r.page = Page::Capture;
    drop(r);
    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title("My Snip — 选择公式").decorations(false).resizable(false)
        .always_on_top(true).skip_taskbar(true).visible(false).shadow(false)
        .background_color(tauri::webview::Color(20,23,26,255))
        .on_navigation(local_navigation)
        .build().map_err(|_| AppError::new("window", "无法打开截图框选窗口。", true))?;
    window.set_position(PhysicalPosition::new(bounds.x, bounds.y)).map_err(|_| AppError::internal())?;
    window.set_size(PhysicalSize::new(bounds.width, bounds.height)).map_err(|_| AppError::internal())?;
    Ok(())
}

async fn start_recognition(app: tauri::AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let (settings, secret) = {
        let store = state.settings.lock().map_err(|_| AppError::internal())?;
        (store.value.clone(), store.secret()?.ok_or_else(|| AppError::new("api_key", "请先设置 API Key。", false))?)
    };
    let (session_id, request_id, image, cancellation) = {
        let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
        if let Some(cancel) = r.test_cancel.take() { cancel.cancel(); }
        let session = r.session.as_mut().ok_or_else(|| AppError::new("session", "截图会话已结束，请重新截图。", false))?;
        session.cancel.cancel();
        session.cancel = CancellationToken::new();
        let request_id = id();
        session.request_id = Some(request_id.clone());
        let result = (session.id.clone(), request_id, session.image.clone(), session.cancel.clone());
        r.phase = Phase::Recognizing;
        r.error = None;
        r.page = Page::Result;
        result
    };
    emit_state(&app);
    tauri::async_runtime::spawn(async move {
        let result = vision::recognize(&settings.base_url, &settings.model, &secret, &image, cancellation).await;
        match &result {
            Ok(value) => crate::diagnostic_log::record("recognition", "ok", Some(value.elapsed_ms)),
            Err(error) => crate::diagnostic_log::record("recognition", &error.code, None),
        }
        let state = app.state::<AppState>();
        let Ok(mut r) = state.runtime.lock() else { return; };
        if r.phase != Phase::Recognizing { return; }
        let Some(session) = r.session.as_mut().filter(|s| s.id == session_id && s.request_id.as_ref() == Some(&request_id)) else { return; };
        match result {
            Ok(result) => {
                session.latex = result.latex;
                session.elapsed_ms = Some(result.elapsed_ms);
                r.phase = Phase::Ready;
                r.error = None;
            }
            Err(error) if error.code == "cancelled" => { return; }
            Err(error) => { r.phase = Phase::Error; r.error = Some(error); }
        }
        drop(r);
        emit_state(&app);
    });
    Ok(())
}

#[tauri::command]
pub fn initial_state(app: tauri::AppHandle, window: WebviewWindow) -> AppResult<ViewState> {
    app.state::<AppState>().snapshot(window.label())
}

#[tauri::command]
pub fn window_ready(app: tauri::AppHandle, window: WebviewWindow) -> AppResult<()> {
    app.state::<AppState>().snapshot(window.label())?;
    window.show().map_err(|_| AppError::internal())?;
    window.set_focus().map_err(|_| AppError::internal())?;
    Ok(())
}

#[tauri::command]
pub async fn begin_capture(app: tauri::AppHandle, window: WebviewWindow) -> AppResult<()> {
    app.state::<AppState>().authorize_workbench(window.label())?;
    begin_capture_impl(app).await
}

#[tauri::command]
pub async fn commit_selection(app: tauri::AppHandle, window: WebviewWindow, generation: u64, selection: Selection) -> AppResult<()> {
    let state = app.state::<AppState>();
    let _operation = state.operations.lock().await;
    let capture = {
        let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
        if r.window_label.as_deref() != Some(window.label()) || r.page != Page::Capture { return Err(stale_window()); }
        if r.capture.as_ref().map(|c| c.generation) != Some(generation) { return Err(stale_window()); }
        r.capture.take().ok_or_else(stale_window)?
    };
    let monitors = app.available_monitors().map_err(|_| AppError::internal())?;
    if !monitors.iter().any(|m| monitor_bounds(m) == capture.bounds) {
        cleanup(&app, Some(window.label()), true)?;
        return Err(AppError::new("display_changed", "显示器设置已变化，请重新截图。", true));
    }
    let (capture, result) = tauri::async_runtime::spawn_blocking(move || {
        let result = capture::crop(&capture, &selection);
        (capture, result)
    }).await.map_err(|_| AppError::internal())?;
    let (png, width, height) = match result {
        Ok(result) => result,
        Err(error) => {
            state.runtime.lock().map_err(|_| AppError::internal())?.capture = Some(capture);
            return Err(error);
        }
    };
    let default = state.settings.lock().map_err(|_| AppError::internal())?.value.default_format;
    {
        let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
        r.session = Some(Session { id: id(), generation, image: Arc::new(png), image_token: id(), width, height,
            latex: String::new(), format: default, request_id: None, cancel: CancellationToken::new(), elapsed_ms: None });
        r.phase = Phase::Recognizing;
        r.error = None;
        r.page = Page::Result;
    }
    drop(capture); // Release the full-screen frame before creating the result WebView.
    open_workbench(&app, Page::Result)?;
    if let Err(error) = start_recognition(app.clone()).await {
        let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
        r.phase = Phase::Error;r.error = Some(error);
        drop(r);emit_state(&app);
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_session(app: tauri::AppHandle, window: WebviewWindow) -> AppResult<()> {
    let state = app.state::<AppState>();
    let _operation = state.operations.lock().await;
    cleanup(&app, Some(window.label()), true)
}

#[tauri::command]
pub async fn retry_recognition(app: tauri::AppHandle, window: WebviewWindow, session_id: String) -> AppResult<()> {
    let state = app.state::<AppState>();
    state.authorize_workbench(window.label())?;
    let _operation = state.operations.lock().await;
    if state.runtime.lock().map_err(|_| AppError::internal())?.session.as_ref().map(|s| s.id.as_str()) != Some(session_id.as_str()) {
        return Err(stale_window());
    }
    start_recognition(app.clone()).await
}

#[tauri::command]
pub async fn copy_result(app: tauri::AppHandle, window: WebviewWindow, session_id: String, generation: u64, latex: String, format: OutputFormat) -> AppResult<()> {
    let state = app.state::<AppState>();
    state.authorize_workbench(window.label())?;
    let _operation = state.operations.try_lock().map_err(|_| busy())?;
    let output = format::export(&latex, format)?;
    {
        let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
        if !matches!(r.phase, Phase::Ready | Phase::Error) { return Err(busy()); }
        let session = r.session.as_mut().filter(|s| s.id == session_id && s.generation == generation).ok_or_else(stale_window)?;
        session.latex = latex;
        session.format = format;
        session.cancel.cancel();
        r.phase = Phase::Copying;
    }
    let copied = tauri::async_runtime::spawn_blocking(move || {
        arboard::Clipboard::new().and_then(|mut c| c.set_text(output))
            .map_err(|_| AppError::new("clipboard", "剪贴板写入失败，结果已保留，请关闭占用剪贴板的程序后重试。", true))
    }).await.map_err(|_| AppError::internal())?;
    match copied {
        Ok(()) => cleanup(&app, Some(window.label()), true),
        Err(error) => {
            let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
            r.phase = Phase::Ready;r.error = Some(error.clone());
            drop(r);emit_state(&app);Err(error)
        }
    }
}

#[tauri::command]
pub async fn open_settings(app: tauri::AppHandle, window: WebviewWindow, latex: Option<String>, format: Option<OutputFormat>) -> AppResult<()> {
    let state = app.state::<AppState>();
    state.authorize_workbench(window.label())?;
    {
        let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
        if let Some(session) = r.session.as_mut() {
            if let Some(latex) = latex { if latex.chars().count() <= format::MAX_LATEX_CHARS { session.latex = latex; } }
            if let Some(format) = format { session.format = format; }
        }
    }
    open_workbench(&app, Page::Settings)
}

#[tauri::command]
pub async fn back_to_result(app: tauri::AppHandle, window: WebviewWindow) -> AppResult<()> {
    let state = app.state::<AppState>();
    state.authorize_workbench(window.label())?;
    let has_session = state.runtime.lock().map_err(|_| AppError::internal())?.session.is_some();
    if has_session { open_workbench(&app, Page::Result) }
    else { cleanup(&app, Some(window.label()), true) }
}

#[tauri::command]
pub fn get_settings(app: tauri::AppHandle, window: WebviewWindow) -> AppResult<SettingsView> {
    let state = app.state::<AppState>();state.authorize_workbench(window.label())?;
    let view = state.settings.lock().map_err(|_| AppError::internal())?.view();Ok(view)
}

#[tauri::command]
pub async fn save_settings(app: tauri::AppHandle, window: WebviewWindow, input: SettingsInput) -> AppResult<SettingsView> {
    let state = app.state::<AppState>();state.authorize_workbench(window.label())?;
    let _operation = state.operations.lock().await;
    let old_shortcut = state.settings.lock().map_err(|_| AppError::internal())?.value.shortcut.clone();
    let shortcut_changed = old_shortcut != input.shortcut.trim();
    if shortcut_changed { register_shortcut(&app, input.shortcut.trim())?; }
    let new_shortcut = input.shortcut.trim().to_owned();
    let result = state.settings.lock().map_err(|_| AppError::internal())?.save(input);
    if shortcut_changed {
        let to_remove = if result.is_ok() { old_shortcut } else { new_shortcut };
        if let Ok(shortcut) = to_remove.parse::<Shortcut>() { let _ = app.global_shortcut().unregister(shortcut); }
    }
    if result.is_ok() {
        state.runtime.lock().map_err(|_| AppError::internal())?.notice = Some("设置已保存。按截图快捷键即可开始。".into());
        emit_state(&app);
    }
    result
}

#[tauri::command]
pub fn set_default_format(app: tauri::AppHandle, window: WebviewWindow, format: OutputFormat) -> AppResult<()> {
    let state = app.state::<AppState>();state.authorize_workbench(window.label())?;
    state.settings.lock().map_err(|_| AppError::internal())?.set_default(format)?;
    if let Some(session) = state.runtime.lock().map_err(|_| AppError::internal())?.session.as_mut() { session.format = format; }
    emit_state(&app);Ok(())
}

#[tauri::command]
pub async fn test_connection(app: tauri::AppHandle, window: WebviewWindow, input: SettingsInput) -> AppResult<Recognition> {
    let state = app.state::<AppState>();state.authorize_workbench(window.label())?;
    let key = if let Some(key) = input.api_key.as_ref().filter(|s| !s.trim().is_empty()) {
        Zeroizing::new(key.clone())
    } else {
        let store = state.settings.lock().map_err(|_| AppError::internal())?;
        if crate::settings::normalize_base_url(&input.base_url)? != store.value.base_url {
            return Err(AppError::new("api_key", "请填写当前接口的 API Key。", false));
        }
        store.secret()?.ok_or_else(|| AppError::new("api_key", "请填写 API Key。", false))?
    };
    let cancel = CancellationToken::new();
    {
        let mut r = state.runtime.lock().map_err(|_| AppError::internal())?;
        if let Some(previous) = r.test_cancel.replace(cancel.clone()) { previous.cancel(); }
        if r.phase == Phase::Recognizing {
            if let Some(session) = r.session.as_ref() { session.cancel.cancel(); }
            r.phase = Phase::Error;
            r.error = Some(AppError::new("cancelled", "识别已暂停，完成设置后可以重新识别。", true));
        }
    }
    let result = vision::recognize(&input.base_url, &input.model, &key, include_bytes!("../assets/connection-test.png"), cancel).await;
    Ok(result?)
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "diagnostics")]
    let smoke = crate::diagnostics::configuration()?;
    let directory = app.path().app_data_dir()?;
    #[cfg(feature = "diagnostics")]
    let directory = smoke.as_ref().map(|s| s.output.join("profile")).unwrap_or(directory);
    crate::diagnostic_log::init(&directory.join("logs"));
    crate::diagnostic_log::record("startup", "ok", None);
    let settings = SettingsStore::load(&directory);
    #[cfg(feature = "diagnostics")]
    let settings = if let Some(smoke) = &smoke { crate::diagnostics::prepare_store(settings, smoke)? } else { settings };
    let initial_setup = settings.value.base_url.is_empty() || settings.value.model.is_empty();
    let shortcut = settings.value.shortcut.clone();
    app.manage(AppState { runtime: Mutex::new(Runtime::default()), settings: Mutex::new(settings), operations: tokio::sync::Mutex::new(()) });
    let handle = app.handle().clone();
    handle.plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(|app, _, event| {
        if event.state == ShortcutState::Pressed {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { let _ = begin_capture_impl(app).await; });
        }
    }).build())?;
    if let Err(error) = register_shortcut(&handle, &shortcut) {
        handle.state::<AppState>().runtime.lock().map_err(|_| AppError::internal())?.error = Some(error);
    }
    use tauri::{menu::{Menu, MenuItem}, tray::TrayIconBuilder};
    let capture = MenuItem::with_id(app, "capture", "截图识别公式", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 My Snip", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&capture, &settings, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main").tooltip("My Snip · 截图识别公式").menu(&menu).show_menu_on_left_click(true);
    if let Some(icon) = app.default_window_icon() { tray = tray.icon(icon.clone()); }
    tray.on_menu_event(|app, event| match event.id.as_ref() {
        "capture" => { let app = app.clone();tauri::async_runtime::spawn(async move { let _ = begin_capture_impl(app).await; }); }
        "settings" => { let app = app.clone();tauri::async_runtime::spawn(async move { let _ = show_settings(app).await; }); }
        "quit" => {
            let state = app.state::<AppState>();
            if let Ok(mut r) = state.runtime.lock() { r.exiting = true; }
            let _ = cleanup(app, None, false);
            let _ = app.global_shortcut().unregister_all();
            app.exit(0);
        }
        _ => (),
    }).build(app)?;
    #[cfg(feature = "diagnostics")]
    if let Some(smoke) = smoke {
        let app = handle.clone();
        tauri::async_runtime::spawn(async move { crate::diagnostics::run(app, smoke).await; });
        return Ok(());
    }
    if initial_setup || std::env::args().any(|a| a == "--settings") {
        let app = handle.clone();
        tauri::async_runtime::spawn(async move { let _ = show_settings(app).await; });
    }
    Ok(())
}

pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let app = window.app_handle().clone();let label = window.label().to_owned();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();let _operation = state.operations.lock().await;
            let _ = cleanup(&app, Some(&label), true);
        });
    }
}
