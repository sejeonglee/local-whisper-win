# Whisper Windows Sidecar

This folder contains the Python sidecar that handles local model bootstrap, audio
capture, and transcription for WhisperWindows.

The default runtime is `live`, which uses `faster-whisper`, `numpy`, and
`sounddevice` from the project virtualenv. Set `WHISPER_WINDOWS_RUNTIME=scaffold`
only when you want to exercise the UI flow without downloading or loading a real
model.

## Qualcomm Snapdragon X Elite (QNN) Runtime

This project keeps the existing `faster-whisper` pipeline unchanged and adds an optional Qualcomm path:

- Set `WHISPER_WINDOWS_BACKEND=qnn` (or run on ARM64 so `auto` selects QNN by default).
- Set `WHISPER_WINDOWS_QUALCOMM_MODEL_PATH` to the downloaded optimized model binary, for example:
  `whisper_large_v3_turbo-hfwhisperdecoder-qualcomm_snapdragon_x_elite.bin`.
- Ensure `onnxruntime-qnn` is present and the Qualcomm AI Engine Runtime SDK (QAIRT) is available in the runtime environment.

Model reference:

- [Qualcomm AI Hub: Whisper-Large-V3-Turbo](https://aihub.qualcomm.com/models/whisper_large_v3_turbo)
- [ONNX Runtime QNN Execution Provider](https://onnxruntime.ai/docs/execution-providers/QNN-ExecutionProvider.html)

Example for a direct QNN smoke test:

```powershell
$env:WHISPER_WINDOWS_BACKEND = "qnn"
$env:WHISPER_WINDOWS_QUALCOMM_MODEL_PATH = "$PWD\\tmp\\whisper_large_v3_turbo-hfwhisperdecoder-qualcomm_snapdragon_x_elite.bin"
npm run tauri:dev
```
