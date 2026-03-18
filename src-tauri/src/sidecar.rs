use serde::Deserialize;
use serde_json::json;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::{clipboard, debug_log, settings, state};

#[derive(Default)]
pub struct SidecarRuntime {
    stdin: Mutex<Option<ChildStdin>>,
    shutdown_requested: Mutex<bool>,
    generation: Mutex<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SidecarEvent {
    #[serde(rename = "type")]
    pub message_type: String,
    pub version: u8,
    pub event: String,
    #[serde(default)]
    pub engine: Option<String>,
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
    pub language: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub bootstrap_mode: Option<String>,
}

pub fn spawn_sidecar(app: &AppHandle) -> Result<(), String> {
    let sidecar_root = sidecar_root(app)?;
    let asr_engine = settings::load_asr_engine(app);
    let generation = next_generation(app);
    let mut command = build_sidecar_command(&sidecar_root);
    let merged_python_path = merged_python_path(&sidecar_root)?;

    command
        .current_dir(&sidecar_root)
        .env("WHISPER_WINDOWS_ASR_ENGINE", &asr_engine)
        .env("PYTHONPATH", merged_python_path)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_bundled_python_env(&mut command, &sidecar_root)?;
    debug_log::append(format!(
        "spawning sidecar generation={} engine={} in {}",
        generation,
        asr_engine,
        sidecar_root.display()
    ));

    let mut child = command
        .spawn()
        .map_err(|err| format!("Failed to start sidecar: {err}"))?;
    debug_log::append(format!("sidecar pid={}", child.id()));
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

    spawn_stdout_thread(app.clone(), stdout, generation);
    spawn_stderr_thread(stderr);
    spawn_wait_thread(app.clone(), child, generation);

    Ok(())
}

pub fn restart_sidecar(app: &AppHandle) -> Result<(), String> {
    let has_running_sidecar = app
        .state::<SidecarRuntime>()
        .stdin
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false);

    if has_running_sidecar {
        let _ = request_shutdown(app);
        std::thread::sleep(Duration::from_millis(350));
    }

    spawn_sidecar(app)
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
    debug_log::append(format!("send_command {cmd}"));
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

fn sidecar_root(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("sidecar");
        if bundled.join("src").exists() {
            debug_log::append(format!("using bundled sidecar root {}", bundled.display()));
            return Ok(bundled);
        }
    }

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sidecar");
    if workspace.join("src").exists() {
        debug_log::append(format!(
            "using workspace sidecar root {}",
            workspace.display()
        ));
        return Ok(workspace);
    }

    Err(format!(
        "Couldn't locate sidecar resources. Expected bundled or workspace sidecar near {}.",
        workspace.display()
    ))
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
        .or_else(|| bundled_python(sidecar_root))
        .or_else(|| workspace_venv_python(sidecar_root))
        .unwrap_or_else(|| PathBuf::from("python"));
    let mut command = Command::new(program);
    command.arg("-m").arg("whisper_sidecar");
    command
}

