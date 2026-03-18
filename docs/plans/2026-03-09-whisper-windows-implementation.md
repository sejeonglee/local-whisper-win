# Whisper Windows Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Windows desktop dictation app using Tauri v2 + React + Python sidecar (faster-whisper) that transcribes speech to text via local GPU and pastes into the active window.

**Architecture:** Tauri v2 (Rust) manages the system tray, global hotkey, and overlay window. A React frontend renders the overlay UI. A Python sidecar process runs faster-whisper on the local GPU, communicates with Tauri via stdio JSON lines, and handles audio capture, VAD, transcription, and clipboard paste.

**Tech Stack:** Tauri v2, Rust, React, TypeScript, Python 3.12, uv, mise, faster-whisper, sounddevice, Silero VAD, pnpm

---

## Phase 1: Project Scaffolding & Tooling

### Task 1: Initialize mise configuration

**Files:**
- Create: `.mise.toml`
- Create: `.gitignore`

**Step 1: Create `.mise.toml`**

```toml
[tools]
python = "3.12"
node = "22"

[env]
UV_LINK_MODE = "copy"
```

Note: Rust/cargo managed separately (rustup on Windows). Not pinned in mise for WSL2 dev.

**Step 2: Create `.gitignore`**

```gitignore
# Python
__pycache__/
*.pyc
.venv/
dist/
*.egg-info/

# Node
node_modules/
dist/

# Tauri
src-tauri/target/

# IDE
.vscode/
.idea/

# OS
.DS_Store
Thumbs.db

# Build artifacts
*.exe
*.msi
```

**Step 3: Commit**

```bash
git add .mise.toml .gitignore
git commit -m "chore: add mise config and gitignore"
```

---

### Task 2: Initialize Python sidecar project with uv

**Files:**
- Create: `sidecar/pyproject.toml`
- Create: `sidecar/.python-version`
- Create: `sidecar/src/whisper_sidecar/__init__.py`

**Step 1: Scaffold sidecar project**

```bash
cd sidecar
uv init --lib --name whisper-sidecar
```

If `uv init` creates files in unexpected locations, adjust manually. The target structure:

```
sidecar/
├── pyproject.toml
├── .python-version
└── src/
    └── whisper_sidecar/
        └── __init__.py
```

**Step 2: Edit `sidecar/pyproject.toml`**

```toml
[project]
name = "whisper-sidecar"
version = "0.1.0"
description = "Local GPU speech-to-text sidecar for Whisper Windows"
requires-python = ">=3.12"
dependencies = [
    "faster-whisper>=1.1.0",
    "sounddevice>=0.5.0",
    "numpy>=1.26.0",
    "pyperclip>=1.9.0",
]

[project.scripts]
whisper-sidecar = "whisper_sidecar.__main__:main"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[dependency-groups]
dev = [
    "pytest>=8.0",
    "pytest-asyncio>=0.23",
]
```

Note: `silero-vad` will be loaded via `torch.hub` or as an ONNX model. We add it when implementing VAD (Task 5).

**Step 3: Set Python version**

```bash
echo "3.12" > sidecar/.python-version
```

**Step 4: Install dependencies**

```bash
cd sidecar
uv sync
```

Note: This will likely fail or skip `faster-whisper` on WSL2 without CUDA. That's fine — the sidecar is tested on Windows. For WSL2 dev, we write and unit-test the non-GPU code.

**Step 5: Commit**

```bash
git add sidecar/
git commit -m "chore: initialize python sidecar project with uv"
```

---

### Task 3: Initialize Tauri v2 + React frontend

**Files:**
- Create: `package.json`
- Create: `src/` (React app)
- Create: `src-tauri/` (Tauri Rust backend)

**Step 1: Scaffold Tauri v2 project**

```bash
pnpm create tauri-app@latest . --template react-ts --manager pnpm
```

If the CLI asks questions:
- Project name: `whisper-windows`
- Frontend: React + TypeScript
- Package manager: pnpm

