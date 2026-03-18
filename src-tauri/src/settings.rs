use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, Shortcut};

pub const DEFAULT_HOTKEY: &str = "Ctrl+H";
const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedSettings {
    hotkey: String,
}

pub fn load_hotkey(app: &AppHandle) -> String {
    read_settings(app)
        .and_then(|settings| normalize_hotkey(&settings.hotkey).ok())
        .unwrap_or_else(|| DEFAULT_HOTKEY.to_string())
}

pub fn save_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create settings directory: {err}"))?;
    }

    let contents = serde_json::to_vec_pretty(&PersistedSettings {
        hotkey: hotkey.to_string(),
    })
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

fn read_settings(app: &AppHandle) -> Option<PersistedSettings> {
    let path = settings_path(app).ok()?;
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("Failed to resolve app config directory: {err}"))?;
    Ok(config_dir.join(SETTINGS_FILE_NAME))
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
