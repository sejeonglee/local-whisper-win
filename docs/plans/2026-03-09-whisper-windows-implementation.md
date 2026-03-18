# Whisper Windows Implementation Plan

**Goal:** Build a Windows desktop dictation app using Tauri v2 + React + Python sidecar so that a fixed hotkey starts and stops recording, speech is transcribed locally, the text is pasted into the previously focused window, and the original clipboard contents are restored exactly.

**Primary target:** Windows x86_64 on an RTX 4050 laptop.

**Future target:** Windows on ARM / Qualcomm Snapdragon laptops.

**MVP scope:** End-to-end dictation works with a fixed `Ctrl+H` hotkey, first-run model download, overlay status, tray, and exact clipboard restoration. Configurable hotkeys and broader settings are intentionally deferred.

## Phase 0: Architecture Hardening

### Task 0.1: Freeze ownership boundaries and state machine

Deliverables:

- Rust owns hotkey, tray, overlay, sidecar supervision, clipboard snapshot/restore, and paste injection
- Python owns model bootstrap, audio capture, and transcription
- React owns display only
- Shell state machine documented as `starting -> downloading_model -> loading_model -> ready -> listening_requested -> listening -> transcribing -> ready|error`

Acceptance criteria:

- No clipboard or desktop automation code is planned in Python
- The shell never assumes recording started until the sidecar confirms it
- Empty audio returns to `ready` without paste

### Task 0.2: Windows-only spike for overlay behavior

Goal: prove the overlay can show status without stealing focus from the target application.

Acceptance criteria:

- Overlay can appear while another editor remains active
- Hotkey works while the overlay is hidden
- Multi-monitor positioning is at least understood, even if MVP uses a fixed position

### Task 0.3: Windows-only spike for full clipboard preservation

Goal: verify that clipboard snapshot/restore preserves non-text payloads, not just plain text.

Acceptance criteria:

- Image clipboard contents survive a transcription cycle
- Rich text or HTML clipboard contents survive a transcription cycle
- Paste uses Win32 input simulation from Rust, not Python

## Phase 1: Project Scaffolding and Tooling

### Task 1: Initialize repo tooling

Files:

- Create `.mise.toml`
- Create `.gitignore`

Requirements:

- Python 3.12
- Node 22
- Rust via rustup on Windows
- Commit lockfiles for reproducible environments

Notes:

- Native Windows is the primary development and validation environment
- WSL may be used for editing and pure unit tests only

### Task 2: Scaffold the Python sidecar with `uv`

Files:

- Create `sidecar/pyproject.toml`
- Create `sidecar/.python-version`
- Create `sidecar/src/whisper_sidecar/__init__.py`
- Create `sidecar/src/whisper_sidecar/ipc.py`
- Create `sidecar/src/whisper_sidecar/bootstrap.py`
- Create `sidecar/src/whisper_sidecar/recorder.py`
- Create `sidecar/src/whisper_sidecar/transcriber.py`
- Create `sidecar/src/whisper_sidecar/__main__.py`

Requirements:

- Keep dependencies tight enough for desktop shipping; avoid loose `>=` ranges where possible
- Python contains no clipboard code
- Sidecar emits structured JSON-line events only

### Task 3: Scaffold Tauri v2 frontend

Files:

- Create `package.json`
- Create `src/`
- Create `src-tauri/`

Requirements:

- Tauri shell launches on Windows
- Overlay window is hidden by default
- Tray plumbing is present even before full logic is wired

## Phase 2: Python Sidecar Core

### Task 4: Implement IPC module and protocol tests

Files:

- `sidecar/src/whisper_sidecar/ipc.py`
- `sidecar/tests/test_ipc.py`

Requirements:

- Every message includes `type` and `version`
- Invalid messages return structured errors, not crashes
- EOF is treated as parent shutdown

### Task 5: Implement model bootstrap and cache detection

Files:

- `sidecar/src/whisper_sidecar/bootstrap.py`
- `sidecar/tests/test_bootstrap.py`

Requirements:

- Check whether the selected model is already cached
- If missing, download it on first run and emit progress events
- If already present, skip download entirely
- Prevent partial downloads from being treated as valid cache
- Emit `starting`, `model_download_started`, `model_download_progress`, `loading_model`, and `ready`

### Task 6: Implement recorder with bounded buffering

Files:

- `sidecar/src/whisper_sidecar/recorder.py`
- `sidecar/tests/test_recorder.py`

Requirements:

- 16 kHz mono float32 capture
- Max recording duration guardrail
- Explicit handling for empty or too-short recordings
- Explicit microphone/device errors surfaced through IPC-friendly exceptions

### Task 7: Implement transcriber wrapper

Files:

- `sidecar/src/whisper_sidecar/transcriber.py`
- `sidecar/tests/test_transcriber.py`

Requirements:

- Use `large-v3-turbo` with the target backend configuration
- Use faster-whisper built-in VAD for MVP
- Return backend metadata as part of startup state
- Keep the public API simple so a future ARM backend can swap in later