This creates `src/`, `src-tauri/`, `package.json`, `tsconfig.json`, etc.

**Step 2: Install frontend dependencies**

```bash
pnpm install
```

**Step 3: Verify structure exists**

Check that these exist:
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `src-tauri/src/main.rs` (or `lib.rs`)
- `src/App.tsx` (or similar)
- `package.json`

**Step 4: Commit**

```bash
git add .
git commit -m "chore: scaffold tauri v2 + react-ts frontend"
```

---

## Phase 2: Python Sidecar Core (tested independently)

### Task 4: Implement IPC protocol handler

**Files:**
- Create: `sidecar/src/whisper_sidecar/ipc.py`
- Create: `sidecar/tests/test_ipc.py`
- Modify: `sidecar/src/whisper_sidecar/__main__.py`

**Step 1: Write the failing test**

```python
# sidecar/tests/test_ipc.py
import json
from io import StringIO
from whisper_sidecar.ipc import read_command, write_event


def test_read_command_parses_json_line():
    stream = StringIO('{"cmd": "start_recording"}\n')
    cmd = read_command(stream)
    assert cmd == {"cmd": "start_recording"}


def test_read_command_returns_none_on_eof():
    stream = StringIO("")
    cmd = read_command(stream)
    assert cmd is None


def test_write_event_outputs_json_line():
    output = StringIO()
    write_event(output, "ready", model="large-v3-turbo", device="cuda")
    output.seek(0)
    line = output.readline()
    data = json.loads(line)
    assert data["event"] == "ready"
    assert data["model"] == "large-v3-turbo"
    assert data["device"] == "cuda"
```

**Step 2: Run test to verify it fails**

```bash
cd sidecar && uv run pytest tests/test_ipc.py -v
```

Expected: FAIL — `ModuleNotFoundError: No module named 'whisper_sidecar.ipc'`

**Step 3: Implement IPC module**

```python
# sidecar/src/whisper_sidecar/ipc.py
import json
import sys
from typing import Any, TextIO


def read_command(stream: TextIO = None) -> dict | None:
    """Read one JSON line command from stream. Returns None on EOF."""
    if stream is None:
        stream = sys.stdin
    line = stream.readline()
    if not line:
        return None
    return json.loads(line.strip())


def write_event(stream: TextIO = None, event: str = "", **kwargs: Any) -> None:
    """Write a JSON line event to stream."""
    if stream is None:
        stream = sys.stdout
    data = {"event": event, **kwargs}
    stream.write(json.dumps(data, ensure_ascii=False) + "\n")
    stream.flush()
```

**Step 4: Run test to verify it passes**

```bash
cd sidecar && uv run pytest tests/test_ipc.py -v
```

Expected: 3 passed

**Step 5: Implement `__main__.py` entry point (skeleton)**

```python
# sidecar/src/whisper_sidecar/__main__.py
import sys
from whisper_sidecar.ipc import read_command, write_event


def main():
    """Main IPC loop. Reads commands from stdin, writes events to stdout."""
    write_event(sys.stdout, "ready", model="loading", device="initializing")

    while True:
        cmd = read_command(sys.stdin)
        if cmd is None:
            break  # stdin closed, parent exited

        action = cmd.get("cmd")
        if action == "shutdown":
            break
        elif action == "start_recording":
            write_event(sys.stdout, "status", state="listening")
        elif action == "stop_recording":
            write_event(sys.stdout, "status", state="transcribing")
            # TODO: actual transcription
            write_event(sys.stdout, "transcription", text="[placeholder]")
        else:
            write_event(sys.stdout, "error", message=f"unknown command: {action}")


if __name__ == "__main__":
    main()
```

**Step 6: Commit**

```bash
git add sidecar/src/whisper_sidecar/ipc.py sidecar/src/whisper_sidecar/__main__.py sidecar/tests/test_ipc.py
git commit -m "feat(sidecar): implement stdio JSON lines IPC protocol"
```

