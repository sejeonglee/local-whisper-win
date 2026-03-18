# Whisper Windows — Design Document

## Overview

A Windows desktop dictation app that replaces the built-in Windows speech recognition for one primary machine first: an x86_64 Windows laptop with an NVIDIA RTX 4050 6GB GPU. The app records short dictation, transcribes locally with faster-whisper, pastes the text into the previously focused application, and restores the original clipboard contents exactly.

The first shipping target is Windows x86_64 with CUDA. A future target is Windows on ARM (for Qualcomm Snapdragon laptops), so the app boundary should avoid hard-coding CUDA assumptions outside the Python transcription layer.

## Product Scope

### MVP

- Fixed global hotkey: `Ctrl+H`
- Toggle dictation: press once to start, press again to stop
- Resident Python sidecar with local model loaded once and kept warm
- First-run model download with cache reuse on later launches
- Small always-on-top overlay with status and download/error feedback
- Tray icon with basic status and quit action
- Local transcription for Korean and English mixed speech
- Clipboard-based paste into the previously active window with full clipboard restoration

### Out of MVP

- Configurable hotkey
- Settings GUI beyond basic status/progress display
- Streaming or partial transcription
- CPU fallback for unsupported machines
- Auto-update mechanism
- Cross-platform support
- Multiple language profiles

## User Flow

1. App starts.
2. Rust/Tauri shell starts the Python sidecar.
3. Python sidecar checks whether the configured model already exists in local cache.
4. If the model is missing, Python downloads it and emits progress events.
5. Python loads the model and emits `ready`.
6. Tray icon and overlay show `Ready`.
7. User presses `Ctrl+H`.
8. Rust shell switches state to `listening_requested` and sends `start_recording`.
9. Python starts microphone capture and emits `listening`.
10. User speaks.
11. User presses `Ctrl+H` again.
12. Rust shell sends `stop_recording`.
13. Python stops capture, transcribes with faster-whisper built-in VAD, and emits either `transcription` or `empty_audio`.
14. Rust shell snapshots the current clipboard, writes the transcription as plain text, issues a paste command to the previously focused window, then restores every original clipboard format.
15. Overlay returns to `Ready`.

## Architecture (3 layers)

```text
┌────────────────────────────────────────────────────────────┐
│                    Tauri v2 Shell (Rust)                  │
│  - Global hotkey registration                             │
│  - Tray icon and menu                                     │
│  - Overlay window lifecycle                               │
│  - Sidecar supervision and state store                    │
│  - Clipboard snapshot / restore (all clipboard formats)   │
│  - Text paste injection via Win32                         │
└───────────────────────┬────────────────────────────────────┘
                        │ stdio JSON lines
┌───────────────────────▼────────────────────────────────────┐
│                   Python Sidecar (.exe)                   │
│  - Model cache check and first-run download               │
│  - Audio capture                                          │
│  - faster-whisper model loading                           │
│  - Transcription                                          │
│  - Backend reporting (cuda today, extensible later)       │
└───────────────────────┬────────────────────────────────────┘
                        │ Tauri events / frontend state
┌───────────────────────▼────────────────────────────────────┐
│                 Web UI (React + TypeScript)               │
│  - Overlay status                                         │
│  - Download progress                                      │
│  - Recoverable / fatal error display                      │
└────────────────────────────────────────────────────────────┘
```

## Ownership Boundaries

### Rust / Tauri

- Owns every Windows-facing behavior
- Registers the fixed MVP hotkey
- Shows and hides the overlay without stealing focus
- Owns tray and app lifecycle
- Supervises the Python child process and parses IPC
- Preserves and restores full clipboard contents
- Injects the final paste into the previously active window

### Python Sidecar

- Owns model download, cache lookup, and model load
- Owns microphone capture and transcription pipeline
- Emits structured events to the shell
- Does not touch clipboard or window focus
- Does not own user-visible desktop behavior

### React / TypeScript

- Displays status only
- Does not own business logic
- Mirrors shell state and progress updates

## Runtime State Model

The shell stores the authoritative app state. Sidecar events drive state transitions for recording and transcription.

States:

- `starting`
- `downloading_model`
- `loading_model`
- `ready`
- `listening_requested`
- `listening`
- `transcribing`
- `error`

Rules:

- Hotkey input is ignored outside valid transitions.
- The shell never assumes recording started until the sidecar emits `listening`.
- Fatal sidecar failure moves the app into `error` and surfaces a tray/overlay message.
- Empty audio is not treated as an error; it returns to `ready` without pasting.

