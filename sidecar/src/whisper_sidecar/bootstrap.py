from __future__ import annotations

import json
import os
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

MODEL_NAME = "large-v3-turbo"
BACKEND = "cuda"
SCAFFOLD_TOTAL_BYTES = 64 * 1024 * 1024


@dataclass(slots=True)
class BootstrapResult:
    cache_dir: Path
    model: str = MODEL_NAME
    backend: str = BACKEND
    stub: bool = True


def default_cache_dir() -> Path:
    local_app_data = os.environ.get("LOCALAPPDATA")
    if local_app_data:
        return Path(local_app_data) / "WhisperWindows" / "models"
    return Path.home() / ".cache" / "WhisperWindows" / "models"


def cache_marker_path(cache_dir: Path) -> Path:
    return cache_dir / MODEL_NAME / "scaffold-cache.json"


def is_cached(cache_dir: Path) -> bool:
    marker = cache_marker_path(cache_dir)
    if not marker.exists():
        return False

    try:
        data = json.loads(marker.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return False

    return data.get("model") == MODEL_NAME and data.get("stub") is True


def ensure_model_ready(emit_event: Callable[..., None], cache_dir: Path | None = None) -> BootstrapResult:
    resolved_cache_dir = cache_dir or Path(os.environ.get("WHISPER_WINDOWS_MODEL_CACHE", default_cache_dir()))
    marker = cache_marker_path(resolved_cache_dir)

    emit_event("starting")

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

    emit_event("loading_model", backend=BACKEND)
    time.sleep(0.1)
    emit_event("ready", model=MODEL_NAME, backend=BACKEND, bootstrap_mode="scaffold")

    return BootstrapResult(cache_dir=resolved_cache_dir)