---

### Task 5: Implement audio recorder with VAD

**Files:**
- Create: `sidecar/src/whisper_sidecar/recorder.py`
- Create: `sidecar/tests/test_recorder.py`

**Step 1: Write the failing test**

```python
# sidecar/tests/test_recorder.py
import numpy as np
from whisper_sidecar.recorder import AudioBuffer


def test_audio_buffer_append_and_get():
    buf = AudioBuffer(sample_rate=16000)
    chunk = np.zeros(1600, dtype=np.float32)  # 100ms
    buf.append(chunk)
    audio = buf.get_audio()
    assert audio.shape == (1600,)
    assert audio.dtype == np.float32


def test_audio_buffer_clear():
    buf = AudioBuffer(sample_rate=16000)
    buf.append(np.ones(1600, dtype=np.float32))
    buf.clear()
    audio = buf.get_audio()
    assert audio.shape == (0,)


def test_audio_buffer_duration():
    buf = AudioBuffer(sample_rate=16000)
    buf.append(np.zeros(16000, dtype=np.float32))  # 1 second
    assert abs(buf.duration_seconds() - 1.0) < 0.001
```

**Step 2: Run test to verify it fails**

```bash
cd sidecar && uv run pytest tests/test_recorder.py -v
```

Expected: FAIL

**Step 3: Implement AudioBuffer**

```python
# sidecar/src/whisper_sidecar/recorder.py
import numpy as np


class AudioBuffer:
    """Accumulates audio chunks for later transcription."""

    def __init__(self, sample_rate: int = 16000):
        self.sample_rate = sample_rate
        self._chunks: list[np.ndarray] = []

    def append(self, chunk: np.ndarray) -> None:
        self._chunks.append(chunk)

    def get_audio(self) -> np.ndarray:
        if not self._chunks:
            return np.array([], dtype=np.float32)
        return np.concatenate(self._chunks)

    def clear(self) -> None:
        self._chunks.clear()

    def duration_seconds(self) -> float:
        total_samples = sum(c.shape[0] for c in self._chunks)
        return total_samples / self.sample_rate
```

**Step 4: Run test to verify it passes**

```bash
cd sidecar && uv run pytest tests/test_recorder.py -v
```

Expected: 3 passed

**Step 5: Add sounddevice recording wrapper**

```python
# Append to sidecar/src/whisper_sidecar/recorder.py
import threading
import sounddevice as sd


class Recorder:
    """Records audio from the default microphone into an AudioBuffer."""

    def __init__(self, sample_rate: int = 16000):
        self.sample_rate = sample_rate
        self.buffer = AudioBuffer(sample_rate)
        self._stream: sd.InputStream | None = None
        self._recording = False

    def _callback(self, indata, frames, time_info, status):
        if self._recording:
            self.buffer.append(indata[:, 0].copy())

    def start(self) -> None:
        self.buffer.clear()
        self._recording = True
        self._stream = sd.InputStream(
            samplerate=self.sample_rate,
            channels=1,
            dtype="float32",
            callback=self._callback,
        )
        self._stream.start()

    def stop(self) -> np.ndarray:
        self._recording = False
        if self._stream:
            self._stream.stop()
            self._stream.close()
            self._stream = None
        return self.buffer.get_audio()
```

Note: `sounddevice` requires PortAudio which may not work in WSL2. This code is tested on Windows. The `AudioBuffer` unit tests work everywhere.

**Step 6: Commit**

```bash
git add sidecar/src/whisper_sidecar/recorder.py sidecar/tests/test_recorder.py
git commit -m "feat(sidecar): implement audio buffer and recorder"
```

---

### Task 6: Implement transcriber wrapper

**Files:**
- Create: `sidecar/src/whisper_sidecar/transcriber.py`
- Create: `sidecar/tests/test_transcriber.py`

**Step 1: Write the failing test**