## IPC Protocol (stdio JSON lines)

Messages are one JSON object per line. Each message includes `type` and `version`.

### Tauri -> Python

```json
{"type":"command","version":1,"cmd":"start_recording"}
{"type":"command","version":1,"cmd":"stop_recording"}
{"type":"command","version":1,"cmd":"shutdown"}
```

### Python -> Tauri

```json
{"type":"event","version":1,"event":"starting"}
{"type":"event","version":1,"event":"model_download_started","model":"large-v3-turbo"}
{"type":"event","version":1,"event":"model_download_progress","received_bytes":123,"total_bytes":456}
{"type":"event","version":1,"event":"loading_model","backend":"cuda"}
{"type":"event","version":1,"event":"ready","model":"large-v3-turbo","backend":"cuda"}
{"type":"event","version":1,"event":"listening"}
{"type":"event","version":1,"event":"transcribing"}
{"type":"event","version":1,"event":"transcription","text":"안녕하세요"}
{"type":"event","version":1,"event":"empty_audio"}
{"type":"event","version":1,"event":"error","code":"microphone_unavailable","message":"..."}
{"type":"event","version":1,"event":"fatal","code":"startup_failed","message":"..."}
```

## Key Decisions

| Decision | Choice | Rationale |
|---|---|---|
| UI framework | Tauri v2 + React + TypeScript | Small shell, native Windows integration, enough UI for overlay/progress |
| Shell boundary | Rust owns all OS integration | Keeps clipboard, hotkey, tray, and window logic out of Python |
| STT engine | faster-whisper (CTranslate2) | Best perf/VRAM ratio for local inference |
| Model | `large-v3-turbo` | Good Korean/English quality on target GPU |
| Quantization | `int8_float16` | Fits target VRAM with headroom |
| Model delivery | First-run download, cached locally | Avoids giant installer while keeping later launches fast |
| MVP hotkey | Fixed `Ctrl+H` | Reduces early scope and state complexity |
| Trigger mode | Toggle | Simplest UX for first release |
| Clipboard strategy | Rust snapshot + restore all formats | Meets requirement to preserve non-text clipboard contents |
| VAD in MVP | faster-whisper built-in VAD only | Simpler first implementation than a separate Silero stage |
| Primary target | Windows x86_64 | Matches the first supported machine |
| Future target | Windows on ARM | Keep backend contract extensible beyond CUDA |
| Development environment | Native Windows primary, WSL optional | Windows-specific behavior must be validated on Windows |

## Project Structure

```text
whisper-windows/
├── .mise.toml
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs
│       ├── state.rs
│       ├── sidecar.rs
│       └── clipboard.rs
├── src/
│   ├── App.tsx
│   └── components/
│       └── StatusOverlay.tsx
├── sidecar/
│   ├── pyproject.toml
│   ├── .python-version
│   ├── src/
│   │   └── whisper_sidecar/
│   │       ├── __init__.py
│   │       ├── __main__.py
│   │       ├── ipc.py
│   │       ├── bootstrap.py
│   │       ├── recorder.py
│   │       └── transcriber.py
│   └── tests/
└── docs/plans/
```

## Model and Cache Strategy

- First launch checks whether the selected model is already available in the local model cache.
- If present, startup skips download and goes directly to model load.
- If missing, Python downloads the model before entering `ready`.
- Download progress is surfaced through IPC so the overlay can show the current phase.
- Partial or failed downloads must not be treated as usable cache entries.

## Performance Targets

- First launch with download excluded: model load to `ready` in a few seconds on the target laptop
- Short dictation latency after stop: under 1 second for common short utterances
- Idle memory should stay low outside the model footprint
- Clipboard restore must happen reliably even for non-text clipboard contents

## Known Risks and Mitigations

1. Global hotkey reliability on Windows can vary. Prototype early and fall back to raw Win32 APIs if the plugin is insufficient.
2. Overlay "show without activating" is a Windows UX risk. Validate it before spending time on UI polish.
3. Full clipboard preservation is more complex than plain text restore. Keep clipboard handling in Rust and spike it early.
4. First-run model download introduces failure and progress UX that the original plan did not need. Treat download as a first-class startup state.
5. CUDA-specific assumptions help the first machine but can block ARM later. Keep backend identifiers and config extensible.
6. WSL is convenient for editing and some unit tests, but packaging and all real integration checks must happen on native Windows.
