use serde::Deserialize;
use serde_json::json;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};

use crate::{clipboard, debug_log, settings, state};

const SIDE_CAR_STARTUP_WATCHDOG_SECS: u64 = 300;
const SIDE_CAR_SHUTDOWN_GRACE_PERIOD_MS: u64 = 2_000;
const SIDE_CAR_FORCE_KILL_TIMEOUT_MS: u64 = 5_000;

#[derive(Default)]
pub struct SidecarRuntime {
    stdin: Mutex<Option<ChildStdin>>,
    pid: Mutex<Option<u32>>,
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
    let sidecar_root = normalize_for_windows(sidecar_root(app)?);
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
    let pid = child.id();
    debug_log::append(format!("sidecar pid={pid}"));
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
        .pid
        .lock()
        .expect("sidecar runtime poisoned") = Some(pid);
    *app.state::<SidecarRuntime>()
        .shutdown_requested
        .lock()
        .expect("sidecar runtime poisoned") = false;

    spawn_stdout_thread(app.clone(), stdout, generation);
    spawn_stderr_thread(stderr);
    spawn_wait_thread(app.clone(), child, generation);
    spawn_startup_watchdog(app.clone(), generation);

    Ok(())
}

pub fn restart_sidecar(app: &AppHandle) -> Result<(), String> {
    stop_sidecar(
        app,
        Duration::from_millis(SIDE_CAR_SHUTDOWN_GRACE_PERIOD_MS),
    )?;

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

pub fn stop_sidecar(app: &AppHandle, graceful_timeout: Duration) -> Result<(), String> {
    let Some(pid) = current_pid(app) else {
        clear_runtime_handles(app);
        return Ok(());
    };

    let shutdown_result = request_shutdown(app);
    if let Err(err) = &shutdown_result {
        debug_log::append(format!(
            "sidecar pid={pid} shutdown request failed; continuing with exit wait: {err}"
        ));
    }

    if wait_for_process_exit(pid, graceful_timeout)? {
        clear_runtime_handles(app);
        return Ok(());
    }

    debug_log::append(format!(
        "sidecar pid={pid} exceeded graceful shutdown timeout; forcing termination"
    ));
    terminate_process_by_pid(pid)?;

    if !wait_for_process_exit(pid, Duration::from_millis(SIDE_CAR_FORCE_KILL_TIMEOUT_MS))? {
        return Err(format!(
            "Timed out waiting for sidecar pid={pid} to terminate after kill."
        ));
    }

    clear_runtime_handles(app);
    Ok(())
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

fn normalize_for_windows(path: PathBuf) -> PathBuf {
    if cfg!(windows) {
        let value = path.to_string_lossy();
        if value.starts_with(r"\\?\") {
            return PathBuf::from(value.trim_start_matches(r"\\?\"));
        }
    }

    path
}

fn build_sidecar_command(sidecar_root: &Path) -> Command {
    if let Ok(uv_path) = env::var("WHISPER_WINDOWS_UV") {
        let mut command = Command::new(normalize_for_windows(PathBuf::from(uv_path)));
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
        .map(normalize_for_windows)
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
        normalize_for_windows(sidecar_root.join("site-packages")),
        normalize_for_windows(
            sidecar_root
                .join("python")
                .join("Lib")
                .join("site-packages"),
        ),
        normalize_for_windows(
            sidecar_root
                .join("python")
                .join("lib")
                .join("python3.12")
                .join("site-packages"),
        ),
    ];

    candidates.into_iter().find(|candidate| candidate.exists())
}

fn merged_python_path(sidecar_root: &Path) -> Result<std::ffi::OsString, String> {
    let mut entries = vec![normalize_for_windows(sidecar_root.join("src"))];

    if let Some(site_packages) = bundled_site_packages(sidecar_root) {
        entries.push(normalize_for_windows(site_packages));
    }

    if let Some(existing) = env::var_os("PYTHONPATH") {
        for path in env::split_paths(&existing) {
            entries.push(normalize_for_windows(path));
        }
    }

    env::join_paths(entries).map_err(|err| format!("Failed to construct PYTHONPATH: {err}"))
}

fn apply_bundled_python_env(command: &mut Command, sidecar_root: &Path) -> Result<(), String> {
    let Some(python_root) =
        bundled_python(sidecar_root).and_then(|python| python.parent().map(PathBuf::from))
    else {
        return Ok(());
    };
    let python_root = normalize_for_windows(python_root);

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
        let phase = state::snapshot(&app).phase;
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
        if let Ok(mut guard) = app.state::<SidecarRuntime>().pid.lock() {
            if is_current {
                guard.take();
            }
        }

        match status {
            Ok(status) if !status.success() && !shutdown_requested && is_current => {
                let mut message = format!("Sidecar exited with status {status}.");
                if matches!(
                    phase,
                    state::AppPhase::Starting
                        | state::AppPhase::DownloadingModel
                        | state::AppPhase::LoadingModel
                ) {
                    message = format!(
                        "Sidecar exited while starting the runtime ({:?}) before reporting ready. Check sidecar logs for Qwen model load details.",
                        phase
                    );
                }
                debug_log::append(format!("sidecar exited with {status}"));
                let _ = state::set_error(&app, message);
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

fn spawn_startup_watchdog(app: AppHandle, generation: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(SIDE_CAR_STARTUP_WATCHDOG_SECS));

        if !is_current_generation(&app, generation) {
            return;
        }

        let snapshot = state::snapshot(&app);
        if matches!(
            snapshot.phase,
            state::AppPhase::Starting
                | state::AppPhase::DownloadingModel
                | state::AppPhase::LoadingModel
        ) {
            let _ = state::set_error(
                &app,
                format!(
                    "Sidecar startup timeout ({SIDE_CAR_STARTUP_WATCHDOG_SECS}s) while loading runtime. Check logs and try restarting."
                ),
            );
        }
    });
}

fn current_pid(app: &AppHandle) -> Option<u32> {
    app.state::<SidecarRuntime>()
        .pid
        .lock()
        .ok()
        .and_then(|guard| *guard)
}

fn clear_runtime_handles(app: &AppHandle) {
    if let Ok(mut guard) = app.state::<SidecarRuntime>().stdin.lock() {
        guard.take();
    }
    if let Ok(mut guard) = app.state::<SidecarRuntime>().pid.lock() {
        guard.take();
    }
}

fn wait_for_process_exit(pid: u32, timeout: Duration) -> Result<bool, String> {
    unsafe {
        let handle = match OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            false,
            pid,
        ) {
            Ok(handle) => handle,
            Err(_) => return Ok(true),
        };
        let wait_result = WaitForSingleObject(handle, wait_timeout(timeout));
        let _ = CloseHandle(handle);

        if wait_result == WAIT_OBJECT_0 {
            return Ok(true);
        }
        if wait_result == WAIT_TIMEOUT {
            return Ok(false);
        }

        Err(format!(
            "Unexpected wait status {wait_result:?} while waiting for sidecar pid={pid}."
        ))
    }
}

fn terminate_process_by_pid(pid: u32) -> Result<(), String> {
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid)
            .map_err(|err| format!("Failed to open sidecar pid={pid} for termination: {err}"))?;
        let terminate_result = TerminateProcess(handle, 1)
            .map_err(|err| format!("Failed to terminate sidecar pid={pid}: {err}"));
        let _ = CloseHandle(handle);
        terminate_result
    }
}

fn wait_timeout(timeout: Duration) -> u32 {
    timeout.as_millis().min(u32::MAX as u128) as u32
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

#[cfg(test)]
mod tests {
    use super::{terminate_process_by_pid, wait_for_process_exit};
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn wait_for_process_exit_detects_finished_child() {
        let mut child = Command::new("cmd")
            .args(["/C", "exit", "0"])
            .spawn()
            .expect("spawn child");
        let pid = child.id();
        let _ = child.wait().expect("wait child");

        assert!(
            wait_for_process_exit(pid, Duration::from_millis(250)).unwrap(),
            "expected finished child to report as exited"
        );
    }

    #[test]
    fn terminate_process_by_pid_stops_running_child() {
        let mut child = Command::new("cmd")
            .args(["/C", "ping 127.0.0.1 -n 6 >NUL"])
            .spawn()
            .expect("spawn long-lived child");
        let pid = child.id();

        assert!(
            !wait_for_process_exit(pid, Duration::from_millis(100)).unwrap(),
            "expected long-lived child to still be running"
        );
        terminate_process_by_pid(pid).expect("terminate child");
        let _ = child.wait().expect("wait terminated child");
        assert!(
            wait_for_process_exit(pid, Duration::from_millis(250)).unwrap(),
            "expected terminated child to exit promptly"
        );
    }
}