```python
# sidecar/tests/test_transcriber.py
from unittest.mock import MagicMock, patch
from whisper_sidecar.transcriber import Transcriber


def test_transcriber_init_stores_config():
    t = Transcriber.__new__(Transcriber)
    t.model_size = "large-v3-turbo"
    t.device = "cuda"
    t.compute_type = "int8_float16"
    t.model = None
    assert t.model_size == "large-v3-turbo"
    assert t.device == "cuda"


def test_transcribe_returns_text():
    """Test that transcribe() extracts text from model segments."""
    t = Transcriber.__new__(Transcriber)
    t.model = MagicMock()

    # Mock segment objects
    seg1 = MagicMock()
    seg1.text = "안녕하세요"
    seg2 = MagicMock()
    seg2.text = " 테스트입니다"

    t.model.transcribe.return_value = ([seg1, seg2], MagicMock())

    import numpy as np
    audio = np.zeros(16000, dtype=np.float32)
    result = t.transcribe(audio)
    assert result == "안녕하세요 테스트입니다"
```

**Step 2: Run test to verify it fails**

```bash
cd sidecar && uv run pytest tests/test_transcriber.py -v
```

Expected: FAIL

**Step 3: Implement transcriber**

```python
# sidecar/src/whisper_sidecar/transcriber.py
import numpy as np
from faster_whisper import WhisperModel


class Transcriber:
    """Wraps faster-whisper for local GPU transcription."""

    def __init__(
        self,
        model_size: str = "large-v3-turbo",
        device: str = "cuda",
        compute_type: str = "int8_float16",
    ):
        self.model_size = model_size
        self.device = device
        self.compute_type = compute_type
        self.model = WhisperModel(
            model_size,
            device=device,
            compute_type=compute_type,
        )

    def transcribe(self, audio: np.ndarray) -> str:
        """Transcribe audio array to text. Auto-detects language."""
        segments, _info = self.model.transcribe(
            audio,
            beam_size=5,
            vad_filter=True,
        )
        return "".join(seg.text for seg in segments).strip()
```

Note: The `__init__` with real model loading only works on Windows with CUDA. Tests mock the model.

**Step 4: Run test to verify it passes**

```bash
cd sidecar && uv run pytest tests/test_transcriber.py -v
```

Expected: 2 passed

**Step 5: Commit**

```bash
git add sidecar/src/whisper_sidecar/transcriber.py sidecar/tests/test_transcriber.py
git commit -m "feat(sidecar): implement faster-whisper transcriber wrapper"
```

---

### Task 7: Implement clipboard text injector

**Files:**
- Create: `sidecar/src/whisper_sidecar/injector.py`
- Create: `sidecar/tests/test_injector.py`

**Step 1: Write the failing test**

```python
# sidecar/tests/test_injector.py
from unittest.mock import patch, MagicMock
from whisper_sidecar.injector import ClipboardInjector


@patch("whisper_sidecar.injector.pyperclip")
def test_inject_text_saves_and_restores_clipboard(mock_clip):
    mock_clip.paste.return_value = "original"
    injector = ClipboardInjector()

    with patch.object(injector, "_simulate_paste") as mock_paste:
        injector.inject("새 텍스트")

    # Should save original, set new text, paste, then restore
    assert mock_clip.copy.call_count == 2
    mock_clip.copy.assert_any_call("새 텍스트")
    mock_clip.copy.assert_any_call("original")
```

**Step 2: Run test to verify it fails**

```bash
cd sidecar && uv run pytest tests/test_injector.py -v
```

Expected: FAIL

**Step 3: Implement injector**

