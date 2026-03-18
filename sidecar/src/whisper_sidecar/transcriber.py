from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Any, Callable

from .bootstrap import (
    ASR_ENGINE_QWEN3,
    ASR_ENGINE_WHISPER,
    BACKEND,
    MODEL_NAME,
    QWEN_PRIMARY_MODEL_NAME,
)
from .recorder import MIN_RECORDING_MS, RecordingResult

COMPUTE_TYPE = "int8_float16"
QWEN_DTYPE = "float16"
QWEN_MAX_INFERENCE_BATCH_SIZE = 1
QWEN_MAX_NEW_TOKENS = 256


class TranscriberError(RuntimeError):
    pass


@dataclass(slots=True)
class TranscriptionResult:
    text: str | None
    engine: str = ASR_ENGINE_WHISPER
    backend: str = BACKEND
    model: str = MODEL_NAME
    language: str | None = None
    timestamps: list[Any] | None = None


class WhisperTranscriber:
    engine = ASR_ENGINE_WHISPER

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
            return TranscriptionResult(
                text=None,
                engine=self.engine,
                backend=self.backend,
                model=self.model_name,
            )

        audio = self._audio_preparer(recording.samples)
        segments, _info = self._model.transcribe(audio, vad_filter=True)
        text = " ".join(segment.text.strip() for segment in segments if getattr(segment, "text", "").strip())
        text = text.strip() or None
        return TranscriptionResult(
            text=text,
            engine=self.engine,
            backend=self.backend,
            model=self.model_name,
            language=extract_whisper_language(_info),
        )


class QwenTranscriber:
    engine = ASR_ENGINE_QWEN3

    def __init__(
        self,
        model: Any,
        *,
        backend: str = BACKEND,
        model_name: str = QWEN_PRIMARY_MODEL_NAME,
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
        model_name: str = QWEN_PRIMARY_MODEL_NAME,
        model_source: str | None = None,
        backend: str = BACKEND,
        dtype: str = QWEN_DTYPE,
        max_inference_batch_size: int = QWEN_MAX_INFERENCE_BATCH_SIZE,
        max_new_tokens: int = QWEN_MAX_NEW_TOKENS,
    ) -> "QwenTranscriber":
        try:
            import torch
            from qwen_asr import Qwen3ASRModel
        except ImportError as exc:  # pragma: no cover - environment dependent
            raise TranscriberError(
                "qwen-asr and torch are required for Qwen3-ASR transcription. Install sidecar dependencies first."
            ) from exc

        model = Qwen3ASRModel.from_pretrained(
            model_source or model_name,
            dtype=resolve_torch_dtype(dtype, torch),
            device_map=resolve_device_map(backend),
            max_inference_batch_size=max_inference_batch_size,
            max_new_tokens=max_new_tokens,
        )
        return cls(model, backend=backend, model_name=model_name)

    def transcribe(self, recording: RecordingResult) -> TranscriptionResult:
        if recording.duration_ms < MIN_RECORDING_MS or not recording.samples:
            return TranscriptionResult(
                text=None,
                engine=self.engine,
                backend=self.backend,
                model=self.model_name,
            )

        audio = self._audio_preparer(recording.samples)
        results = self._model.transcribe(
            audio=(audio, recording.sample_rate),
            language=None,
        )
        result = results[0] if results else None
        text = (getattr(result, "text", None) or "").strip() or None
        language = getattr(result, "language", None)
        timestamps = getattr(result, "time_stamps", None)
        return TranscriptionResult(
            text=text,
            engine=self.engine,
            backend=self.backend,
            model=self.model_name,
            language=language,
            timestamps=timestamps,
        )


def prepare_audio_input(samples: list[float]) -> Any:
    try:
        import numpy as np
    except ImportError as exc:  # pragma: no cover - environment dependent
        raise TranscriberError(
            "numpy is required to prepare live transcription input. Install sidecar dependencies first."
        ) from exc

    return np.asarray(samples, dtype="float32")


def extract_whisper_language(info: Any) -> str | None:
    if isinstance(info, dict):
        language = info.get("language")
        return language if isinstance(language, str) else None
    language = getattr(info, "language", None)
    return language if isinstance(language, str) else None


def resolve_device_map(backend: str) -> str:
    if backend == "cpu":
        return "cpu"
    return f"{backend}:0"


def resolve_torch_dtype(dtype_name: str, torch_module: Any) -> Any:
    normalized = dtype_name.strip().lower()
    if normalized == "float16":
        return torch_module.float16
    if normalized == "bfloat16":
        return torch_module.bfloat16
    raise TranscriberError(f"Unsupported torch dtype '{dtype_name}'.")


class StubTranscriber:
    def __init__(
        self,
        *,
        engine: str = ASR_ENGINE_WHISPER,
        backend: str = BACKEND,
        model_name: str = MODEL_NAME,
    ) -> None:
        self.engine = engine
        self.backend = backend
        self.model_name = model_name

    def transcribe(self, recording: RecordingResult) -> TranscriptionResult:
        scaffold_text = os.environ.get("WHISPER_WINDOWS_STUB_TEXT", "").strip()
        if recording.duration_ms < MIN_RECORDING_MS or not scaffold_text:
            return TranscriptionResult(
                text=None,
                engine=self.engine,
                backend=self.backend,
                model=self.model_name,
            )
        return TranscriptionResult(
            text=scaffold_text,
            engine=self.engine,
            backend=self.backend,
            model=self.model_name,
        )
