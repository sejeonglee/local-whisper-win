# WhisperWindows

WhisperWindows is a Typeless-inspired, local-first Windows dictation app. It stays resident in the tray, listens for a global `Ctrl+H` hotkey, records a short utterance, runs local Whisper transcription through a Python sidecar, pastes the result back into the previously focused app, and restores the original clipboard contents.

The current MVP originally targeted native Windows x86_64 with an NVIDIA GPU. Primary speech target is mixed Korean and English dictation.

## What Is Implemented

- Fixed global hotkey: `Ctrl+H`
- Toggle flow: press once to start recording, press again to stop
- Resident Python sidecar kept warm after startup
- Local transcription with `faster-whisper`
- First-run model download with cache reuse on later launches
- Floating overlay with startup, download, listening, transcribing, ready, and error states
- Tray icon with status-aware actions
- Clipboard-safe paste back into the previously focused window

## What Windows Appear

- A small borderless overlay window appears during startup and dictation states.
- The overlay is transparent, always-on-top, and hidden from the taskbar.
- A tray icon appears so the app can stay resident even when the overlay is hidden.
- The release build should not open a browser tab or a `127.0.0.1` dev window. That path is only for `tauri dev`.
- Errors are shown in the same overlay and tray status flow instead of opening a separate settings window.

## Model And Runtime

- Speech-to-text runs through `faster-whisper` on the Python sidecar.
- The model used by the live runtime is `large-v3-turbo`.
- The current model source is the Hugging Face repository `mobiuslabsgmbh/faster-whisper-large-v3-turbo`.
- Model files are not bundled into the installer.
- On first launch, the app downloads the model into `%LOCALAPPDATA%\\WhisperWindows\\models\\large-v3-turbo`.
- Later launches reuse the cached model.

## Native Windows Development Setup

You need these tools installed on the build machine:

- Node.js 22
- Rust toolchain with `rustup`
- Python 3.12
- `uv`

Once those are available, run this from PowerShell in the repository root:

```powershell
npm install
uv sync --project sidecar
npm run tauri:dev
```

Notes:

- `uv sync --project sidecar` creates `sidecar/.venv`, which is the Python environment used by the sidecar during development.
- The app is designed to be validated on native Windows. WSL is fine for editing and some tests, but hotkey, tray, clipboard, audio, and packaging checks should happen on Windows itself.

## Ubuntu/GNOME Development Setup

Install:

- Node.js 22
- Rust toolchain with `rustup`
- Python 3.12
- `uv`
- (For automatic paste support) `wl-clipboard` and `xdotool` (when available)

Then from the project root:

```bash
npm install
uv sync --project sidecar
npm run tauri:dev
```

### Running on Linux

Once dependencies are installed:

```bash
npm run tauri:dev
```

If you want to test clipboard + overlay behavior without downloading model files, use:

```bash
WHISPER_RUNTIME=scaffold WHISPER_WINDOWS_STUB_TEXT="안녕하세요" npm run tauri:dev
```

Notes:

- The app should now run as a tray + floating overlay app on Ubuntu and attempt to use `xdotool` for automation.
- If `xdotool` is not available, the sidecar still places the transcript into the clipboard so you can manually paste.
- Clipboard paths and caches are Linux-aware (for Ubuntu: `${XDG_CACHE_HOME:-~/.cache}/WhisperWindows/models` by default).

## Useful Runtime Flags

For a fast UI smoke test without downloading or loading the real model:

```powershell
$env:WHISPER_WINDOWS_RUNTIME = "scaffold"
npm run tauri:dev
```

For a file-based debug log:

```powershell
$env:WHISPER_WINDOWS_DEBUG_LOG = "$PWD\\tmp\\whisperwindows-debug.log"
npm run tauri:dev
```

## Packaging And Build

To build platform-native installers/distributions on the current OS:

```powershell
npm run tauri:build
```

That command does:

- Builds the React frontend
- Stages a portable Python runtime into `sidecar/.python-runtime`
- Bundles the sidecar source, Python runtime, and Python site-packages into the Tauri release output

Expected artifacts:

- `src-tauri/target/release/bundle/nsis/WhisperWindows_0.1.0_x64-setup.exe`
- `src-tauri/target/release/bundle/msi/WhisperWindows_0.1.0_x64_en-US.msi`
- On Ubuntu, the same command generates Linux bundle artifacts such as `.deb`/`.rpm`/`.appimage` depending on your toolchain.

If `WHISPER_BACKEND` is set to `cpu` or `auto`, Ubuntu builds that do not have CUDA can still run transcription.

If you want to smoke-test the packaged app layout without running the installer UI, you can also run:

```powershell
src-tauri\target\release\whisper-windows.exe
```

## Installation Flow For End Users

1. Build the installers with `npm run tauri:build`, or download a previously built installer artifact.
2. Run either the NSIS `.exe` installer or the MSI package.
3. Launch WhisperWindows.
4. Wait through the first-run model download and model load.
5. Press `Ctrl+H` to start dictation, speak, then press `Ctrl+H` again to paste the transcription back into the previously focused app.

## Validation Status

These checks have been confirmed on native Windows:

- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `sidecar\\.venv\\Scripts\\python.exe -m unittest discover -s sidecar\\tests`
- `npm run tauri:build`
- Release executable smoke test from `src-tauri/target/release/whisper-windows.exe`
- Bundled sidecar runtime resolution from `src-tauri/target/release/sidecar`
- Live dictation end-to-end on Windows with Korean transcription pasted back into a target window

A separate clean-machine installer pass is still recommended before broader distribution.

## Architecture In One Sentence

Rust/Tauri owns the Windows-facing shell, tray, hotkey, clipboard, and paste behavior; Python owns model bootstrap, audio capture, and transcription; React only mirrors app state in the overlay.
