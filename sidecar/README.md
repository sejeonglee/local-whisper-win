# Whisper Windows Sidecar

This folder contains the Python sidecar that handles local model bootstrap,
audio capture, and transcription for WhisperWindows.

The default runtime is `live`, which uses `faster-whisper`, `numpy`, and
`sounddevice` from the project virtualenv. Set `WHISPER_RUNTIME=scaffold` (or
`WHISPER_WINDOWS_RUNTIME=scaffold`) only when you want to exercise the UI flow
without downloading or loading a real model.

Linux users on Ubuntu/GNOME can set `WHISPER_BACKEND=cpu` to force a CPU-only
runtime when CUDA is not available.
