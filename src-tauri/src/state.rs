use crate::{capture::{FrozenCapture, MonitorBounds}, error::{AppError, AppResult}, format::OutputFormat, platform::Origin, settings::{SettingsStore, SettingsView}};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub const STATE_EVENT: &str = "snip-state";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Page { Result, Settings, Capture }

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Phase { Dormant, Selecting, Recognizing, Ready, Copying, Error }

pub struct Session {
    pub id: String,
    pub generation: u64,
    pub image: Arc<Vec<u8>>,
    pub image_token: String,
    pub width: u32,
    pub height: u32,
    pub latex: String,
    pub format: OutputFormat,
    pub request_id: Option<String>,
    pub cancel: CancellationToken,
    pub elapsed_ms: Option<u64>,
}

pub struct Runtime {
    pub generation: u64,
    pub window_generation: u64,
    pub window_label: Option<String>,
    pub page: Page,
    pub phase: Phase,
    pub session: Option<Session>,
    pub capture: Option<FrozenCapture>,
    pub origin: Origin,
    pub error: Option<AppError>,
    pub notice: Option<String>,
    pub preferred_monitor: Option<MonitorBounds>,
    pub test_cancel: Option<CancellationToken>,
    pub exiting: bool,
}

impl Default for Runtime {
    fn default() -> Self {
        Self { generation: 0, window_generation: 0, window_label: None, page: Page::Settings, phase: Phase::Dormant,
            session: None, capture: None, origin: Origin::default(), error: None, notice: None, preferred_monitor: None,
            test_cancel: None, exiting: false }
    }
}

pub struct AppState {
    pub runtime: Mutex<Runtime>,
    pub settings: Mutex<SettingsStore>,
    pub operations: tokio::sync::Mutex<()>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureView {
    pub generation: u64,
    pub image_token: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: String,
    pub generation: u64,
    pub image_token: String,
    pub width: u32,
    pub height: u32,
    pub latex: String,
    pub format: OutputFormat,
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewState {
    pub window_generation: u64,
    pub page: Page,
    pub phase: Phase,
    pub session: Option<SessionView>,
    pub capture: Option<CaptureView>,
    pub settings: SettingsView,
    pub error: Option<AppError>,
    pub notice: Option<String>,
    pub platform: String,
}

impl AppState {
    pub fn snapshot(&self, label: &str) -> AppResult<ViewState> {
        let r = self.runtime.lock().map_err(|_| AppError::internal())?;
        if r.window_label.as_deref() != Some(label) { return Err(stale_window()); }
        let session = r.session.as_ref().map(|s| SessionView { id: s.id.clone(), generation: s.generation, image_token: s.image_token.clone(),
            width: s.width, height: s.height, latex: s.latex.clone(), format: s.format, elapsed_ms: s.elapsed_ms });
        let capture = r.capture.as_ref().map(|c| CaptureView { generation: c.generation, image_token: c.image_token.clone(), width: c.image.width(), height: c.image.height() });
        let (window_generation, page, phase, error, notice) = (r.window_generation, r.page, r.phase, r.error.clone(), r.notice.clone());
        drop(r);
        let settings = self.settings.lock().map_err(|_| AppError::internal())?.view();
        Ok(ViewState { window_generation, page, phase, session, capture, settings, error, notice,
            platform: if cfg!(target_os = "macos") { "macos" } else { "windows" }.into() })
    }

    pub fn authorize_workbench(&self, label: &str) -> AppResult<()> {
        let r = self.runtime.lock().map_err(|_| AppError::internal())?;
        if r.window_label.as_deref() != Some(label) || !label.starts_with("workbench-") { return Err(stale_window()); }
        Ok(())
    }
}

pub fn stale_window() -> AppError { AppError::new("stale_window", "窗口已更新，请在当前窗口中继续。", false) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_and_overlay_windows_cannot_use_settings_or_results() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState { runtime: Mutex::new(Runtime::default()), settings: Mutex::new(SettingsStore::load(dir.path())), operations: tokio::sync::Mutex::new(()) };
        state.runtime.lock().unwrap().window_label = Some("workbench-new".into());
        assert!(state.authorize_workbench("workbench-new").is_ok());
        assert!(state.authorize_workbench("workbench-old").is_err());
        assert!(state.authorize_workbench("capture-new").is_err());
    }
}
