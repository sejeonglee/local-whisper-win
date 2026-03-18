use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::{sidecar, state};

const TRAY_ID: &str = "main";
const STATUS_ITEM_ID: &str = "status";
const SHOW_OVERLAY_ID: &str = "show_overlay";
const HIDE_OVERLAY_ID: &str = "hide_overlay";
const QUIT_ID: &str = "quit";

#[derive(Default)]
pub struct TrayState {
    status_item: Mutex<Option<MenuItem<Wry>>>,
}

pub fn setup(app: &AppHandle) -> Result<(), String> {
    let status_item = MenuItem::with_id(
        app,
        STATUS_ITEM_ID,
        "Starting WhisperWindows...",
        false,
        None::<&str>,
    )
    .map_err(|err| err.to_string())?;
    let show_overlay_item =
        MenuItem::with_id(app, SHOW_OVERLAY_ID, "Show overlay", true, None::<&str>)
            .map_err(|err| err.to_string())?;
    let hide_overlay_item =
        MenuItem::with_id(app, HIDE_OVERLAY_ID, "Hide overlay", true, None::<&str>)
            .map_err(|err| err.to_string())?;
    let quit_item = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)
        .map_err(|err| err.to_string())?;

    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &show_overlay_item,
            &hide_overlay_item,
            &quit_item,
        ],
    )
    .map_err(|err| err.to_string())?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("WhisperWindows")
        .show_menu_on_left_click(false);

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.on_menu_event(|app, event| match event.id().as_ref() {
        SHOW_OVERLAY_ID => {
            let _ = show_overlay(app);
        }
        HIDE_OVERLAY_ID => {
            let _ = hide_overlay(app);
        }
        QUIT_ID => {
            let _ = sidecar::request_shutdown(app);
            app.exit(0);
        }
        _ => {}
    })
    .build(app)
    .map_err(|err| err.to_string())?;

    *app.state::<TrayState>()
        .status_item
        .lock()
        .expect("tray state poisoned") = Some(status_item);

    sync(app, &state::snapshot(app))
}

pub fn sync(app: &AppHandle, snapshot: &state::AppSnapshot) -> Result<(), String> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let tooltip = format!("WhisperWindows: {}", snapshot.message);
        tray.set_tooltip(Some(tooltip))
            .map_err(|err| err.to_string())?;
    }

    if let Some(status_item) = app
        .state::<TrayState>()
        .status_item
        .lock()
        .expect("tray state poisoned")
        .as_ref()
        .cloned()
    {
        status_item
            .set_text(&format_status(snapshot))
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}

pub fn show_overlay(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main overlay window is unavailable.".to_string())?;
    window.show().map_err(|err| err.to_string())
}

pub fn hide_overlay(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main overlay window is unavailable.".to_string())?;
    window.hide().map_err(|err| err.to_string())
}

fn format_status(snapshot: &state::AppSnapshot) -> String {
    format!("Status: {}", snapshot.message)
}
