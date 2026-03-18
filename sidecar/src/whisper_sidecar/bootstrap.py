from __future__ import annotations

import json
import os
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

MODEL_NAME = "large-v3-turbo"
MODEL_REPOSITORY = "mobiuslabsgmbh/faster-whisper-large-v3-turbo"
MODEL_ALLOW_PATTERNS = [
    "config.json",
    "preprocessor_config.json",
    "model.bin",
    "tokenizer.json",
    "vocabulary.*",
]
BACKEND = "cuda"
SCAFFOLD_TOTAL_BYTES = 64 * 1024 * 1024


class BootstrapError(RuntimeError):
    pass


@dataclass(slots=True)
class BootstrapResult:
    cache_dir: Path
    model: str = MODEL_NAME
    backend: str = BACKEND
    stub: bool = True
    model_path: Path | None = None


def default_cache_dir() -> Path:
    local_app_data = os.environ.get("LOCALAPPDATA")
    if local_app_data:
        return Path(local_app_data) / "WhisperWindows" / "models"
    return Path.home() / ".cache" / "WhisperWindows" / "models"


def cache_marker_path(cache_dir: Path) -> Path:
    return cache_dir / MODEL_NAME / "scaffold-cache.json"


def runtime_mode() -> str:
    return os.environ.get("WHISPER_WINDOWS_RUNTIME", "live").strip().lower()


def is_cached(cache_dir: Path) -> bool:
    marker = cache_marker_path(cache_dir)
    if not marker.exists():
        return False

    try:
        data = json.loads(marker.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return False

    return data.get("model") == MODEL_NAME and data.get("stub") is True


def resolve_model_repository(model_name: str = MODEL_NAME) -> str:
    if "/" in model_name:
        return model_name
    if model_name == MODEL_NAME:
        return MODEL_REPOSITORY
    raise BootstrapError(f"Unsupported model repository lookup for '{model_name}'.")


def huggingface_snapshot_download(**kwargs: object) -> object:
    try:
        from huggingface_hub import snapshot_download
    except ImportError as exc:  # pragma: no cover - environment dependent
        raise BootstrapError(
            "huggingface_hub is required for live model bootstrap. Install sidecar dependencies first."
        ) from exc

    return snapshot_download(**kwargs)


def live_model_dir(cache_dir: Path, model_name: str = MODEL_NAME) -> Path:
    return cache_dir / model_name


def has_required_model_files(model_dir: Path) -> bool:
    required_files = [
        model_dir / "config.json",
        model_dir / "preprocessor_config.json",
        model_dir / "model.bin",
        model_dir / "tokenizer.json",
    ]
    return all(path.exists() for path in required_files) and any(model_dir.glob("vocabulary.*"))


def is_live_model_cached(cache_dir: Path, model_name: str = MODEL_NAME) -> bool:
    return has_required_model_files(live_model_dir(cache_dir, model_name))


def live_download_plan(cache_dir: Path, model_name: str = MODEL_NAME) -> tuple[int, bool]:
    plan = huggingface_snapshot_download(
        repo_id=resolve_model_repository(model_name),
        local_dir=str(live_model_dir(cache_dir, model_name)),
        allow_patterns=MODEL_ALLOW_PATTERNS,
        dry_run=True,
    )
    pending = [item for item in plan if getattr(item, "will_download", False)]
    total_bytes = sum(int(getattr(item, "file_size", 0)) for item in pending)
    return total_bytes, bool(pending)


def download_live_model(
    emit_event: Callable[..., None],
    cache_dir: Path,
    model_name: str = MODEL_NAME,
) -> None:
    total_bytes, has_pending_files = live_download_plan(cache_dir, model_name)
    if not has_pending_files:
        return

    emit_event("model_download_started", model=model_name, total_bytes=total_bytes)

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
                model=model_name,
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

    huggingface_snapshot_download(
        repo_id=resolve_model_repository(model_name),
        local_dir=str(live_model_dir(cache_dir, model_name)),
        allow_patterns=MODEL_ALLOW_PATTERNS,
        tqdm_class=DownloadProgressTqdm,
    )


def ensure_live_model_ready(emit_event: Callable[..., None], cache_dir: Path) -> Path:
    model_dir = live_model_dir(cache_dir)
    if not is_live_model_cached(cache_dir):
        download_live_model(emit_event, cache_dir)
    return model_dir


def ensure_model_ready(emit_event: Callable[..., None], cache_dir: Path | None = None) -> BootstrapResult:
    resolved_cache_dir = cache_dir or Path(os.environ.get("WHISPER_WINDOWS_MODEL_CACHE", default_cache_dir()))
    marker = cache_marker_path(resolved_cache_dir)
    selected_runtime = runtime_mode()
    model_path: Path | None = None

    emit_event("starting")

    if selected_runtime == "scaffold":
        if not is_cached(resolved_cache_dir):
            marker.parent.mkdir(parents=True, exist_ok=True)
            emit_event("model_download_started", model=MODEL_NAME, total_bytes=SCAFFOLD_TOTAL_BYTES)

            for step in range(1, 6):
                time.sleep(0.15)
                emit_event(
                    "model_download_progress",
                    model=MODEL_NAME,
                    received_bytes=(SCAFFOLD_TOTAL_BYTES * step) // 5,
                    total_bytes=SCAFFOLD_TOTAL_BYTES,
                )

            marker.write_text(
                json.dumps(
                    {
                        "model": MODEL_NAME,
                        "stub": True,
                        "created_at": time.time(),
                    },
                    indent=2,
                ),
                encoding="utf-8",
            )
    else:
        model_path = ensure_live_model_ready(emit_event, resolved_cache_dir)

    emit_event("loading_model", backend=BACKEND)
    if selected_runtime == "scaffold":
        time.sleep(0.1)

    return BootstrapResult(
        cache_dir=resolved_cache_dir,
        stub=selected_runtime == "scaffold",
        model_path=model_path,
    )
