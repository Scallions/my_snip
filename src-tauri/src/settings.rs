use crate::{error::{AppError, AppResult}, format::OutputFormat};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::{Path, PathBuf}, sync::Mutex};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub schema_version: u32,
    pub base_url: String,
    pub model: String,
    pub default_format: OutputFormat,
    pub shortcut: String,
    pub revision: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self { schema_version: 1, base_url: String::new(), model: String::new(), default_format: OutputFormat::Latex,
            shortcut: if cfg!(target_os = "macos") { "Command+Shift+M" } else { "Ctrl+Shift+M" }.into(), revision: 0 }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    #[serde(flatten)]
    pub settings: Settings,
    pub has_api_key: bool,
    pub warning: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub shortcut: String,
    pub default_format: OutputFormat,
}

pub struct SettingsStore {
    pub path: PathBuf,
    pub value: Settings,
    pub warning: Option<String>,
    credential_lock: Mutex<()>,
    credential_service: String,
}

impl SettingsStore {
    pub fn load(directory: &Path) -> Self {
        let path = directory.join("settings.json");
        let (value, warning) = if path.exists() {
            match fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Settings>(&b).ok()) {
                Some(value) if value.schema_version == 1 => (value, None),
                _ => (Settings::default(), Some("原有设置无法读取。已使用临时默认值，保存新设置前会保留原文件备份。".into())),
            }
        } else { (Settings::default(), None) };
        Self { path, value, warning, credential_lock: Mutex::new(()), credential_service: "com.mysnip.desktop".into() }
    }

    fn entry(&self) -> AppResult<keyring::Entry> {
        keyring::Entry::new(&self.credential_service, "default-api")
            .map_err(|_| credential_error())
    }

    #[cfg(feature = "diagnostics")]
    pub fn isolate_diagnostics(&mut self, service: String) {
        assert!(service.starts_with("com.mysnip.test."));
        self.credential_service = service;
    }

    #[cfg(feature = "diagnostics")]
    pub fn remove_diagnostics_secret(&self) {
        if self.credential_service.starts_with("com.mysnip.test.") {
            if let Ok(entry) = self.entry() { let _ = entry.delete_credential(); }
        }
    }

    pub fn secret(&self) -> AppResult<Option<Zeroizing<String>>> {
        let _guard = self.credential_lock.lock().map_err(|_| AppError::internal())?;
        match self.entry()?.get_password() {
            Ok(value) if !value.trim().is_empty() => Ok(Some(Zeroizing::new(value))),
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(credential_error()),
        }
    }

    pub fn view(&self) -> SettingsView {
        let (has_api_key, credential_warning) = match self.secret() {
            Ok(secret) => (secret.is_some(), None),
            Err(_) => (false, Some("系统凭据暂时不可用，请检查系统授权后重试。".into())),
        };
        SettingsView { settings: self.value.clone(), has_api_key, warning: self.warning.clone().or(credential_warning) }
    }

    pub fn save(&mut self, input: SettingsInput) -> AppResult<SettingsView> {
        let base_url = normalize_base_url(&input.base_url)?;
        let model = validate_model(&input.model)?;
        if input.shortcut.trim().is_empty() || input.shortcut.len() > 100 {
            return Err(AppError::new("shortcut", "请填写有效的截图快捷键。", false));
        }
        let new_secret = input.api_key.as_deref().map(str::trim).filter(|s| !s.is_empty());
        if new_secret.is_some_and(|s| s.len() > 4096 || s.chars().any(char::is_control)) {
            return Err(AppError::new("api_key", "API Key 格式无效，请重新填写。", false));
        }
        if !self.value.base_url.is_empty() && base_url != self.value.base_url && new_secret.is_none() {
            return Err(AppError::new("api_key", "更改接口地址时，请重新填写该接口的 API Key。", false));
        }
        let previous_secret = self.secret()?;
        if new_secret.is_none() && previous_secret.is_none() {
            return Err(AppError::new("api_key", "请填写 API Key。密钥仅保存到系统凭据存储。", false));
        }
        let next = Settings { schema_version: 1, base_url, model, shortcut: input.shortcut.trim().to_owned(),
            default_format: input.default_format, revision: self.value.revision + 1 };
        if let Some(secret) = new_secret {
            let _guard = self.credential_lock.lock().map_err(|_| AppError::internal())?;
            self.entry()?.set_password(secret).map_err(|_| credential_error())?;
        }
        if let Err(error) = self.persist(&next) {
            if new_secret.is_some() {
                if let Ok(entry) = self.entry() {
                    if let Some(secret) = previous_secret { let _ = entry.set_password(secret.as_str()); }
                    else { let _ = entry.delete_credential(); }
                }
            }
            return Err(error);
        }
        self.value = next;
        self.warning = None;
        Ok(self.view())
    }

    pub fn set_default(&mut self, format: OutputFormat) -> AppResult<()> {
        let mut next = self.value.clone();
        next.default_format = format;
        next.revision += 1;
        self.persist(&next)?;
        self.value = next;
        Ok(())
    }

    fn persist(&self, value: &Settings) -> AppResult<()> {
        fs::create_dir_all(self.path.parent().ok_or_else(AppError::internal)?)
            .map_err(|_| save_error())?;
        if self.warning.is_some() && self.path.exists() {
            fs::copy(&self.path, self.path.with_extension("json.backup")).map_err(|_| save_error())?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(value).map_err(|_| save_error())?;
        let mut file = fs::File::create(&temporary).map_err(|_| save_error())?;
        file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|_| save_error())?;
        drop(file);
        // Same-directory replacement is atomic on both supported platforms.
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows::core::PCWSTR;
            use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
            let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = self.path.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe { MoveFileExW(PCWSTR(from.as_ptr()), PCWSTR(to.as_ptr()), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) }
                .map_err(|_| save_error())?;
        }
        #[cfg(not(windows))]
        fs::rename(&temporary, &self.path).map_err(|_| save_error())?;
        Ok(())
    }
}

