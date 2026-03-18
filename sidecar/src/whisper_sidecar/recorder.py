from __future__ import annotations

from dataclasses import dataclass
import time


@dataclass(slots=True)
class RecordingResult:
    started_at: float
    stopped_at: float
    audio_bytes: bytes

    @property
    def duration_ms(self) -> int:
        return int((self.stopped_at - self.started_at) * 1000)


class StubRecorder:
    def __init__(self) -> None:
        self._started_at: float | None = None

    def start(self) -> None:
        if self._started_at is not None:
            raise RuntimeError("Recorder is already active")
        self._started_at = time.time()

    def stop(self) -> RecordingResult:
        if self._started_at is None:
            raise RuntimeError("Recorder was not active")

        started_at = self._started_at
        self._started_at = None
        return RecordingResult(started_at=started_at, stopped_at=time.time(), audio_bytes=b"")