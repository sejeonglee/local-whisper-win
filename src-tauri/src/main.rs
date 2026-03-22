#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod debug_log;
mod settings;
mod sidecar;
mod state;
mod tray;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[tauri::command]
fn get_app_state(app: AppHandle) -> state::AppSnapshot {
    state::snapshot(&app)
}

#[tauri::command]
fn set_hotkey(app: AppHandle, hotkey: String) -> Result<state::AppSnapshot, String> {
    let normalized = settings::normalize_hotkey(&hotkey)?;
    let current = state::snapshot(&app).hotkey;
    if normalized == current {
        return Ok(state::snapshot(&app));
    }

    debug_log::append(format!(
        "hotkey update requested: {current} -> {normalized}"
    ));
    app.global_shortcut()
        .unregister(current.as_str())
        .map_err(|err| format!("Failed to unregister current shortcut {current}: {err}"))?;

    if let Err(err) = app.global_shortcut().register(normalized.as_str()) {
        let _ = app.global_shortcut().register(current.as_str());
        return Err(format!("Failed to register {normalized}: {err}"));
    }

    if let Err(err) = settings::save_hotkey(&app, &normalized) {
        let _ = app.global_shortcut().unregister(normalized.as_str());
        let _ = app.global_shortcut().register(current.as_str());
        return Err(err);
    }

    if let Err(err) = state::set_hotkey_label(&app, normalized.clone()) {
        let _ = app.global_shortcut().unregister(normalized.as_str());
        let _ = app.global_shortcut().register(current.as_str());
        let _ = settings::save_hotkey(&app, &current);
        return Err(err);
    }

    debug_log::append(format!("hotkey update applied: {normalized}"));
    Ok(state::snapshot(&app))
}

#[tauri::command]
fn set_asr_engine(app: AppHandle, asr_engine: String) -> Result<state::AppSnapshot, String> {
    let normalized = settings::normalize_asr_engine(&asr_engine)?;
    let snapshot = state::snapshot(&app);
    let phase = snapshot.phase.clone();
    let current = snapshot
        .engine
        .clone()
        .unwrap_or_else(|| settings::DEFAULT_ASR_ENGINE.to_string());
    if normalized == current {
        return Ok(state::snapshot(&app));
    }

    if matches!(
        phase,
        state::AppPhase::ListeningRequested | state::AppPhase::Listening | state::AppPhase::Transcribing
    ) {
        return Err("Wait until the current dictation cycle finishes before switching the ASR engine.".to_string());
    }

    debug_log::append(format!(
        "asr engine update requested: {current} -> {normalized}"
    ));

    settings::save_asr_engine(&app, &normalized)?;
    if let Err(err) = state::set_asr_engine_label(&app, normalized.clone()) {
        let _ = settings::save_asr_engine(&app, &current);
        return Err(err);
    }

    if let Err(err) = sidecar::restart_sidecar(&app) {
        let _ = settings::save_asr_engine(&app, &current);
        let _ = state::set_asr_engine_label(&app, current.clone());
        return Err(err);
    }

    debug_log::append(format!("asr engine update applied: {normalized}"));
    Ok(state::snapshot(&app))
}

fn handle_hotkey(app: &AppHandle) {
    let phase = state::snapshot(app).phase;
    debug_log::append(format!("hotkey pressed while phase={phase:?}"));
    match phase {
        state::AppPhase::Ready => {
            debug_log::append("hotkey -> start_recording");
            clipboard::capture_paste_target();
            let _ = state::set_listening_requested(app);
            if let Err(err) = sidecar::send_command(app, "start_recording") {
                clipboard::clear_paste_target();
                let _ = state::set_error(app, err);
            }
        }
        state::AppPhase::Listening => {
            debug_log::append("hotkey -> stop_recording");
            let _ = state::set_transcribing_pending(app);
            if let Err(err) = sidecar::send_command(app, "stop_recording") {
                let _ = state::set_error(app, err);
            }
        }
        _ => {}
    }
}

fn main() {
    tauri::Builder::default()
        .manage(state::AppState::default())
        .manage(sidecar::SidecarRuntime::default())
        .manage(tray::TrayState::default())
        .invoke_handler(tauri::generate_handler![get_app_state, set_hotkey, set_asr_engine])
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        handle_hotkey(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let startup_hotkey = settings::load_hotkey(&handle);
            let startup_asr_engine = settings::load_asr_engine(&handle);
            debug_log::append("startup begin");
            state::seed_hotkey(&handle, startup_hotkey.clone());
            state::seed_asr_engine(&handle, startup_asr_engine);

            app.global_shortcut()
                .register(startup_hotkey.as_str())
                .map_err(|err| {
                    std::io::Error::other(format!("Failed to register {startup_hotkey}: {err}"))
                })?;
            debug_log::append(format!("startup registered shortcut {startup_hotkey}"));

            if let Err(err) = state::broadcast(&handle) {
                return Err(std::io::Error::other(err).into());
            }
            debug_log::append("startup broadcasted initial state");
            if let Err(err) = tray::setup(&handle) {
                return Err(std::io::Error::other(err).into());
            }
            debug_log::append("startup tray ready");
            if let Err(err) = tray::show_overlay(&handle) {
                return Err(std::io::Error::other(err).into());
            }
            debug_log::append("startup overlay shown");

            if let Err(err) = sidecar::spawn_sidecar(&handle) {
                debug_log::append(format!("startup sidecar spawn failed: {err}"));
                let _ = state::set_error(&handle, format!("Sidecar failed to start: {err}"));
            }
            debug_log::append("startup sidecar spawn attempted");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
