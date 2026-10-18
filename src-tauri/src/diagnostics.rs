//! Opt-in native acceptance harness. Not included in normal release builds.
use crate::{app, capture::Selection, error::{AppError, AppResult}, format::OutputFormat, settings::{SettingsInput, SettingsStore}, state::{AppState, Page, Phase}};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{io::{Read, Write}, net::TcpListener, path::PathBuf, sync::{Arc, atomic::{AtomicUsize, Ordering}}, time::Duration};
use tauri::Manager;

pub struct Smoke {
    pub output: PathBuf,
    endpoint: String,
    requests: Arc<AtomicUsize>,
    failures: Arc<AtomicUsize>,
    service: String,
}

pub fn credential_probe() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("--probe-fixture-credential") { return; }
    let valid = args.get(2).filter(|s| s.starts_with("com.mysnip.test.")).and_then(|service| {
        keyring::Entry::new(service, "default-api").ok()?.get_password().ok()
    }).is_some_and(|secret| secret == "public-smoke-fixture");
    std::process::exit(if valid { 0 } else { 1 });
}

struct ClipboardBackup {
    text: Option<String>,
    image: Option<arboard::ImageData<'static>>,
}
impl ClipboardBackup {
    fn read() -> Self {
        let mut clipboard = arboard::Clipboard::new().ok();
        let text = clipboard.as_mut().and_then(|c| c.get_text().ok());
        let image = if text.is_none() { clipboard.as_mut().and_then(|c| c.get_image().ok()).map(|i| i.to_owned()) } else { None };
        Self { text, image }
    }
}
impl Drop for ClipboardBackup {
    fn drop(&mut self) {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Some(text) = self.text.take() { let _ = clipboard.set_text(text); }
            else if let Some(image) = self.image.take() { let _ = clipboard.set_image(image); }
        }
    }
}

pub fn configuration() -> AppResult<Option<Smoke>> {
    let args: Vec<_> = std::env::args().collect();
    let Some(index) = args.iter().position(|a| a == "--smoke") else { return Ok(None); };
    let output = PathBuf::from(args.get(index + 1).ok_or_else(|| failure("--smoke requires an output directory"))?);
    std::fs::create_dir_all(&output).map_err(|_| failure("cannot create smoke output"))?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| failure("cannot start isolated fixture"))?;
    let endpoint = format!("http://{}/v1", listener.local_addr().map_err(|_| failure("fixture address"))?);
    let requests = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();let failed = failures.clone();let directory = output.clone();
    std::thread::spawn(move || {
        for (index, incoming) in listener.incoming().take(2).enumerate() {
            let Ok(mut stream) = incoming else { failed.fetch_add(1, Ordering::SeqCst);break; };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
            let mut bytes = Vec::new();let mut buffer = [0;8192];
            let mut split = None;let mut length = 0;
            while bytes.len() < 20 * 1024 * 1024 {
                let Ok(n) = stream.read(&mut buffer) else { break; };
                if n == 0 { break; }
                bytes.extend_from_slice(&buffer[..n]);
                if split.is_none() {
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        length = headers.lines().find_map(|l| l.strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok())).unwrap_or(0);
                        if !headers.contains("authorization: bearer public-smoke-fixture") { failed.fetch_add(1, Ordering::SeqCst); }
                        split = Some(end + 4);
                    }
                }
                if split.is_some_and(|s| bytes.len() >= s + length) { break; }
            }
            let valid = split.and_then(|s| serde_json::from_slice::<Value>(&bytes[s..]).ok()).and_then(|v| {
                if v["stream"] != false || v["model"] != "fixture-vision" { return None; }
                let data = v["messages"][0]["content"][1]["image_url"]["url"].as_str()?.strip_prefix("data:image/png;base64,")?;
                let png = STANDARD.decode(data).ok()?;
                let image = image::load_from_memory(&png).ok()?;
                if image.width() == 0 || image.height() == 0 { return None; }
                let _ = std::fs::write(directory.join(format!("request-{index}-crop.png")), &png);
                Some(())
            }).is_some();
            if !valid { failed.fetch_add(1, Ordering::SeqCst); }
            count.fetch_add(1, Ordering::SeqCst);
            let body = json!({"choices":[{"finish_reason":"stop","message":{"content":"E = mc^2"}}]}).to_string();
            let _ = write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
        }
    });
    Ok(Some(Smoke { output, endpoint, requests, failures, service: format!("com.mysnip.test.{}",uuid::Uuid::new_v4()) }))
}

