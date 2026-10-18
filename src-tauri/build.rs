fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "initial_state", "window_ready", "begin_capture", "commit_selection", "cancel_session",
            "retry_recognition", "copy_result", "open_settings", "back_to_result", "get_settings",
            "save_settings", "set_default_format", "test_connection",
        ]),
    )).expect("failed to build My Snip command permissions");
}