```python
# sidecar/src/whisper_sidecar/injector.py
import time
import pyperclip


class ClipboardInjector:
    """Injects text into the active window via clipboard paste."""

    def inject(self, text: str) -> None:
        """Save clipboard, paste text, restore clipboard."""
        original = pyperclip.paste()
        pyperclip.copy(text)
        time.sleep(0.05)  # Small delay for clipboard to update
        self._simulate_paste()
        time.sleep(0.05)
        pyperclip.copy(original)

    def _simulate_paste(self) -> None:
        """Simulate Ctrl+V keypress. Windows-only via ctypes."""
        try:
            import ctypes
            user32 = ctypes.windll.user32
            VK_CONTROL = 0x11
            VK_V = 0x56
            KEYEVENTF_KEYUP = 0x0002
            user32.keybd_event(VK_CONTROL, 0, 0, 0)
            user32.keybd_event(VK_V, 0, 0, 0)
            user32.keybd_event(VK_V, 0, KEYEVENTF_KEYUP, 0)
            user32.keybd_event(VK_CONTROL, 0, KEYEVENTF_KEYUP, 0)
        except AttributeError:
            # Not on Windows (e.g., WSL2 during development)
            pass
```

**Step 4: Run test to verify it passes**

```bash
cd sidecar && uv run pytest tests/test_injector.py -v
```

Expected: 1 passed

**Step 5: Commit**

```bash
git add sidecar/src/whisper_sidecar/injector.py sidecar/tests/test_injector.py
git commit -m "feat(sidecar): implement clipboard text injector"
```

---

### Task 8: Wire up the full sidecar main loop

**Files:**
- Modify: `sidecar/src/whisper_sidecar/__main__.py`
- Create: `sidecar/tests/test_main.py`

**Step 1: Write integration test**

```python
# sidecar/tests/test_main.py
import json
from io import StringIO
from unittest.mock import patch, MagicMock
from whisper_sidecar.__main__ import main


def test_main_loop_start_stop_transcribe():
    """Test the full IPC loop with mocked transcriber and recorder."""
    commands = (
        '{"cmd": "start_recording"}\n'
        '{"cmd": "stop_recording"}\n'
        '{"cmd": "shutdown"}\n'
    )
    stdin = StringIO(commands)
    stdout = StringIO()

    mock_transcriber = MagicMock()
    mock_transcriber.model_size = "large-v3-turbo"
    mock_transcriber.device = "cuda"
    mock_transcriber.transcribe.return_value = "테스트"

    mock_recorder = MagicMock()
    import numpy as np
    mock_recorder.stop.return_value = np.zeros(16000, dtype=np.float32)

    mock_injector = MagicMock()

    with patch("whisper_sidecar.__main__.Transcriber", return_value=mock_transcriber), \
         patch("whisper_sidecar.__main__.Recorder", return_value=mock_recorder), \
         patch("whisper_sidecar.__main__.ClipboardInjector", return_value=mock_injector):
        main(stdin=stdin, stdout=stdout)

    stdout.seek(0)
    events = [json.loads(line) for line in stdout.readlines()]
    event_types = [(e["event"], e.get("state") or e.get("text", "")) for e in events]

    assert ("ready", "") in event_types or any(e["event"] == "ready" for e in events)
    assert ("status", "listening") in event_types
    assert ("status", "transcribing") in event_types
    assert any(e["event"] == "transcription" for e in events)
```

**Step 2: Run test to verify it fails**

```bash
cd sidecar && uv run pytest tests/test_main.py -v
```

Expected: FAIL

**Step 3: Update `__main__.py` with full wiring**

```python
# sidecar/src/whisper_sidecar/__main__.py
import sys
from typing import TextIO

from whisper_sidecar.ipc import read_command, write_event
from whisper_sidecar.recorder import Recorder
from whisper_sidecar.transcriber import Transcriber
from whisper_sidecar.injector import ClipboardInjector


def main(stdin: TextIO = None, stdout: TextIO = None):
    if stdin is None:
        stdin = sys.stdin
    if stdout is None:
        stdout = sys.stdout

    transcriber = Transcriber()
    recorder = Recorder()
    injector = ClipboardInjector()

    write_event(stdout, "ready", model=transcriber.model_size, device=transcriber.device)

    while True:
        cmd = read_command(stdin)
        if cmd is None:
            break

        action = cmd.get("cmd")

        if action == "shutdown":
            break
        elif action == "start_recording":
            recorder.start()
            write_event(stdout, "status", state="listening")
        elif action == "stop_recording":
            write_event(stdout, "status", state="transcribing")
            audio = recorder.stop()
            if audio.size > 0:
                text = transcriber.transcribe(audio)
                if text:
                    injector.inject(text)
                    write_event(stdout, "transcription", text=text)
                else:
                    write_event(stdout, "transcription", text="")
            else:
                write_event(stdout, "transcription", text="")
        else:
            write_event(stdout, "error", message=f"unknown command: {action}")


if __name__ == "__main__":
    main()
```

