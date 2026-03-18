from __future__ import annotations

import sys
from typing import Any

from .bootstrap import BootstrapResult, ensure_model_ready
from .ipc import ProtocolError, emit_error, emit_event, iter_commands
from .recorder import Recorder, StubRecorder
from .transcriber import StubTranscriber, WhisperTranscriber


def main() -> int:
    try:
        bootstrap_result = ensure_model_ready(emit_event)
        recorder, transcriber = create_runtime(bootstrap_result)
    except Exception as exc:  # pragma: no cover - startup guardrail
        emit_error("startup_failed", str(exc), fatal=True)
        return 1

    emit_event(
        "ready",
        model=getattr(transcriber, "model_name", bootstrap_result.model),
        backend=getattr(transcriber, "backend", bootstrap_result.backend),
        bootstrap_mode="scaffold" if bootstrap_result.stub else "live",
    )

    for item in iter_commands(sys.stdin):
        if isinstance(item, ProtocolError):
            emit_error("invalid_command", str(item))
            continue

        try:
            if item.cmd == "start_recording":
                recorder.start()
                emit_event("listening")
            elif item.cmd == "stop_recording":
                recording = recorder.stop()
                emit_event("transcribing")
                result = transcriber.transcribe(recording)
                if result.text:
                    emit_event("transcription", text=result.text)
                else:
                    emit_event("empty_audio")
            elif item.cmd == "shutdown":
                return 0
            else:
                emit_error("unknown_command", f"Unsupported command: {item.cmd}")
        except Exception as exc:  # pragma: no cover - runtime guardrail
            emit_error("runtime_error", str(exc))

    return 0


def create_runtime(bootstrap_result: BootstrapResult) -> tuple[Any, Any]:
    if not bootstrap_result.stub:
        return (
            Recorder(),
            WhisperTranscriber.load(
                model_name=bootstrap_result.model,
                backend=bootstrap_result.backend,
                download_root=str(bootstrap_result.cache_dir),
                local_files_only=True,
            ),
        )

    return StubRecorder(), StubTranscriber()


if __name__ == "__main__":
    raise SystemExit(main())
