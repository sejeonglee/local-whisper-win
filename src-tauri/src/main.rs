#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod debug_log;
mod sidecar;
mod state;
mod tray;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[tauri::command]
fn get_app_state(app: AppHandle) -> state::AppSnapshot {
    state::snapshot(&app)
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
        .invoke_handler(tauri::generate_handler![get_app_state])
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
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL), Code::KeyH);
            let handle = app.handle().clone();
            debug_log::append("startup begin");

            app.global_shortcut().register(shortcut)?;
            debug_log::append("startup registered shortcut");

            if let Err(err) = state::broadcast(&handle) {
                return Err(std::io::Error::other(err).into());
            }
            debug_log::append("startup broadcasted initial state");
            if let Err(err) = tray::setup(&handle) {
                return Err(std::io::Error::other(err).into());
            }
            debug_log::append("startup tray ready");
            if let Err(err) = sidecar::spawn_sidecar(&handle) {
                return Err(std::io::Error::other(err).into());
            }
            debug_log::append("startup sidecar spawned");
            if let Err(err) = tray::show_overlay(&handle) {
                return Err(std::io::Error::other(err).into());
            }
            debug_log::append("startup overlay shown");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
