from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Any, Callable

from .bootstrap import MODEL_NAME
from .recorder import MIN_RECORDING_MS, RecordingResult

BACKEND = "cuda"
QNN_BACKEND = "qnn"
COMPUTE_TYPE = "int8_float16"
QNN_PROVIDER = "QNNExecutionProvider"
QNN_DEFAULT_PROVIDER_OPTIONS = [{"backend_type": "htp"}]


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

        model = WhisperModel(
            model_source or model_name,
            device=backend,
            compute_type=compute_type,
            download_root=download_root,
            local_files_only=local_files_only,
        )
        return cls(model, backend=backend, model_name=model_name)

    def transcribe(self, recording: RecordingResult) -> TranscriptionResult:
        if recording.duration_ms < MIN_RECORDING_MS or not recording.samples:
            return TranscriptionResult(text=None, backend=self.backend, model=self.model_name)

        audio = self._audio_preparer(recording.samples)
        segments, _info = self._model.transcribe(audio, vad_filter=True)
        text = " ".join(
            segment.text.strip() for segment in segments if getattr(segment, "text", "").strip()
        )
        text = text.strip() or None
        return TranscriptionResult(text=text, backend=self.backend, model=self.model_name)


class QualcommQnnTranscriber:
    backend = QNN_BACKEND

    def __init__(self, *, model_name: str, model_path: str, session: object) -> None:
        self._model_path = model_path
        self._session = session
        self.model_name = model_name

    @classmethod
    def load(
        cls,
        *,
        model_name: str = MODEL_NAME,
        model_source: str | None = None,
        backend: str = QNN_BACKEND,
        _compute_type: str = COMPUTE_TYPE,
        _download_root: str | None = None,
        _local_files_only: bool = False,
    ) -> "QualcommQnnTranscriber":
        if not model_source:
            raise TranscriberError(
                "WHISPER_WINDOWS_QUALCOMM_MODEL_PATH is required for Snapdragon X Elite runtime."
            )
        if backend != QNN_BACKEND:
            raise TranscriberError("Qualcomm QNN loader can only be used with backend='qnn'.")

        try:
            import onnxruntime
        except ImportError as exc:  # pragma: no cover - environment dependent
            raise TranscriberError(
                "onnxruntime-qnn is required for Snapdragon X Elite runtime. "
                "Install with `uv add --project sidecar onnxruntime-qnn`."
            ) from exc

        try:
            session = onnxruntime.InferenceSession(
                model_source,
                providers=[QNN_PROVIDER],
                provider_options=QNN_DEFAULT_PROVIDER_OPTIONS,
                sess_options=onnxruntime.SessionOptions(),
            )
        except Exception as exc:  # pragma: no cover - runtime setup dependent
            raise TranscriberError(
                "Failed to initialize Qualcomm QNN runtime session. "
                "Check that Qualcomm AI Engine Runtime SDK / QAIRT is installed and "
                "the compiled .bin model was exported for Snapdragon X Elite."
            ) from exc

        return cls(model_name=model_name, model_path=model_source, session=session)

    def transcribe(self, recording: RecordingResult) -> TranscriptionResult:
        if recording.duration_ms < MIN_RECORDING_MS or not recording.samples:
            return TranscriptionResult(text=None, backend=self.backend, model=self.model_name)
        raise TranscriberError(
            f"QNN transcription path is scaffolding-only for now; model file {self._model_path} "
            "was discovered and runtime session initializes, but end-to-end token decoding "
            "is not implemented yet in this branch."
        )


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
