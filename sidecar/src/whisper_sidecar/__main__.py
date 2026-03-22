from __future__ import annotations

import sys
from typing import Any

from .bootstrap import ASR_ENGINE_QWEN3, BootstrapResult, ensure_model_ready
from .ipc import ProtocolError, configure_stdio, emit_error, emit_event, iter_commands
from .recorder import Recorder, StubRecorder
from .transcriber import QwenTranscriber, StubTranscriber, WhisperTranscriber


def main() -> int:
    configure_stdio()
    try:
        bootstrap_result = ensure_model_ready(emit_event)
        recorder, transcriber = create_runtime(bootstrap_result)
    except Exception as exc:  # pragma: no cover - startup guardrail
        emit_error("startup_failed", str(exc), fatal=True)
        return 1

    emit_event(
        "ready",
        engine=getattr(transcriber, "engine", bootstrap_result.engine),
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
                    payload: dict[str, object] = {
                        "text": result.text,
                        "engine": result.engine,
                        "model": result.model,
                        "backend": result.backend,
                    }
                    if result.language:
                        payload["language"] = result.language
                    emit_event("transcription", **payload)
                else:
                    emit_event(
                        "empty_audio",
                        engine=result.engine,
                        model=result.model,
                        backend=result.backend,
                    )
            elif item.cmd == "shutdown":
                return 0
            else:
                emit_error("unknown_command", f"Unsupported command: {item.cmd}")
        except Exception as exc:  # pragma: no cover - runtime guardrail
            emit_error("runtime_error", str(exc))

    return 0


def create_runtime(bootstrap_result: BootstrapResult) -> tuple[Any, Any]:
    if not bootstrap_result.stub:
        if bootstrap_result.engine == ASR_ENGINE_QWEN3:
            return (
                Recorder(),
                QwenTranscriber.load(
                    model_name=bootstrap_result.model,
                    model_source=str(bootstrap_result.model_path) if bootstrap_result.model_path else None,
                    backend=bootstrap_result.backend,
                ),
            )
        return (
            Recorder(),
            WhisperTranscriber.load(
                model_name=bootstrap_result.model,
                model_source=str(bootstrap_result.model_path) if bootstrap_result.model_path else None,
                backend=bootstrap_result.backend,
            ),
        )

    return (
        StubRecorder(),
        StubTranscriber(
            engine=bootstrap_result.engine,
            model_name=bootstrap_result.model,
            backend=bootstrap_result.backend,
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
