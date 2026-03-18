from __future__ import annotations

import json
import os
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

ASR_ENGINE_WHISPER = "whisper"
ASR_ENGINE_QWEN3 = "qwen3"
DEFAULT_ASR_ENGINE = ASR_ENGINE_WHISPER

WHISPER_MODEL_NAME = "large-v3-turbo"
MODEL_NAME = WHISPER_MODEL_NAME
WHISPER_MODEL_REPOSITORY = "mobiuslabsgmbh/faster-whisper-large-v3-turbo"
MODEL_REPOSITORY = WHISPER_MODEL_REPOSITORY
WHISPER_MODEL_ALLOW_PATTERNS = [
    "config.json",
    "preprocessor_config.json",
    "model.bin",
    "tokenizer.json",
    "vocabulary.*",
]
MODEL_ALLOW_PATTERNS = WHISPER_MODEL_ALLOW_PATTERNS
QWEN_PRIMARY_MODEL_NAME = "Qwen/Qwen3-ASR-1.7B"
QWEN_FALLBACK_MODEL_NAME = "Qwen/Qwen3-ASR-0.6B"
QWEN_VRAM_THRESHOLD_BYTES = 8 * 1024 * 1024 * 1024
BACKEND = "cuda"
SCAFFOLD_TOTAL_BYTES = 64 * 1024 * 1024


class BootstrapError(RuntimeError):
    pass


@dataclass(slots=True)
class ResolvedSelection:
    engine: str
    model: str
    backend: str = BACKEND
    total_vram_bytes: int | None = None


@dataclass(slots=True)
class BootstrapResult:
    cache_dir: Path
    engine: str = DEFAULT_ASR_ENGINE
    model: str = MODEL_NAME
    backend: str = BACKEND
    stub: bool = True
    model_path: Path | None = None


def default_cache_dir() -> Path:
    local_app_data = os.environ.get("LOCALAPPDATA")
    if local_app_data:
        return Path(local_app_data) / "WhisperWindows" / "models"
    return Path.home() / ".cache" / "WhisperWindows" / "models"


def normalize_engine(value: str | None) -> str:
    normalized = (value or DEFAULT_ASR_ENGINE).strip().lower()
    if normalized in {ASR_ENGINE_WHISPER, ASR_ENGINE_QWEN3}:
        return normalized
    return DEFAULT_ASR_ENGINE


def configured_engine() -> str:
    return normalize_engine(os.environ.get("WHISPER_WINDOWS_ASR_ENGINE"))


def default_model_for_engine(engine: str) -> str:
    if engine == ASR_ENGINE_QWEN3:
        return QWEN_PRIMARY_MODEL_NAME
    return WHISPER_MODEL_NAME


def model_dir_name(model_name: str) -> str:
    return model_name.rsplit("/", 1)[-1]


def cache_marker_path(
    cache_dir: Path,
    *,
    engine: str = DEFAULT_ASR_ENGINE,
    model_name: str | None = None,
) -> Path:
    resolved_model = model_name or default_model_for_engine(engine)
    return live_model_dir(cache_dir, engine=engine, model_name=resolved_model) / "scaffold-cache.json"


def runtime_mode() -> str:
    return os.environ.get("WHISPER_WINDOWS_RUNTIME", "live").strip().lower()


