# Whisper Windows — Design Document

## Overview

A Windows desktop dictation app that replaces the built-in Windows speech recognition. Uses local GPU (RTX 4050 6GB VRAM) with faster-whisper for low-latency Korean/English mixed transcription.

## User Flow

1. App starts → Python sidecar launches → model loads to GPU (warm state)
2. Tray icon shows "Ready"
3. User presses Ctrl+H → overlay shows "Listening..." → recording starts
4. User speaks (Korean/English/Konglish)
5. User presses Ctrl+H again → recording stops
6. VAD trims silence from audio edges
7. faster-whisper transcribes → text result
8. Text pasted into active window via clipboard → clipboard restored
9. Overlay returns to idle

## Architecture (3-layer)

```
┌─────────────────────────────────────────────┐
│            Tauri v2 Shell (Rust)             │
│  - System tray (tray-icon plugin)           │
│  - Global hotkey (global-shortcut plugin)    │
│  - Window management (overlay)              │
│  - Sidecar lifecycle (shell plugin)         │
└──────────────┬──────────────────────────────┘
               │ Tauri commands / events
┌──────────────▼──────────────────────────────┐
│          Web UI (React + TypeScript)         │
│  - Small overlay: "Listening / Transcribing" │
│  - Settings page (hotkey config)             │
│  - Status indicators                         │
└──────────────────────────────────────────────┘

┌──────────────────────────────────────────────┐
│       Python Sidecar (.exe via PyInstaller)   │
│  - faster-whisper (large-v3-turbo, int8)     │
│  - Audio capture (sounddevice, 16kHz mono)   │
│  - Silero VAD (silence trimming)             │
│  - Clipboard paste (text injection)          │
│  - IPC: stdio JSON lines with Tauri shell    │
│  - Resident process (model loaded once)      │
└──────────────────────────────────────────────┘
```

## IPC Protocol (stdio JSON lines)

```json
// Tauri → Python
{"cmd": "start_recording"}
{"cmd": "stop_recording"}
{"cmd": "shutdown"}

// Python → Tauri
{"event": "ready", "model": "large-v3-turbo", "device": "cuda"}
{"event": "status", "state": "listening"}
{"event": "status", "state": "transcribing"}
{"event": "transcription", "text": "안녕하세요"}
{"event": "error", "message": "..."}
```

## Key Decisions

| Decision | Choice | Rationale |
|---|---|---|
| UI framework | Tauri v2 + React + TypeScript | Low memory (30-50MB), first-class sidecar, small binary |
| STT engine | faster-whisper (CTranslate2) | Best perf/VRAM ratio for local inference |
| Model | large-v3-turbo | 809M params, ~1.5GB int8 VRAM, good ko/en |
| Quantization | int8_float16 | Fits in 6GB VRAM with headroom |
| Target GPU | RTX 4050 Laptop (6GB VRAM) | Ada Lovelace, CUDA 12 compatible |
| IPC | stdio JSON lines | Simple, no network, no firewall issues |
| Sidecar packaging | PyInstaller → .exe | Tauri requires external binary per target triple |
| Hotkey | User-configurable (default Ctrl+H) | Ctrl+H is collision-prone |
| Trigger mode | Toggle (press to start, press to stop) | User preference |
| Text injection | Clipboard paste (save/restore) | Simple, works everywhere |
| Audio | 16kHz mono float32 (sounddevice) | Whisper's native input format |
| VAD | Silero VAD | Best accuracy for speech/silence boundary |
| Language | Auto-detect (ko/en mixed) | Supports Korean + English + Konglish |
| Dev tooling | uv + mise (Python), pnpm (frontend), cargo (Rust) | User preference |

## Project Structure

```
whisper-windows/
├── .mise.toml                  # Python + Node + Rust version pinning
├── src-tauri/                  # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   └── src/
│       └── main.rs
├── src/                        # React frontend
│   ├── App.tsx
│   ├── components/
│   └── ...
├── sidecar/                    # Python STT process
│   ├── .python-version
│   ├── pyproject.toml          # managed by uv
│   ├── src/
│   │   └── whisper_sidecar/
│   │       ├── __init__.py
│   │       ├── __main__.py     # entry point (stdio IPC loop)
│   │       ├── recorder.py     # sounddevice audio capture + VAD
│   │       ├── transcriber.py  # faster-whisper wrapper
│   │       └── injector.py     # clipboard paste into active window
│   └── tests/
├── package.json
├── tsconfig.json
└── docs/plans/
```

## GPU Requirements

- GPU: NVIDIA RTX 4050 Laptop (6GB VRAM, Ada Lovelace)
- CUDA: 12.x
- cuDNN: 9.x
- faster-whisper/CTranslate2 must be built against matching CUDA version
- Model VRAM usage: ~1.5GB (int8), ~2.5GB (fp16)
- Remaining VRAM available for OS/other apps

## Performance Expectations

- Short dictation (1-3 seconds): sub-second transcription latency expected
- Model cold start: 3-5 seconds (one-time at app launch)
- Audio processing overhead: negligible (VAD + trimming)
- Total UX latency target: < 1 second from stop-recording to text-inserted

## Known Risks & Mitigations

1. **Global hotkey reliability**: Tauri plugin has open bugs → fallback to raw Win32 RegisterHotKey via Rust
2. **CUDA/cuDNN packaging**: Pin exact versions, test on clean Windows install
3. **WebView2 dependency**: Use Tauri's embedded bootstrapper for installer
4. **Overlay "show without activating"**: Prototype early, may need Rust-side window flags
5. **PyInstaller binary size**: faster-whisper + CUDA can produce large .exe → consider Nuitka or ship Python separately

## MVP Scope

- Toggle hotkey → record → transcribe → clipboard paste
- Tray icon with status (ready/listening/transcribing)
- Small always-on-top overlay showing state
- Resident Python sidecar with model pre-loaded on GPU
- User-configurable hotkey

## Out of MVP

- Streaming/real-time transcription
- Settings GUI beyond hotkey config
- Cloud API fallback
- Auto-update mechanism
- Cross-platform support
- Multiple language profile presets