pub fn normalize_base_url(value: &str) -> AppResult<String> {
    let mut url = url::Url::parse(value.trim()).map_err(|_| endpoint_error())?;
    let localhost = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1" | "[::1]"));
    if (url.scheme() != "https" && !(url.scheme() == "http" && localhost))
        || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some()
        || url.host_str().is_none()
    { return Err(endpoint_error()); }
    let path = url.path().trim_end_matches('/').trim_end_matches("/chat/completions").trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

pub fn endpoint(base: &str) -> AppResult<String> {
    Ok(format!("{}/chat/completions", normalize_base_url(base)?))
}

pub fn validate_model(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(AppError::new("model", "请填写支持图片输入的模型名称。", false))
    } else { Ok(value.to_owned()) }
}

fn endpoint_error() -> AppError { AppError::new("endpoint", "请填写 HTTPS 接口地址（本机 localhost 可使用 HTTP），不要附带查询参数或账号密码。", false) }
fn credential_error() -> AppError { AppError::new("credential", "无法访问系统凭据存储，请检查系统授权后重试。", true) }
fn save_error() -> AppError { AppError::new("settings_save", "设置保存失败，原有设置已保留，请检查应用数据目录是否可写。", true) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_gateway_paths_and_rejects_credential_urls() {
        assert_eq!(endpoint("https://example.com/gateway/v1/chat/completions/").unwrap(), "https://example.com/gateway/v1/chat/completions");
        assert_eq!(endpoint("http://127.0.0.1:1234/v1").unwrap(), "http://127.0.0.1:1234/v1/chat/completions");
        for invalid in ["http://example.com/v1", "https://user:secret@example.com/v1", "https://example.com/v1?token=x", "file:///x"] {
            assert!(endpoint(invalid).is_err());
        }
    }
    #[test]
    fn default_preference_persists_without_a_model_or_secret() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SettingsStore::load(dir.path());
        store.set_default(OutputFormat::MdBlock).unwrap();
        assert_eq!(SettingsStore::load(dir.path()).value.default_format, OutputFormat::MdBlock);
        let text = fs::read_to_string(dir.path().join("settings.json")).unwrap();
        assert!(!text.contains("apiKey"));
    }
}