def is_cached(
    cache_dir: Path,
    *,
    engine: str = DEFAULT_ASR_ENGINE,
    model_name: str | None = None,
) -> bool:
    resolved_model = model_name or default_model_for_engine(engine)
    marker = cache_marker_path(cache_dir, engine=engine, model_name=resolved_model)
    if not marker.exists():
        return False

    try:
        data = json.loads(marker.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return False

    return (
        data.get("engine") == engine
        and data.get("model") == resolved_model
        and data.get("stub") is True
    )


def resolve_model_repository(model_name: str = MODEL_NAME) -> str:
    if "/" in model_name:
        return model_name
    if model_name == WHISPER_MODEL_NAME:
        return WHISPER_MODEL_REPOSITORY
    raise BootstrapError(f"Unsupported model repository lookup for '{model_name}'.")


def huggingface_snapshot_download(**kwargs: object) -> object:
    try:
        from huggingface_hub import snapshot_download
    except ImportError as exc:  # pragma: no cover - environment dependent
        raise BootstrapError(
            "huggingface_hub is required for live model bootstrap. Install sidecar dependencies first."
        ) from exc

    return snapshot_download(**kwargs)


def live_model_dir(
    cache_dir: Path,
    engine: str = DEFAULT_ASR_ENGINE,
    model_name: str = MODEL_NAME,
) -> Path:
    return cache_dir / engine / model_dir_name(model_name)


def has_required_whisper_model_files(model_dir: Path) -> bool:
    required_files = [
        model_dir / "config.json",
        model_dir / "preprocessor_config.json",
        model_dir / "model.bin",
        model_dir / "tokenizer.json",
    ]
    return all(path.exists() for path in required_files) and any(model_dir.glob("vocabulary.*"))


def has_required_qwen_model_files(model_dir: Path) -> bool:
    required_files = [
        model_dir / "config.json",
        model_dir / "preprocessor_config.json",
        model_dir / "tokenizer_config.json",
        model_dir / "vocab.json",
        model_dir / "merges.txt",
    ]
    has_weights = any(model_dir.glob("model-*.safetensors")) or (model_dir / "model.safetensors").exists()
    return all(path.exists() for path in required_files) and has_weights


def has_required_model_files(
    model_dir: Path,
    *,
    engine: str,
    model_name: str,
) -> bool:
    if engine == ASR_ENGINE_QWEN3:
        return has_required_qwen_model_files(model_dir)
    return has_required_whisper_model_files(model_dir)


def is_live_model_cached(
    cache_dir: Path,
    *,
    engine: str = DEFAULT_ASR_ENGINE,
    model_name: str = MODEL_NAME,
) -> bool:
    return has_required_model_files(
        live_model_dir(cache_dir, engine=engine, model_name=model_name),
        engine=engine,
        model_name=model_name,
    )


def model_download_kwargs(cache_dir: Path, selection: ResolvedSelection) -> dict[str, object]:
    kwargs: dict[str, object] = {
        "repo_id": resolve_model_repository(selection.model),
        "local_dir": str(live_model_dir(cache_dir, engine=selection.engine, model_name=selection.model)),
    }
    if selection.engine == ASR_ENGINE_WHISPER:
        kwargs["allow_patterns"] = WHISPER_MODEL_ALLOW_PATTERNS
    return kwargs


def live_download_plan(cache_dir: Path, selection: ResolvedSelection) -> tuple[int, bool]:
    plan = huggingface_snapshot_download(
        **model_download_kwargs(cache_dir, selection),
        dry_run=True,
    )
    pending = [item for item in plan if getattr(item, "will_download", False)]
    total_bytes = sum(int(getattr(item, "file_size", 0)) for item in pending)
    return total_bytes, bool(pending)


def download_live_model(
    emit_event: Callable[..., None],
    cache_dir: Path,
    selection: ResolvedSelection,
) -> None:
    total_bytes, has_pending_files = live_download_plan(cache_dir, selection)
    if not has_pending_files:
        return

    emit_event(
        "model_download_started",
        engine=selection.engine,
        model=selection.model,
        total_bytes=total_bytes,
    )

    try:
        from tqdm.auto import tqdm as tqdm_base
    except ImportError:  # pragma: no cover - system Python test path
        class tqdm_base:  # type: ignore[no-redef]
            def __init__(self, *args, **kwargs):
                self.n = kwargs.get("initial", 0)

            def update(self, n=1):
                self.n += n
                return None

            def close(self):
                return None

    class DownloadProgressTqdm(tqdm_base):
        def __init__(self, *args, **kwargs):
            self._is_bytes_progress = kwargs.get("name") == "huggingface_hub.snapshot_download"
            self._last_reported = -1
            self._tracked_bytes = int(kwargs.get("initial", 0))
            super().__init__(*args, **kwargs)

        def _emit(self, *, force: bool = False) -> None:
            if not self._is_bytes_progress:
                return

            received = min(max(int(self.n), self._tracked_bytes), total_bytes)
            if not force and received == self._last_reported:
                return

            emit_event(
                "model_download_progress",
                engine=selection.engine,
                model=selection.model,
                received_bytes=received,
                total_bytes=total_bytes,
            )
            self._last_reported = received

        def update(self, n=1):
            self._tracked_bytes += int(n)
            value = super().update(n)
            self._emit()
            return value

        def close(self):
            self._emit(force=True)
            return super().close()

    download_kwargs = model_download_kwargs(cache_dir, selection)
    download_kwargs["tqdm_class"] = DownloadProgressTqdm
    huggingface_snapshot_download(**download_kwargs)


def ensure_live_model_ready(
    emit_event: Callable[..., None],
    cache_dir: Path,
    selection: ResolvedSelection,
) -> Path:
    model_dir = live_model_dir(cache_dir, engine=selection.engine, model_name=selection.model)
    if not is_live_model_cached(cache_dir, engine=selection.engine, model_name=selection.model):
        download_live_model(emit_event, cache_dir, selection)
    return model_dir


def detect_qwen_gpu_memory() -> int:
    try:
        import torch
    except ImportError as exc:  # pragma: no cover - environment dependent
        raise BootstrapError(
            "Qwen3-ASR requires PyTorch. Install a CUDA-enabled torch build in the sidecar environment before selecting Qwen3-ASR."
        ) from exc

    if not torch.cuda.is_available():
        raise BootstrapError(
            "Qwen3-ASR requires a CUDA-enabled PyTorch installation on Windows. Install a CUDA-enabled torch build in the sidecar environment before selecting Qwen3-ASR."
        )

    return int(torch.cuda.get_device_properties(0).total_memory)


def resolve_live_selection(engine: str) -> ResolvedSelection:
    if engine == ASR_ENGINE_WHISPER:
        return ResolvedSelection(engine=engine, model=WHISPER_MODEL_NAME)

    total_vram_bytes = detect_qwen_gpu_memory()
    resolved_model = (
        QWEN_PRIMARY_MODEL_NAME
        if total_vram_bytes >= QWEN_VRAM_THRESHOLD_BYTES
        else QWEN_FALLBACK_MODEL_NAME
    )
    return ResolvedSelection(
        engine=engine,
        model=resolved_model,
        total_vram_bytes=total_vram_bytes,
    )


def scaffold_selection(engine: str) -> ResolvedSelection:
    return ResolvedSelection(engine=engine, model=default_model_for_engine(engine))


def ensure_model_ready(emit_event: Callable[..., None], cache_dir: Path | None = None) -> BootstrapResult:
    resolved_cache_dir = cache_dir or Path(os.environ.get("WHISPER_WINDOWS_MODEL_CACHE", default_cache_dir()))
    selected_runtime = runtime_mode()
    selected_engine = configured_engine()
    selection = (
        scaffold_selection(selected_engine)
        if selected_runtime == "scaffold"
        else resolve_live_selection(selected_engine)
    )
    marker = cache_marker_path(
        resolved_cache_dir,
        engine=selection.engine,
        model_name=selection.model,
    )
    model_path: Path | None = None

    emit_event("starting", engine=selection.engine)

    if selected_runtime == "scaffold":
        if not is_cached(
            resolved_cache_dir,
            engine=selection.engine,
            model_name=selection.model,
        ):
            marker.parent.mkdir(parents=True, exist_ok=True)
            emit_event(
                "model_download_started",
                engine=selection.engine,
                model=selection.model,
                total_bytes=SCAFFOLD_TOTAL_BYTES,
            )

            for step in range(1, 6):
                time.sleep(0.15)
                emit_event(
                    "model_download_progress",
                    engine=selection.engine,
                    model=selection.model,
                    received_bytes=(SCAFFOLD_TOTAL_BYTES * step) // 5,
                    total_bytes=SCAFFOLD_TOTAL_BYTES,
                )

            marker.write_text(
                json.dumps(
                    {
                        "engine": selection.engine,
                        "model": selection.model,
                        "stub": True,
                        "created_at": time.time(),
                    },
                    indent=2,
                ),
                encoding="utf-8",
            )
    else:
        model_path = ensure_live_model_ready(emit_event, resolved_cache_dir, selection)

    emit_event(
        "loading_model",
        engine=selection.engine,
        model=selection.model,
        backend=selection.backend,
    )
    if selected_runtime == "scaffold":
        time.sleep(0.1)

    return BootstrapResult(
        cache_dir=resolved_cache_dir,
        engine=selection.engine,
        model=selection.model,
        backend=selection.backend,
        stub=selected_runtime == "scaffold",
        model_path=model_path,
    )
