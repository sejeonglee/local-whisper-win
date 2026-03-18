from __future__ import annotations

import os
from dataclasses import dataclass

from .recorder import RecordingResult


@dataclass(slots=True)
class TranscriptionResult:
    text: str | None
    backend: str = "cuda"


class StubTranscriber:
    def transcribe(self, recording: RecordingResult) -> TranscriptionResult:
        scaffold_text = os.environ.get("WHISPER_WINDOWS_STUB_TEXT", "").strip()
        if recording.duration_ms < 200 or not scaffold_text:
            return TranscriptionResult(text=None)
        return TranscriptionResult(text=scaffold_text)