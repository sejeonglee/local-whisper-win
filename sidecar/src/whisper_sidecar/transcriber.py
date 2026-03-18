from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Any, Callable

from .bootstrap import MODEL_NAME
from .recorder import MIN_RECORDING_MS, RecordingResult

BACKEND = "cuda"
COMPUTE_TYPE = "int8_float16"


class TranscriberError(RuntimeError):
    pass


@dataclass(slots=True)
class TranscriptionResult:
    text: str | None
    backend: str = BACKEND
    model: str = MODEL_NAME


class WhisperTranscriber:
    def __init__(
        self,
        model: Any,
        *,
        backend: str = BACKEND,
        model_name: str = MODEL_NAME,
        audio_preparer: Callable[[list[float]], Any] | None = None,
    ) -> None:
        self._model = model
        self.backend = backend
        self.model_name = model_name
        self._audio_preparer = audio_preparer or prepare_audio_input

    @classmethod
    def load(
        cls,
        *,
        model_name: str = MODEL_NAME,
        backend: str = BACKEND,
        compute_type: str = COMPUTE_TYPE,
    ) -> "WhisperTranscriber":
        try:
            from faster_whisper import WhisperModel
        except ImportError as exc:  # pragma: no cover - environment dependent
            raise TranscriberError(
                "faster-whisper is required for live transcription. Install sidecar dependencies first."
            ) from exc

        model = WhisperModel(model_name, device=backend, compute_type=compute_type)
        return cls(model, backend=backend, model_name=model_name)

    def transcribe(self, recording: RecordingResult) -> TranscriptionResult:
        if recording.duration_ms < MIN_RECORDING_MS or not recording.samples:
            return TranscriptionResult(text=None, backend=self.backend, model=self.model_name)

        audio = self._audio_preparer(recording.samples)
        segments, _info = self._model.transcribe(audio, vad_filter=True)
        text = " ".join(segment.text.strip() for segment in segments if getattr(segment, "text", "").strip())
        text = text.strip() or None
        return TranscriptionResult(text=text, backend=self.backend, model=self.model_name)


def prepare_audio_input(samples: list[float]) -> Any:
    try:
        import numpy as np
    except ImportError as exc:  # pragma: no cover - environment dependent
        raise TranscriberError(
            "numpy is required to prepare live transcription input. Install sidecar dependencies first."
        ) from exc

    return np.asarray(samples, dtype="float32")


class StubTranscriber:
    backend = BACKEND
    model_name = MODEL_NAME

    def transcribe(self, recording: RecordingResult) -> TranscriptionResult:
        scaffold_text = os.environ.get("WHISPER_WINDOWS_STUB_TEXT", "").strip()
        if recording.duration_ms < MIN_RECORDING_MS or not scaffold_text:
            return TranscriptionResult(text=None, backend=self.backend, model=self.model_name)
        return TranscriptionResult(text=scaffold_text, backend=self.backend, model=self.model_name)
