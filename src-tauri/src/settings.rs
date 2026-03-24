use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, Shortcut};

pub const DEFAULT_HOTKEY: &str = "Ctrl+H";
pub const DEFAULT_ASR_ENGINE: &str = "whisper";
const LOCAL_DATA_DIR_NAME: &str = "WhisperWindows";
const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedSettings {
    hotkey: String,
    #[serde(default = "default_asr_engine")]
    asr_engine: String,
}

fn default_asr_engine() -> String {
    DEFAULT_ASR_ENGINE.to_string()
}

fn default_settings() -> PersistedSettings {
    PersistedSettings {
        hotkey: DEFAULT_HOTKEY.to_string(),
        asr_engine: default_asr_engine(),
    }
}

pub fn load_hotkey(app: &AppHandle) -> String {
    read_settings(app)
        .and_then(|settings| normalize_hotkey(&settings.hotkey).ok())
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string())
}

pub fn load_asr_engine(app: &AppHandle) -> String {
    read_settings(app)
        .and_then(|settings| normalize_asr_engine(&settings.asr_engine).ok())
        .unwrap_or_else(|| DEFAULT_ASR_ENGINE.to_string())
}

pub fn save_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let mut settings = read_settings(app).unwrap_or_else(default_settings);
    settings.hotkey = hotkey.to_string();
    write_settings(app, &settings)
}

pub fn save_asr_engine(app: &AppHandle, asr_engine: &str) -> Result<(), String> {
    let mut settings = read_settings(app).unwrap_or_else(default_settings);
    settings.asr_engine = asr_engine.to_string();
    write_settings(app, &settings)
}

fn write_settings(app: &AppHandle, settings: &PersistedSettings) -> Result<(), String> {
    let path = settings_path(app)?;
    let legacy_path = legacy_settings_path(app).ok();
    write_settings_to_path(&path, settings)?;

    if legacy_path.as_ref().is_some_and(|legacy| legacy != &path) {
        cleanup_legacy_settings(legacy_path.as_ref().expect("legacy path checked"));
    }

    Ok(())
}

fn write_settings_to_path(path: &PathBuf, settings: &PersistedSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create settings directory: {err}"))?;
    }

    let contents = serde_json::to_vec_pretty(settings)
        .map_err(|err| format!("Failed to encode settings: {err}"))?;

    fs::write(path, contents).map_err(|err| format!("Failed to save settings: {err}"))
}

pub fn normalize_hotkey(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Enter a shortcut like Ctrl+Shift+H or Ctrl+Alt+Space.".to_string());
    }

    let shortcut = Shortcut::from_str(trimmed)
        .map_err(|err| format!("Invalid shortcut '{trimmed}': {err}"))?;
    if shortcut.key == Code::Escape {
        return Err(
            "Escape alone is too easy to trigger accidentally. Choose a different shortcut."
                .to_string(),
        );
    }

    Ok(format_shortcut_display(shortcut))
}

pub fn normalize_asr_engine(input: &str) -> Result<String, String> {
    match input.trim().to_ascii_lowercase().as_str() {
        "whisper" => Ok("whisper".to_string()),
        "qwen3" => Ok("qwen3".to_string()),
        _ => Err("Choose either whisper or qwen3.".to_string()),
    }
}

fn read_settings(app: &AppHandle) -> Option<PersistedSettings> {
    let primary_path = settings_path(app).ok()?;
    if let Some(settings) = read_settings_from_path(&primary_path) {
        return Some(settings);
    }

    let legacy_path = legacy_settings_path(app).ok()?;
    let settings = read_settings_from_path(&legacy_path)?;
    let _ = write_settings_to_path(&primary_path, &settings);
    cleanup_legacy_settings(&legacy_path);
    Some(settings)
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_local_data_root(app)?.join(SETTINGS_FILE_NAME))
}

fn legacy_settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("Failed to resolve app config directory: {err}"))?;
    Ok(config_dir.join(SETTINGS_FILE_NAME))
}

fn app_local_data_root(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(local_app_data).join(LOCAL_DATA_DIR_NAME));
    }

    app.path()
        .app_config_dir()
        .map_err(|err| format!("Failed to resolve fallback app data directory: {err}"))
}

fn read_settings_from_path(path: &PathBuf) -> Option<PersistedSettings> {
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn cleanup_legacy_settings(path: &PathBuf) {
    let _ = fs::remove_file(path);
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir(parent);
    }
}

fn format_shortcut_display(shortcut: Shortcut) -> String {
    let mut parts = Vec::new();

    if shortcut
        .mods
        .contains(tauri_plugin_global_shortcut::Modifiers::CONTROL)
    {
        parts.push("Ctrl".to_string());
    }
    if shortcut
        .mods
        .contains(tauri_plugin_global_shortcut::Modifiers::SHIFT)
    {
        parts.push("Shift".to_string());
    }
    if shortcut
        .mods
        .contains(tauri_plugin_global_shortcut::Modifiers::ALT)
    {
        parts.push("Alt".to_string());
    }
    if shortcut
        .mods
        .contains(tauri_plugin_global_shortcut::Modifiers::SUPER)
    {
        parts.push("Super".to_string());
    }

    parts.push(format_key_label(shortcut.key));
    parts.join("+")
}

fn format_key_label(key: Code) -> String {
    let raw = key.to_string();

    if let Some(letter) = raw.strip_prefix("Key") {
        return letter.to_uppercase();
    }
    if let Some(digit) = raw.strip_prefix("Digit") {
        return digit.to_string();
    }
    if let Some(direction) = raw.strip_prefix("Arrow") {
        return direction.to_string();
    }
    if let Some(number) = raw.strip_prefix("Numpad") {
        return format!("Num {number}");
    }
    if let Some(volume) = raw.strip_prefix("AudioVolume") {
        return format!("Volume {volume}");
    }

    raw
}

#[cfg(test)]
mod tests {
    use super::normalize_hotkey;

    #[test]
    fn normalizes_ctrl_shortcuts() {
        assert_eq!(normalize_hotkey("ctrl+h").unwrap(), "Ctrl+H");
        assert_eq!(
            normalize_hotkey("ctrl+shift+space").unwrap(),
            "Ctrl+Shift+Space"
        );
    }

    #[test]
    fn rejects_empty_shortcuts() {
        assert!(normalize_hotkey("   ").is_err());
    }
}