**Step 4: Run test to verify it passes**

```bash
cd sidecar && uv run pytest tests/test_main.py -v
```

Expected: PASS

**Step 5: Commit**

```bash
git add sidecar/src/whisper_sidecar/__main__.py sidecar/tests/test_main.py
git commit -m "feat(sidecar): wire up full IPC main loop with recorder, transcriber, injector"
```

---

## Phase 3: Tauri Shell (Rust)

### Task 9: Configure Tauri plugins and sidecar permissions

**Files:**
- Modify: `src-tauri/Cargo.toml` — add plugin dependencies
- Modify: `src-tauri/tauri.conf.json` — configure sidecar, window
- Create: `src-tauri/capabilities/default.json` — permissions

**Step 1: Add Tauri plugin dependencies to `Cargo.toml`**

Add to `[dependencies]`:
```toml
tauri-plugin-global-shortcut = "2"
tauri-plugin-shell = "2"
```

**Step 2: Configure `tauri.conf.json`**

Key sections to add/modify:

```json
{
  "bundle": {
    "externalBin": ["binaries/whisper-sidecar"]
  },
  "app": {
    "windows": [
      {
        "title": "Whisper Windows",
        "width": 300,
        "height": 80,
        "decorations": false,
        "transparent": true,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "visible": false
      }
    ]
  }
}
```

**Step 3: Create capabilities file**

```json
// src-tauri/capabilities/default.json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capabilities",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "shell:allow-spawn",
    "shell:allow-stdin-write",
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister"
  ]
}
```

**Step 4: Commit**

```bash
git add src-tauri/
git commit -m "chore(tauri): configure plugins, sidecar, and permissions"
```

---

### Task 10: Implement Rust sidecar manager and hotkey handler

**Files:**
- Modify: `src-tauri/src/main.rs` (or `lib.rs`)

**Step 1: Implement main.rs**

```rust
// src-tauri/src/lib.rs
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tauri_plugin_shell::ShellExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static RECORDING: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // Spawn Python sidecar
            let shell = app.shell();
            let sidecar = shell.sidecar("whisper-sidecar")
                .expect("failed to create sidecar command");

            let (mut rx, child) = sidecar.spawn()
                .expect("failed to spawn sidecar");

            // Store child handle for stdin writes
            let child = Arc::new(std::sync::Mutex::new(child));
            let child_clone = child.clone();

            // Listen to sidecar stdout
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri_plugin_shell::process::CommandEvent;
                while let Some(event) = rx.recv().await {
                    match event {
                        CommandEvent::Stdout(line) => {
                            // Forward to frontend
                            let _ = app_handle.emit("sidecar-event", String::from_utf8_lossy(&line).to_string());
                        }
                        CommandEvent::Stderr(line) => {
                            eprintln!("sidecar stderr: {}", String::from_utf8_lossy(&line));
                        }
                        _ => {}
                    }
                }
            });

            // Register global hotkey (Ctrl+H)
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL), Code::KeyH);
            app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, _event| {
                let is_recording = RECORDING.load(Ordering::SeqCst);
                let cmd = if is_recording {
                    r#"{"cmd": "stop_recording"}"#
                } else {
                    r#"{"cmd": "start_recording"}"#
                };
                RECORDING.store(!is_recording, Ordering::SeqCst);

                if let Ok(mut child) = child_clone.lock() {
                    let _ = child.write((cmd.to_string() + "\n").as_bytes());
                }
            })?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Note: This is the core Rust logic. Exact API may vary slightly with Tauri v2 version — check docs during implementation. The sidecar binary must be placed at `src-tauri/binaries/whisper-sidecar-{target-triple}` (e.g., `whisper-sidecar-x86_64-pc-windows-msvc.exe`).

**Step 2: Commit**

```bash
git add src-tauri/src/
git commit -m "feat(tauri): implement sidecar manager and global hotkey handler"
```

---

### Task 11: Add system tray

**Files:**
- Modify: `src-tauri/src/lib.rs` — add tray setup
- Create: `src-tauri/icons/tray-idle.png` (16x16 or 32x32)

**Step 1: Add tray icon to Cargo.toml**

The tray icon feature is built into Tauri v2 core. Ensure `"tray-icon"` feature is enabled in `tauri` dependency.

**Step 2: Add tray setup in `setup()`**

```rust
use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent};
use tauri::menu::{Menu, MenuItem};

