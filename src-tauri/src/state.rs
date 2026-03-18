use serde::Serialize;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

use crate::sidecar::SidecarEvent;

pub const APP_STATE_CHANGED: &str = "app-state-changed";
pub const HOTKEY_LABEL: &str = "Ctrl+H";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppPhase {
    Starting,
    DownloadingModel,
    LoadingModel,
    Ready,
    ListeningRequested,
    Listening,
    Transcribing,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub model: Option<String>,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub percent: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub phase: AppPhase,
    pub hotkey: String,
    pub model: Option<String>,
    pub backend: Option<String>,
    pub message: String,
    pub last_error: Option<String>,
    pub download_progress: Option<DownloadProgress>,
    pub is_stub_bootstrap: bool,
    pub updated_at: u64,
}

impl Default for AppSnapshot {
    fn default() -> Self {
        Self {
            phase: AppPhase::Starting,
            hotkey: HOTKEY_LABEL.to_string(),
            model: None,
            backend: None,
            message: "Preparing WhisperWindows startup...".to_string(),
            last_error: None,
            download_progress: None,
            is_stub_bootstrap: true,
            updated_at: now_timestamp_ms(),
        }
    }
}

#[derive(Default)]
pub struct AppState {
    inner: Mutex<AppSnapshot>,
}

pub fn snapshot(app: &AppHandle) -> AppSnapshot {
    app.state::<AppState>()
        .inner
        .lock()
        .expect("app state poisoned")
        .clone()
}

pub fn broadcast(app: &AppHandle) -> Result<(), String> {
    app.emit(APP_STATE_CHANGED, snapshot(app))
        .map_err(|err| err.to_string())
}

pub fn set_listening_requested(app: &AppHandle) -> Result<(), String> {
    update(app, |state| {
        state.phase = AppPhase::ListeningRequested;
        state.message = "Requesting microphone capture from the sidecar...".to_string();
        state.last_error = None;
    })
}

pub fn set_transcribing_pending(app: &AppHandle) -> Result<(), String> {
    update(app, |state| {
        state.phase = AppPhase::Transcribing;
        state.message = "Stopping recording and preparing transcription...".to_string();
        state.last_error = None;
    })
}

pub fn set_error(app: &AppHandle, message: impl Into<String>) -> Result<(), String> {
    let message = message.into();
    update(app, move |state| {
        state.phase = AppPhase::Error;
        state.message = message.clone();
        state.last_error = Some(message.clone());
        state.download_progress = None;
    })
}

pub fn apply_sidecar_event(app: &AppHandle, event: &SidecarEvent) -> Result<(), String> {
    update(app, |state| match event.event.as_str() {
        "starting" => {
            state.phase = AppPhase::Starting;
            state.message = "Starting local transcription sidecar...".to_string();
            state.last_error = None;
        }
        "model_download_started" => {
            state.phase = AppPhase::DownloadingModel;
            state.model = event.model.clone();
            state.message = "Preparing first-run model cache...".to_string();
            state.download_progress = Some(DownloadProgress {
                model: event.model.clone(),
                received_bytes: 0,
                total_bytes: event.total_bytes.unwrap_or_default(),
                percent: Some(0),
            });
        }
        "model_download_progress" => {
            let received = event.received_bytes.unwrap_or_default();
            let total = event.total_bytes.unwrap_or_default();
            state.phase = AppPhase::DownloadingModel;
            state.model = event.model.clone().or_else(|| state.model.clone());
            state.download_progress = Some(DownloadProgress {
                model: event.model.clone().or_else(|| state.model.clone()),
                received_bytes: received,
                total_bytes: total,
                percent: if total > 0 {
                    Some(((received.saturating_mul(100)) / total) as u8)
                } else {
                    None
                },
            });
            state.message = "Downloading the first-run model cache...".to_string();
        }
        "loading_model" => {
            state.phase = AppPhase::LoadingModel;
            state.backend = event.backend.clone();
            state.message = "Loading the local model into the sidecar...".to_string();
        }
        "ready" => {
            state.phase = AppPhase::Ready;
            state.model = event.model.clone().or_else(|| state.model.clone());
            state.backend = event.backend.clone().or_else(|| state.backend.clone());
            state.download_progress = None;
            state.last_error = None;
            state.is_stub_bootstrap = event.bootstrap_mode.as_deref() == Some("scaffold");
            state.message = format!("Ready for dictation. Press {} to toggle.", HOTKEY_LABEL);
        }
        "listening" => {
            state.phase = AppPhase::Listening;
            state.message = "Listening for dictation input...".to_string();
        }
        "transcribing" => {
            state.phase = AppPhase::Transcribing;
            state.message = "Transcribing captured audio...".to_string();
        }
        "transcription" => {
            state.phase = AppPhase::Ready;
            state.message = if let Some(text) = event.text.as_deref() {
                format!(
                    "Captured {} characters. Paste wiring is still scaffolded.",
                    text.chars().count()
                )
            } else {
                format!("Ready for dictation. Press {} to toggle.", HOTKEY_LABEL)
            };
        }
        "empty_audio" => {
            state.phase = AppPhase::Ready;
            state.message = format!("No speech captured. Press {} to try again.", HOTKEY_LABEL);
        }
        "error" | "fatal" => {
            state.phase = AppPhase::Error;
            state.last_error = Some(
                event
                    .message
                    .clone()
                    .unwrap_or_else(|| "Unknown sidecar error".to_string()),
            );
            state.message = state
                .last_error
                .clone()
                .unwrap_or_else(|| "Unknown sidecar error".to_string());
        }
        _ => {}
    })
}

fn update(app: &AppHandle, mutate: impl FnOnce(&mut AppSnapshot)) -> Result<(), String> {
    let next_snapshot = {
        let state = app.state::<AppState>();
        let mut guard = state.inner.lock().expect("app state poisoned");
        mutate(&mut guard);
        guard.updated_at = now_timestamp_ms();
        guard.clone()
    };

    app.emit(APP_STATE_CHANGED, next_snapshot)
        .map_err(|err| err.to_string())
}

fn now_timestamp_ms() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    now.as_millis() as u64
}
