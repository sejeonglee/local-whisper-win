from __future__ import annotations

import sys

from .bootstrap import ensure_model_ready
from .ipc import ProtocolError, emit_error, emit_event, iter_commands
from .recorder import StubRecorder
from .transcriber import StubTranscriber


def main() -> int:
    recorder = StubRecorder()
    transcriber = StubTranscriber()

    try:
        ensure_model_ready(emit_event)
    except Exception as exc:  # pragma: no cover - startup guardrail
        emit_error("startup_failed", str(exc), fatal=True)
        return 1

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


if __name__ == "__main__":
    raise SystemExit(main())