// Inside setup():
let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
let menu = Menu::with_items(app, &[&quit])?;

let _tray = TrayIconBuilder::new()
    .icon(app.default_window_icon().unwrap().clone())
    .menu(&menu)
    .tooltip("Whisper Windows — Ready")
    .on_menu_event(|app, event| {
        if event.id.as_ref() == "quit" {
            app.exit(0);
        }
    })
    .build(app)?;
```

**Step 3: Commit**

```bash
git add src-tauri/
git commit -m "feat(tauri): add system tray with quit menu"
```

---

## Phase 4: React Frontend (Overlay UI)

### Task 12: Build overlay status component

**Files:**
- Modify: `src/App.tsx`
- Create: `src/components/StatusOverlay.tsx`
- Create: `src/App.css` (if not exists, modify existing)

**Step 1: Create StatusOverlay component**

```tsx
// src/components/StatusOverlay.tsx
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

type AppState = "ready" | "listening" | "transcribing";

export function StatusOverlay() {
  const [state, setState] = useState<AppState>("ready");
  const [lastText, setLastText] = useState("");

  useEffect(() => {
    const unlisten = listen<string>("sidecar-event", (event) => {
      try {
        const data = JSON.parse(event.payload);
        if (data.event === "status") {
          setState(data.state as AppState);
        } else if (data.event === "transcription") {
          setLastText(data.text);
          setState("ready");
        } else if (data.event === "ready") {
          setState("ready");
        }
      } catch {}
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  const stateConfig = {
    ready: { label: "Ready", color: "#4ade80" },
    listening: { label: "Listening...", color: "#f97316" },
    transcribing: { label: "Transcribing...", color: "#3b82f6" },
  };

  const config = stateConfig[state];

  return (
    <div className="overlay">
      <div className="status-dot" style={{ backgroundColor: config.color }} />
      <span className="status-label">{config.label}</span>
    </div>
  );
}
```

**Step 2: Style the overlay**

```css
/* src/App.css */
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  background: transparent;
  overflow: hidden;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
}

.overlay {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  background: rgba(0, 0, 0, 0.8);
  border-radius: 20px;
  color: white;
  font-size: 14px;
  user-select: none;
  -webkit-app-region: drag;
}

.status-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  animation: pulse 1.5s infinite;
}

.status-label {
  font-weight: 500;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}
```

**Step 3: Update App.tsx**

```tsx
// src/App.tsx
import { StatusOverlay } from "./components/StatusOverlay";
import "./App.css";

function App() {
  return <StatusOverlay />;
}

export default App;
```

**Step 4: Commit**

```bash
git add src/
git commit -m "feat(ui): implement status overlay component"
```

---

## Phase 5: Integration & Testing

### Task 13: End-to-end integration test on Windows

**Files:** None created — this is a manual test task.

**Step 1: Build the Python sidecar**

On Windows:
```bash
cd sidecar
uv run pyinstaller --onefile --name whisper-sidecar src/whisper_sidecar/__main__.py
```

Copy the output `.exe` to `src-tauri/binaries/whisper-sidecar-x86_64-pc-windows-msvc.exe`.

**Step 2: Run Tauri dev mode**

```bash
pnpm tauri dev
```

**Step 3: Test the flow**

1. Verify tray icon appears
2. Verify overlay window shows "Ready"
3. Press Ctrl+H — verify overlay shows "Listening..."
4. Speak a short Korean sentence
5. Press Ctrl+H again — verify overlay shows "Transcribing..."
6. Verify text is pasted into an open text editor
7. Verify overlay returns to "Ready"

**Step 4: Test GPU usage**

Open `nvidia-smi` in another terminal:
```bash
nvidia-smi -l 1
```

Verify:
- Model is loaded on GPU (~1.5GB VRAM for int8)
- VRAM usage stays under 3GB total
- GPU utilization spikes briefly during transcription

**Step 5: Measure latency**

Time from pressing Ctrl+H (stop) to text appearing. Target: < 1 second for short sentences.

---

### Task 14: Add PyInstaller build script

**Files:**
- Create: `sidecar/build.py`

**Step 1: Create build script**

```python
# sidecar/build.py
"""Build the sidecar .exe for Tauri bundling."""
import subprocess
import shutil
import platform
from pathlib import Path

def build():
    target_triple = {
        ("Windows", "AMD64"): "x86_64-pc-windows-msvc",
    }.get((platform.system(), platform.machine()), "unknown")

    subprocess.run([
        "uv", "run", "pyinstaller",
        "--onefile",
        "--name", "whisper-sidecar",
        "--distpath", "dist",
        "src/whisper_sidecar/__main__.py",
    ], check=True, cwd=Path(__file__).parent)

    # Copy to Tauri binaries directory
    src = Path(__file__).parent / "dist" / "whisper-sidecar.exe"
    dst_dir = Path(__file__).parent.parent / "src-tauri" / "binaries"
    dst_dir.mkdir(parents=True, exist_ok=True)
    dst = dst_dir / f"whisper-sidecar-{target_triple}.exe"
    shutil.copy2(src, dst)
    print(f"Copied to {dst}")

if __name__ == "__main__":
    build()
```

**Step 2: Add pyinstaller to dev dependencies**

In `sidecar/pyproject.toml`, add to `[dependency-groups] dev`:
```toml
dev = [
    "pytest>=8.0",
    "pytest-asyncio>=0.23",
    "pyinstaller>=6.0",
]
```

**Step 3: Commit**

```bash
git add sidecar/build.py sidecar/pyproject.toml
git commit -m "chore(sidecar): add PyInstaller build script for Tauri bundling"
```

---

## Execution Order Summary

| Phase | Task | Description | Depends on |
|-------|------|-------------|------------|
| 1 | 1 | mise config + gitignore | — |
| 1 | 2 | Python sidecar project init | Task 1 |
| 1 | 3 | Tauri + React scaffold | Task 1 |
| 2 | 4 | IPC protocol handler | Task 2 |
| 2 | 5 | Audio recorder + VAD | Task 2 |
| 2 | 6 | Transcriber wrapper | Task 2 |
| 2 | 7 | Clipboard injector | Task 2 |
| 2 | 8 | Full sidecar main loop | Tasks 4-7 |
| 3 | 9 | Tauri plugin config | Task 3 |
| 3 | 10 | Rust sidecar + hotkey | Task 9 |
| 3 | 11 | System tray | Task 10 |
| 4 | 12 | React overlay UI | Task 3 |
| 5 | 13 | E2E integration test | Tasks 8, 11, 12 |
| 5 | 14 | PyInstaller build script | Task 8 |

Tasks 2 & 3 can run in parallel. Tasks 4-7 can run in parallel. Tasks 9-11 are sequential. Task 12 can run in parallel with Phase 3.
