use serde::Deserialize;
use serde_json::json;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::{clipboard, state};

#[derive(Default)]
pub struct SidecarRuntime {
    stdin: Mutex<Option<ChildStdin>>,
    shutdown_requested: Mutex<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SidecarEvent {
    #[serde(rename = "type")]
    pub message_type: String,
    pub version: u8,
    pub event: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub received_bytes: Option<u64>,
    #[serde(default)]
    pub total_bytes: Option<u64>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub bootstrap_mode: Option<String>,
}

pub fn spawn_sidecar(app: &AppHandle) -> Result<(), String> {
    let sidecar_root = sidecar_root();
    let mut command = build_sidecar_command(&sidecar_root);
    let python_path = sidecar_root.join("src");
    let existing_python_path = env::var_os("PYTHONPATH")
        .map(PathBuf::from)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let merged_python_path = if existing_python_path.is_empty() {
        python_path.display().to_string()
    } else {
        format!("{};{}", python_path.display(), existing_python_path)
    };

    command
        .current_dir(&sidecar_root)
        .env("PYTHONPATH", merged_python_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|err| format!("Failed to start sidecar: {err}"))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Sidecar stdin is unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Sidecar stdout is unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Sidecar stderr is unavailable".to_string())?;

    *app.state::<SidecarRuntime>()
        .stdin
        .lock()
        .expect("sidecar runtime poisoned") = Some(stdin);
    *app.state::<SidecarRuntime>()
        .shutdown_requested
        .lock()
        .expect("sidecar runtime poisoned") = false;

    spawn_stdout_thread(app.clone(), stdout);
    spawn_stderr_thread(stderr);
    spawn_wait_thread(app.clone(), child);

    Ok(())
}

pub fn request_shutdown(app: &AppHandle) -> Result<(), String> {
    {
        let runtime = app.state::<SidecarRuntime>();
        let mut guard = runtime
            .shutdown_requested
            .lock()
            .expect("sidecar runtime poisoned");
        *guard = true;
    }

    send_command(app, "shutdown")
}

pub fn send_command(app: &AppHandle, cmd: &str) -> Result<(), String> {
    let payload = json!({
        "type": "command",
        "version": 1,
        "cmd": cmd,
    })
    .to_string();

    let runtime = app.state::<SidecarRuntime>();
    let mut guard = runtime.stdin.lock().expect("sidecar runtime poisoned");
    let stdin = guard
        .as_mut()
        .ok_or_else(|| "Sidecar is not available. Restart the app to try again.".to_string())?;

    stdin
        .write_all(payload.as_bytes())
        .map_err(|err| format!("Failed to write to sidecar: {err}"))?;
    stdin
        .write_all(b"\n")
        .map_err(|err| format!("Failed to terminate sidecar command: {err}"))?;
    stdin
        .flush()
        .map_err(|err| format!("Failed to flush sidecar command: {err}"))
}

fn sidecar_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sidecar")
}

fn build_sidecar_command(sidecar_root: &Path) -> Command {
    if let Ok(uv_path) = env::var("WHISPER_WINDOWS_UV") {
        let mut command = Command::new(uv_path);
        command
            .arg("run")
            .arg("--project")
            .arg(sidecar_root)
            .arg("whisper-sidecar");
        return command;
    }

    let program = env::var("WHISPER_WINDOWS_PYTHON")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| workspace_venv_python(sidecar_root))
        .unwrap_or_else(|| PathBuf::from("python"));
    let mut command = Command::new(program);
    command.arg("-m").arg("whisper_sidecar");
    command
}

fn workspace_venv_python(sidecar_root: &Path) -> Option<PathBuf> {
    let candidates = [
        sidecar_root.join(".venv").join("Scripts").join("python.exe"),
        sidecar_root.join(".venv").join("bin").join("python"),
    ];

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn spawn_stdout_thread(app: AppHandle, stdout: ChildStdout) {
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) if !line.trim().is_empty() => handle_stdout_line(&app, &line),
                Ok(_) => {}
                Err(err) => {
                    let _ = state::set_error(&app, format!("Failed reading sidecar output: {err}"));
                    break;
                }
            }
        }
    });
}

fn spawn_stderr_thread(stderr: ChildStderr) {
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                eprintln!("[sidecar] {line}");
            }
        }
    });
}

fn spawn_wait_thread(app: AppHandle, mut child: Child) {
    std::thread::spawn(move || {
        let status = child.wait();
        let shutdown_requested = app
            .state::<SidecarRuntime>()
            .shutdown_requested
            .lock()
            .map(|guard| *guard)
            .unwrap_or(false);

        if let Ok(mut guard) = app.state::<SidecarRuntime>().stdin.lock() {
            guard.take();
        }

        match status {
            Ok(status) if !status.success() && !shutdown_requested => {
                let _ = state::set_error(&app, format!("Sidecar exited with status {status}."));
            }
            Ok(_) => {}
            Err(err) => {
                let _ = state::set_error(&app, format!("Failed waiting for sidecar exit: {err}"));
            }
        }
    });
}

fn handle_stdout_line(app: &AppHandle, line: &str) {
    match parse_event(line) {
        Ok(event) => {
            if event.event == "transcription" {
                if let Some(text) = event.text.as_deref() {
                    if let Err(err) = clipboard::paste_transcription(text) {
                        let _ = state::set_error(app, err);
                        return;
                    }
                }
            }
            let _ = state::apply_sidecar_event(app, &event);
        }
        Err(err) => {
            let _ = state::set_error(app, format!("Invalid sidecar event: {err}"));
        }
    }
}

fn parse_event(line: &str) -> Result<SidecarEvent, String> {
    let event: SidecarEvent = serde_json::from_str(line).map_err(|err| err.to_string())?;
    if event.message_type != "event" {
        return Err(format!("Unexpected message type: {}", event.message_type));
    }
    if event.version != 1 {
        return Err(format!("Unsupported protocol version: {}", event.version));
    }
    Ok(event)
}
