from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Any, Callable

from .bootstrap import MODEL_NAME
from .recorder import MIN_RECORDING_MS, RecordingResult

BACKEND = os.environ.get("WHISPER_BACKEND", os.environ.get("WHISPER_WINDOWS_BACKEND", "auto")).strip().lower() or "auto"
COMPUTE_TYPE = os.environ.get("WHISPER_COMPUTE_TYPE", "int8_float16")


class TranscriberError(RuntimeError):
    pass


@dataclass(slots=True)
class TranscriptionResult:
    text: str | None
    backend: str
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
        model_source: str | None = None,
        backend: str = BACKEND,
        compute_type: str = COMPUTE_TYPE,
        download_root: str | None = None,
        local_files_only: bool = False,
    ) -> "WhisperTranscriber":
        try:
            from faster_whisper import WhisperModel
        except ImportError as exc:  # pragma: no cover - environment dependent
            raise TranscriberError(
                "faster-whisper is required for live transcription. Install sidecar dependencies first."
            ) from exc

        requested_backend = backend.strip().lower() if backend else "auto"
        backend_candidates = [requested_backend] if requested_backend != "auto" else ["cuda", "cpu"]

        last_error: Exception | None = None
        for selected_backend in backend_candidates:
            try:
                model = WhisperModel(
                    model_source or model_name,
                    device=selected_backend,
                    compute_type=compute_type,
                    download_root=download_root,
                    local_files_only=local_files_only,
                )
                return cls(model, backend=selected_backend, model_name=model_name)
            except Exception as exc:  # pragma: no cover - runtime dependent
                last_error = exc
                continue

        raise TranscriberError(f"failed to initialize WhisperModel with backend candidates {backend_candidates}: {last_error}")

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
    backend = "scaffold"
    model_name = MODEL_NAME

    def transcribe(self, recording: RecordingResult) -> TranscriptionResult:
        scaffold_text = os.environ.get("WHISPER_WINDOWS_STUB_TEXT", "").strip()
        if recording.duration_ms < MIN_RECORDING_MS or not scaffold_text:
            return TranscriptionResult(text=None, backend=self.backend, model=self.model_name)
        return TranscriptionResult(text=scaffold_text, backend=self.backend, model=self.model_name)
