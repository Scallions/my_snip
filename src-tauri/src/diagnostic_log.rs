//! Bounded metadata only. Never accepts images, URLs, formulas, or response bodies.
use std::{fs::{self, OpenOptions}, io::Write, path::PathBuf, sync::{Mutex, OnceLock}, time::{SystemTime, UNIX_EPOCH}};

static PATH: OnceLock<Mutex<PathBuf>> = OnceLock::new();

pub fn init(directory: &std::path::Path) {
    if fs::create_dir_all(directory).is_ok() {
        let _ = PATH.set(Mutex::new(directory.join("diagnostics.jsonl")));
    }
}

pub fn record(stage: &'static str, code: &str, elapsed_ms: Option<u64>) {
    if !code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') || code.len() > 64 { return; }
    let Some(path) = PATH.get().and_then(|p| p.lock().ok()) else { return; };
    if fs::metadata(&*path).is_ok_and(|m| m.len() >= 256 * 1024) {
        let previous = path.with_extension("previous.jsonl");
        // Two bounded files only; these contain our own diagnostic metadata.
        let _ = fs::remove_file(&previous);
        let _ = fs::rename(&*path, previous);
    }
    let entry = serde_json::json!({
        "time": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        "stage": stage, "code": code, "elapsedMs": elapsed_ms
    });
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&*path) {
        let _ = writeln!(file, "{entry}");
    }
}
