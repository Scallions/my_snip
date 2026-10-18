mod error;
mod diagnostic_log;
mod format;
mod settings;
mod capture;
mod platform;
mod vision;
mod state;
mod app;
#[cfg(feature = "diagnostics")]
mod diagnostics;

pub fn run() {
    #[cfg(feature = "diagnostics")]
    diagnostics::credential_probe();
    use tauri::Manager;
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { let _ = crate::app::show_settings(app).await; });
        }))
        .register_uri_scheme_protocol("snip-image", |context, request| {
            let token = request.uri().path().trim_start_matches('/');
            let bytes = context.app_handle().try_state::<state::AppState>().and_then(|state| {
                let runtime = state.runtime.lock().ok()?;
                if runtime.window_label.as_deref() != Some(context.webview_label()) { return None; }
                if context.webview_label().starts_with("capture-") {
                    runtime.capture.as_ref().filter(|c| c.image_token == token).map(|c| c.png.clone())
                } else if context.webview_label().starts_with("workbench-") {
                    runtime.session.as_ref().filter(|s| s.image_token == token).map(|s| s.image.clone())
                } else { None }
            });
            let (status, bytes) = match bytes {
                Some(bytes) => (200, bytes.as_ref().clone()),
                None => (404, Vec::new()),
            };
            tauri::http::Response::builder().status(status)
                .header("Content-Type", "image/png")
                .header("Cache-Control", "no-store")
                .header("Access-Control-Allow-Origin", "*")
                .header("X-Content-Type-Options", "nosniff")
                .body(bytes).expect("static image response headers")
        })
        .setup(app::setup)
        .on_window_event(app::on_window_event)
        .invoke_handler(tauri::generate_handler![
            app::initial_state, app::window_ready, app::begin_capture, app::commit_selection,
            app::cancel_session, app::retry_recognition, app::copy_result,
            app::open_settings, app::back_to_result, app::get_settings, app::save_settings,
            app::set_default_format, app::test_connection,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start My Snip");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            let exiting = app.try_state::<state::AppState>().and_then(|state| state.runtime.lock().ok().map(|r| r.exiting)).unwrap_or(false);
            if !exiting && code.is_none() { api.prevent_exit(); }
        }
    });
}