### Task 8: Wire the full sidecar main loop

Files:

- `sidecar/src/whisper_sidecar/__main__.py`
- `sidecar/tests/test_main.py`

Requirements:

- Startup path: boot -> optional download -> model load -> ready
- Runtime path: start recording -> listening -> stop -> transcribing -> transcription|empty_audio
- Fatal startup failures emit `fatal` and terminate cleanly
- No paste or clipboard logic in the sidecar

## Phase 3: Rust Shell

### Task 9: Configure Tauri plugins and permissions

Files:

- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `src-tauri/capabilities/default.json`

Requirements:

- Sidecar spawn permissions configured
- Global shortcut permissions configured
- Overlay window configured but hidden by default

### Task 10: Implement sidecar supervisor and app state store

Files:

- `src-tauri/src/main.rs`
- `src-tauri/src/state.rs`
- `src-tauri/src/sidecar.rs`

Requirements:

- Spawn the sidecar and parse structured events
- Maintain authoritative shell state
- Gate hotkey transitions based on state
- Surface startup, download, ready, recording, transcription, and error states to the UI
- Detect child exit and move into an error state instead of silently hanging

### Task 11: Implement fixed MVP hotkey flow

Files:

- `src-tauri/src/main.rs`
- `src-tauri/src/state.rs`

Requirements:

- Register `Ctrl+H`
- First press from `ready` requests start recording
- Second press from `listening` requests stop recording
- Hotkey is ignored during `starting`, `downloading_model`, `loading_model`, and `transcribing`

### Task 12: Implement clipboard snapshot, paste, and restore in Rust

Files:

- `src-tauri/src/clipboard.rs`
- `src-tauri/src/main.rs`

Requirements:

- Preserve non-text clipboard formats
- Paste the transcription into the previously active window
- Restore original clipboard contents after paste
- If paste fails, still attempt clipboard restoration

### Task 13: Add tray and user-visible error handling

Files:

- `src-tauri/src/main.rs`
- `src-tauri/icons/`

Requirements:

- Tray shows basic lifecycle state in tooltip or menu text
- Quit action shuts down the sidecar cleanly
- Fatal error state is visible without attaching a debugger

## Phase 4: Frontend Overlay

### Task 14: Build overlay status component

Files:

- `src/App.tsx`
- `src/components/StatusOverlay.tsx`
- `src/App.css`

Requirements:

- Show `starting`, `downloading`, `loading`, `ready`, `listening`, `transcribing`, and `error`
- Show model download progress on first run
- Keep UI thin; no app logic beyond state display

## Phase 5: Validation and Packaging

### Task 15: Validate end-to-end on native Windows

Requirements:

- Test on the target x86_64 laptop, not only in WSL
- Verify first-run download path
- Verify cached restart path
- Verify clipboard restoration with text, image, and rich text clipboard contents
- Verify overlay does not steal focus from the target app

### Task 16: Package the Python sidecar for Tauri bundling

Files:

- `sidecar/build.py`
- `sidecar/pyproject.toml`

Requirements:

- Produce `whisper-sidecar-{target-triple}.exe`
- Bundle only the sidecar executable, not the model weights
- Keep model download as a runtime concern on first launch

### Task 17: Clean-machine Windows test

Requirements:

- Validate installer and sidecar startup on a clean Windows environment
- Validate first-run model download and subsequent cache reuse
- Validate tray, hotkey, paste, and shutdown behavior without dev tools attached

## Execution Order Summary

| Phase | Task | Description | Depends on |
|---|---|---|---|
| 0 | 0.1 | Freeze boundaries and state machine | — |
| 0 | 0.2 | Overlay behavior spike | 0.1 |
| 0 | 0.3 | Clipboard preservation spike | 0.1 |
| 1 | 1 | Tooling setup | 0.1 |
| 1 | 2 | Python sidecar scaffold | 1 |
| 1 | 3 | Tauri scaffold | 1 |
| 2 | 4 | IPC module | 2 |
| 2 | 5 | Model bootstrap and cache | 4 |
| 2 | 6 | Recorder | 2 |
| 2 | 7 | Transcriber | 5 |
| 2 | 8 | Full sidecar loop | 4, 5, 6, 7 |
| 3 | 9 | Tauri permissions | 3 |
| 3 | 10 | Sidecar supervisor and state | 8, 9 |
| 3 | 11 | Fixed hotkey flow | 10 |
| 3 | 12 | Clipboard snapshot and restore | 10 |
| 3 | 13 | Tray and error handling | 10 |
| 4 | 14 | Overlay UI | 10 |
| 5 | 15 | Native Windows E2E validation | 11, 12, 13, 14 |
| 5 | 16 | Sidecar packaging | 8 |
| 5 | 17 | Clean-machine Windows test | 15, 16 |

## Notes for Future ARM / Snapdragon Support

- Do not let the UI or shell assume `cuda` is the only backend string.
- Keep model/bootstrap code isolated so an ARM-specific backend can replace the current CUDA path later.
- Treat ARM support as a new backend target, not as a small post-processing tweak.