pub fn prepare_store(mut store: SettingsStore, smoke: &Smoke) -> AppResult<SettingsStore> {
    store.isolate_diagnostics(smoke.service.clone());
    store.save(SettingsInput { base_url: smoke.endpoint.clone(), model: "fixture-vision".into(), api_key: Some("public-smoke-fixture".into()),
        shortcut: if cfg!(target_os="macos") { "Command+Control+Shift+F12" } else { "Ctrl+Alt+Shift+F12" }.into(), default_format: OutputFormat::Latex })?;
    Ok(store)
}

fn failure(message: &str) -> AppError { AppError::new("smoke",message,false) }
fn progress(smoke: &Smoke, stage: &str) {
    let data=json!({"pid":std::process::id(),"stage":stage});
    let _=std::fs::write(smoke.output.join("progress.json"),data.to_string());
}

async fn visible_window(app: &tauri::AppHandle) -> AppResult<tauri::WebviewWindow> {
    for _ in 0..150 {
        let label=app.state::<AppState>().runtime.lock().map_err(|_| AppError::internal())?.window_label.clone();
        if let Some(window)=label.and_then(|l|app.get_webview_window(&l)) {
            if window.is_visible().unwrap_or(false) { return Ok(window); }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(failure("the actual WebView did not reach window_ready through frontend IPC"))
}

async fn no_windows(app: &tauri::AppHandle) -> AppResult<()> {
    // WebView removal is acknowledged by the platform event loop after destroy.
    for _ in 0..100 {
        if app.webview_windows().is_empty() { return Ok(()); }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(failure("business WebView survived cleanup"))
}

#[cfg(windows)]
async fn screenshot_window(window: &tauri::WebviewWindow, path: PathBuf) -> AppResult<()> {
    let position=window.inner_position().map_err(|_|failure("window position"))?;
    let size=window.inner_size().map_err(|_|failure("window size"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let monitor=xcap::Monitor::from_point(position.x + size.width as i32/2,position.y + size.height as i32/2).map_err(|_|failure("screenshot monitor"))?;
        let mx=monitor.x().map_err(|_|failure("monitor x"))?;
        let my=monitor.y().map_err(|_|failure("monitor y"))?;
        let image=monitor.capture_image().map_err(|_|failure("native UI screenshot"))?;
        let x=(position.x-mx).max(0) as u32;let y=(position.y-my).max(0) as u32;
        let width=size.width.min(image.width().saturating_sub(x));let height=size.height.min(image.height().saturating_sub(y));
        image::imageops::crop_imm(&image,x,y,width,height).to_image().save(path).map_err(|_|failure("save UI screenshot"))
    }).await.map_err(|_|failure("capture worker"))?
}

#[cfg(not(windows))]
async fn screenshot_window(_: &tauri::WebviewWindow, _: PathBuf) -> AppResult<()> { Ok(()) }

pub async fn run(app: tauri::AppHandle, smoke: Smoke) {
    progress(&smoke,"starting");
    let result=exercise(&app,&smoke).await;
    if result.is_err() {
        for window in app.webview_windows().values() {
            let _ = window.show();
            let _ = std::fs::write(smoke.output.join("failure-url.txt"), window.url().map(|u|u.to_string()).unwrap_or_default());
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = screenshot_window(window,smoke.output.join("native-failure.png")).await;
        }
    }
    let state=app.state::<AppState>();
    if let Ok(store)=state.settings.lock() { store.remove_diagnostics_secret(); }
    let _=app::cleanup(&app,None,false);
    let report=match &result {
        Ok(value)=>json!({"passed":true,"pid":std::process::id(),"checks":value,"source":"Native Tauri app with real desktop capture and a controlled local HTTP fixture; no claim of real-model accuracy."}),
        Err(error)=>json!({"passed":false,"pid":std::process::id(),"error":error.message}),
    };
    let _=std::fs::write(smoke.output.join("report.json"),serde_json::to_vec_pretty(&report).unwrap_or_default());
    if let Ok(mut r)=state.runtime.lock() { r.exiting=true; }
    progress(&smoke,if result.is_ok(){"passed"}else{"failed"});
    app.exit(if result.is_ok(){0}else{1});
}

async fn exercise(app: &tauri::AppHandle, smoke: &Smoke) -> AppResult<Value> {
    let state=app.state::<AppState>();
    let mut probe=std::process::Command::new(std::env::current_exe().map_err(|_|failure("probe executable"))?);
    probe.args(["--probe-fixture-credential", &smoke.service]);
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        probe.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    if !probe.status().map_err(|_|failure("credential child process"))?.success() {
        return Err(failure("system credential could not be reopened in a fresh process"));
    }
    app::open_workbench(app,Page::Settings)?;
    let settings_window=visible_window(app).await?;
    progress(smoke,"settings-visible");
    tokio::time::sleep(Duration::from_millis(500)).await;
    screenshot_window(&settings_window,smoke.output.join("native-settings.png")).await?;
    app::begin_capture_impl(app.clone()).await?;
    let overlay=visible_window(app).await?;
    progress(smoke,"capture-visible");
    let (generation,width,height,viewport_width,viewport_height)={
        let r=state.runtime.lock().map_err(|_|AppError::internal())?;
        let c=r.capture.as_ref().ok_or_else(||failure("capture missing"))?;
        (c.generation,c.image.width(),c.image.height(),c.bounds.width as f64/c.bounds.scale_factor,c.bounds.height as f64/c.bounds.scale_factor)
    };
    let selection=Selection{x:40.0,y:40.0,width:400.0_f64.min(viewport_width-80.0),height:140.0_f64.min(viewport_height-80.0),viewport_width,viewport_height};
    app::commit_selection(app.clone(),overlay,generation,selection).await?;
    for _ in 0..150 {
        if state.runtime.lock().map_err(|_|AppError::internal())?.phase==Phase::Ready {break;}
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let workbench=visible_window(app).await?;
    let session={
        let r=state.runtime.lock().map_err(|_|AppError::internal())?;
        if r.phase!=Phase::Ready {return Err(failure("recognition failed to complete"));}
        r.session.as_ref().map(|s|(s.id.clone(),s.generation,s.latex.clone())).ok_or_else(||failure("result missing"))?
    };
    if session.2!="E = mc^2" {return Err(failure("fixture response not reflected in native session"));}
    progress(smoke,"result-visible");
    tokio::time::sleep(Duration::from_secs(5)).await;
    screenshot_window(&workbench,smoke.output.join("native-result-light.png")).await?;
    workbench.set_theme(Some(tauri::Theme::Dark)).map_err(|_|failure("theme"))?;
    tokio::time::sleep(Duration::from_millis(600)).await;
    screenshot_window(&workbench,smoke.output.join("native-result-dark.png")).await?;
    let input=SettingsInput{base_url:smoke.endpoint.clone(),model:"fixture-vision".into(),api_key:None,
        shortcut:state.settings.lock().map_err(|_|AppError::internal())?.value.shortcut.clone(),default_format:OutputFormat::Latex};
    let connection=app::test_connection(app.clone(),workbench.clone(),input).await?;
    if connection.latex!="E = mc^2" {return Err(failure("connection test did not recognize sample"));}
    app::set_default_format(app.clone(),workbench.clone(),OutputFormat::MdInline)?;
    let before=ClipboardBackup::read();
    app::copy_result(app.clone(),workbench,session.0,session.1,"x^2 + y^2".into(),OutputFormat::MdBlock).await?;
    let copied=arboard::Clipboard::new().map_err(|_|failure("clipboard init"))?.get_text().map_err(|_|failure("clipboard readback"))?;
    if copied!="$$\nx^2 + y^2\n$$" {return Err(failure("copy did not use latest edited snapshot and selected format"));}
    drop(before);
    no_windows(app).await?;
    let persisted=SettingsStore::load(&smoke.output.join("profile"));
    if persisted.value.default_format!=OutputFormat::MdInline {return Err(failure("temporary copy format overwrote persistent default"));}
    progress(smoke,"repeating-capture-cleanup");
    for i in 0..50 {
        app::begin_capture_impl(app.clone()).await?;
        let _ = visible_window(app).await?;
        let label=state.runtime.lock().map_err(|_|AppError::internal())?.window_label.clone().ok_or_else(||failure("overlay not created"))?;
        app::cleanup(app,Some(&label),false)?;
        no_windows(app).await?;
        if i%10==0 {progress(smoke,&format!("capture-cycle-{}",i+1));}
    }
    progress(smoke,"idle-after-50-cycles");
    tokio::time::sleep(Duration::from_secs(30)).await;
    progress(smoke,"idle-30-seconds");
    tokio::time::sleep(Duration::from_secs(45)).await;
    tokio::time::sleep(Duration::from_secs(45)).await;
    progress(smoke,"idle-120-seconds");
    tokio::time::sleep(Duration::from_secs(3)).await;
    if smoke.requests.load(Ordering::SeqCst)!=2 || smoke.failures.load(Ordering::SeqCst)!=0 {return Err(failure("HTTP fixture contract validation failed"));}
    Ok(json!({"frontendBootedThroughRealIpc":true,"credentialReopenedInFreshProcess":true,"nativeCapturePixels":[width,height],"httpImageRequests":2,
        "connectionSampleReturned":true,"latestEditedClipboardVerified":true,"defaultFormatPersisted":true,"captureCleanupCycles":50,
        "remainingBusinessWebviews":app.webview_windows().len(),"idleSampleSeconds":120}))
}
