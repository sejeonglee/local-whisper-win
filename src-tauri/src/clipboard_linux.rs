use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use crate::debug_log;

static PASTE_TARGET: OnceLock<Mutex<Option<String>>> = OnceLock::new();

struct ClipboardSnapshot {
    plain_text: Option<String>,
    is_empty: bool,
}

pub fn capture_paste_target() {
    let target = capture_active_window();
    if let Some(window_id) = target {
        if let Ok(mut guard) = paste_target().lock() {
            *guard = Some(window_id.clone());
        }
        debug_log::append(format!("captured paste target window_id={window_id}"));
        return;
    }

    debug_log::append("capture_paste_target skipped: no active window detected");
}

pub fn clear_paste_target() {
    if let Ok(mut guard) = paste_target().lock() {
        *guard = None;
    }
}

pub fn paste_transcription(text: &str) -> Result<(), String> {
    let previous_clipboard = snapshot_clipboard();
    let target = load_paste_target();
    debug_log::append(format!(
        "paste_transcription start chars={} target={}",
        text.chars().count(),
        target.clone().unwrap_or_default()
    ));

    set_text_clipboard(text)?;

    let auto_paste_ok = restore_focus(target.as_deref())
        .and_then(|_| send_ctrl_v())
        .is_ok();

    if !auto_paste_ok {
        debug_log::append("Auto-paste unavailable: text remains on clipboard for manual paste.");
        return Ok(());
    }

    thread::sleep(Duration::from_millis(120));
    restore_clipboard(previous_clipboard)
}

fn paste_target() -> &'static Mutex<Option<String>> {
    PASTE_TARGET.get_or_init(|| Mutex::new(None))
}

fn load_paste_target() -> Option<String> {
    paste_target()
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn capture_active_window() -> Option<String> {
    if !command_exists("xdotool") {
        return None;
    }

    let output = run_command_output("xdotool", &["getwindowfocus"])?;
    let output = output.trim().to_string();
    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

fn snapshot_clipboard() -> ClipboardSnapshot {
    match snapshot_plain_text_clipboard() {
        Ok(plain_text) => ClipboardSnapshot {
            plain_text,
            is_empty: false,
        },
        Err(_) => ClipboardSnapshot {
            plain_text: None,
            is_empty: true,
        },
    }
}

fn restore_clipboard(previous: ClipboardSnapshot) -> Result<(), String> {
    if let Some(text) = previous.plain_text {
        return set_text_clipboard(&text);
    }

    if previous.is_empty {
        clear_clipboard()?;
    }

    Ok(())
}

fn clear_clipboard() -> Result<(), String> {
    if command_exists("wl-copy") {
        return run_command_status("wl-copy", &["--clear"], None);
    }

    if command_exists("xclip") {
        return run_command_status("xclip", &["-selection", "clipboard", "-in"], Some(""));
    }

    Ok(())
}

fn set_text_clipboard(text: &str) -> Result<(), String> {
    let has_text = !text.is_empty();
    debug_log::append(format!("set_text_clipboard {} chars", text.chars().count()));

    if command_exists("wl-copy") {
        let mut child = Command::new("wl-copy")
            .arg("--type")
            .arg("text/plain")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|err| format!("Failed to start wl-copy: {err}"))?;

        if has_text {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(text.as_bytes())
                    .map_err(|err| format!("Failed writing to wl-copy stdin: {err}"))?;
            }
        }

        let status = child
            .wait()
            .map_err(|err| format!("Failed waiting for wl-copy: {err}"))?;
        if !status.success() {
            return Err(format!("Failed to write text clipboard: status={status}"));
        }

        return Ok(());
    }

    if command_exists("xclip") {
        let mut child = Command::new("xclip")
            .args(["-selection", "clipboard", "-in"])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|err| format!("Failed to start xclip: {err}"))?;

        if has_text {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(text.as_bytes())
                    .map_err(|err| format!("Failed writing to xclip stdin: {err}"))?;
            }
        }

        let status = child
            .wait()
            .map_err(|err| format!("Failed waiting for xclip: {err}"))?;
        if !status.success() {
            return Err(format!("Failed to write text clipboard: status={status}"));
        }

        return Ok(());
    }

    Err("No clipboard utility found. Install wl-clipboard or xclip on Linux.".to_string())
}

fn restore_focus(target: Option<&str>) -> Result<(), String> {
    if let Some(window_id) = target {
        if command_exists("xdotool") {
            return run_command_status("xdotool", &["windowactivate", "--sync", window_id], None);
        }
    }

    Ok(())
}

fn send_ctrl_v() -> Result<(), String> {
    if !command_exists("xdotool") {
        return Err("xdotool is not installed. Automatic paste is unavailable.".to_string());
    }

    run_command_status("xdotool", &["key", "--clearmodifiers", "ctrl+v"], None)
}

fn snapshot_plain_text_clipboard() -> Result<Option<String>, String> {
    if command_exists("wl-paste") {
        let output = run_command_output("wl-paste", &["-n"])?;
        let text = output.trim_end_matches('\0').to_string();
        return Ok(if text.is_empty() { None } else { Some(text) });
    }

    if !command_exists("xclip") {
        return Err("No clipboard utility found to read clipboard.".to_string());
    }

    let output = run_command_output("xclip", &["-selection", "clipboard", "-o"])?;
    let text = output.trim_end_matches('\n').to_string();
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}

fn run_command_output(command: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(command)
        .args(args)
        .output()
        .map_err(|err| format!("Failed to run {command}: {err}"))?;

    if !output.status.success() {
        return Err(format!("{command} returned status {status}", status = output.status));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_command_status(
    command: &str,
    args: &[&str],
    stdin_text: Option<&str>,
) -> Result<(), String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(if stdin_text.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .spawn()
        .map_err(|err| format!("Failed to run {command}: {err}"))?;

    if let Some(text) = stdin_text {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|err| format!("Failed to write stdin for {command}: {err}"))?;
        }
    }

    let status = child
        .wait()
        .map_err(|err| format!("Failed waiting for {command}: {err}"))?;
    if !status.success() {
        return Err(format!("{command} failed with status {status}"));
    }

    Ok(())
}

fn command_exists(command: &str) -> bool {
    if cfg!(target_os = "windows") {
        let candidates = [format!("{command}.exe"), command.to_string()];
        return candidates.iter().any(|candidate| which::command_exists(candidate));
    }

    which::command_exists(command)
}

mod which {
    use std::process::Command;

    pub fn command_exists(command: &str) -> bool {
        Command::new("sh")
            .arg("-lc")
            .arg(format!("command -v {}", command))
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}