fn bundled_python(sidecar_root: &Path) -> Option<PathBuf> {
    let candidates = [
        sidecar_root.join("python").join("python.exe"),
        sidecar_root.join("python").join("bin").join("python"),
    ];

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn workspace_venv_python(sidecar_root: &Path) -> Option<PathBuf> {
    let candidates = [
        sidecar_root
            .join(".venv")
            .join("Scripts")
            .join("python.exe"),
        sidecar_root.join(".venv").join("bin").join("python"),
    ];

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn bundled_site_packages(sidecar_root: &Path) -> Option<PathBuf> {
    let candidates = [
        sidecar_root.join("site-packages"),
        sidecar_root
            .join("python")
            .join("Lib")
            .join("site-packages"),
        sidecar_root
            .join("python")
            .join("lib")
            .join("python3.12")
            .join("site-packages"),
    ];

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn merged_python_path(sidecar_root: &Path) -> Result<std::ffi::OsString, String> {
    let mut entries = vec![sidecar_root.join("src")];

    if let Some(site_packages) = bundled_site_packages(sidecar_root) {
        entries.push(site_packages);
    }

    if let Some(existing) = env::var_os("PYTHONPATH") {
        entries.extend(env::split_paths(&existing));
    }

    env::join_paths(entries).map_err(|err| format!("Failed to construct PYTHONPATH: {err}"))
}

fn apply_bundled_python_env(command: &mut Command, sidecar_root: &Path) -> Result<(), String> {
    let Some(python_root) =
        bundled_python(sidecar_root).and_then(|python| python.parent().map(PathBuf::from))
    else {
        return Ok(());
    };

    let mut path_entries = vec![python_root.clone()];
    let scripts_dir = python_root.join("Scripts");
    if scripts_dir.exists() {
        path_entries.push(scripts_dir);
    }
    if let Some(existing) = env::var_os("PATH") {
        path_entries.extend(env::split_paths(&existing));
    }

    let merged_path =
        env::join_paths(path_entries).map_err(|err| format!("Failed to construct PATH: {err}"))?;
    command.env("PYTHONHOME", &python_root);
    command.env("PATH", merged_path);
    debug_log::append(format!(
        "configured bundled python runtime {}",
        python_root.display()
    ));

    Ok(())
}

fn spawn_stdout_thread(app: AppHandle, stdout: ChildStdout, generation: u64) {
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) if !line.trim().is_empty() => {
                    if is_current_generation(&app, generation) {
                        handle_stdout_line(&app, &line);
                    } else {
                        debug_log::append(format!("ignoring stale sidecar stdout: {line}"));
                    }
                }
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
                debug_log::append(format!("sidecar stderr: {line}"));
                eprintln!("[sidecar] {line}");
            }
        }
    });
}

fn spawn_wait_thread(app: AppHandle, mut child: Child, generation: u64) {
    std::thread::spawn(move || {
        let status = child.wait();
        let is_current = is_current_generation(&app, generation);
        let shutdown_requested = app
            .state::<SidecarRuntime>()
            .shutdown_requested
            .lock()
            .map(|guard| *guard)
            .unwrap_or(false);

        if let Ok(mut guard) = app.state::<SidecarRuntime>().stdin.lock() {
            if is_current {
                guard.take();
            }
        }

        match status {
            Ok(status) if !status.success() && !shutdown_requested && is_current => {
                debug_log::append(format!("sidecar exited with {status}"));
                let _ = state::set_error(&app, format!("Sidecar exited with status {status}."));
            }
            Ok(status) => {
                debug_log::append(format!("sidecar exited cleanly with {status}"));
            }
            Err(err) if is_current => {
                debug_log::append(format!("sidecar wait failed: {err}"));
                let _ = state::set_error(&app, format!("Failed waiting for sidecar exit: {err}"));
            }
            Err(err) => {
                debug_log::append(format!("stale sidecar wait failed: {err}"));
            }
        }
    });
}

fn handle_stdout_line(app: &AppHandle, line: &str) {
    debug_log::append(format!("sidecar stdout: {line}"));
    match parse_event(line) {
        Ok(event) => {
            if event.event == "transcription" {
                if let Some(text) = event.text.as_deref() {
                    debug_log::append(format!(
                        "attempting paste of {} chars",
                        text.chars().count()
                    ));
                    if let Err(err) = clipboard::paste_transcription(text) {
                        debug_log::append(format!("paste failed: {err}"));
                        let _ = state::set_error(app, err);
                        return;
                    }
                    debug_log::append("paste succeeded");
                }
            }
            let _ = state::apply_sidecar_event(app, &event);
        }
        Err(err) => {
            debug_log::append(format!("sidecar parse error: {err}"));
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

fn next_generation(app: &AppHandle) -> u64 {
    let runtime = app.state::<SidecarRuntime>();
    let mut guard = runtime.generation.lock().expect("sidecar runtime poisoned");
    *guard += 1;
    *guard
}

fn is_current_generation(app: &AppHandle, generation: u64) -> bool {
    app.state::<SidecarRuntime>()
        .generation
        .lock()
        .map(|guard| *guard == generation)
        .unwrap_or(false)
}
