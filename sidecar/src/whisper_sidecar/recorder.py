from __future__ import annotations

from dataclasses import dataclass, field
import time
from typing import Any, Callable

SAMPLE_RATE = 16_000
CHANNELS = 1
MAX_RECORDING_SECONDS = 30
MIN_RECORDING_MS = 200


class RecorderError(RuntimeError):
    pass


@dataclass(slots=True)
class RecordingResult:
    started_at: float
    stopped_at: float
    samples: list[float] = field(default_factory=list)
    sample_rate: int = SAMPLE_RATE

    @property
    def duration_ms(self) -> int:
        return int((self.stopped_at - self.started_at) * 1000)


class Recorder:
    def __init__(
        self,
        *,
        clock: Callable[[], float] = time.time,
        stream_factory: Callable[..., Any] | None = None,
        max_recording_seconds: int = MAX_RECORDING_SECONDS,
    ) -> None:
        self._clock = clock
        self._stream_factory = stream_factory or default_stream_factory
        self._max_recording_seconds = max_recording_seconds
        self._started_at: float | None = None
        self._stream: Any | None = None
        self._samples: list[float] = []

    def start(self) -> None:
        if self._stream is not None:
            raise RecorderError("Recorder is already active")

        self._samples = []
        self._started_at = self._clock()

        try:
            stream = self._stream_factory(
                callback=self._handle_audio,
                samplerate=SAMPLE_RATE,
                channels=CHANNELS,
                dtype="float32",
            )
            stream.start()
        except Exception as exc:  # pragma: no cover - exercised through integration/runtime
            self._started_at = None
            raise RecorderError(f"Microphone capture failed to start: {exc}") from exc

        self._stream = stream

    def stop(self) -> RecordingResult:
        if self._stream is None or self._started_at is None:
            raise RecorderError("Recorder was not active")

        started_at = self._started_at
        stream = self._stream
        self._started_at = None
        self._stream = None
        samples: list[float] = []

        try:
            stream.stop()
        finally:
            try:
                stream.close()
            finally:
                samples = self._samples
                self._samples = []

        stopped_at = self._clock()
        if stopped_at - started_at > self._max_recording_seconds:
            raise RecorderError(
                f"Recording exceeded the {self._max_recording_seconds}-second safety limit"
            )

        return RecordingResult(
            started_at=started_at,
            stopped_at=stopped_at,
            samples=samples,
        )

    def _handle_audio(self, indata: Any, _frames: int, _time_info: Any, status: Any) -> None:
        if status:
            raise RecorderError(f"Microphone stream reported an error: {status}")
        self._samples.extend(flatten_samples(indata))


def default_stream_factory(**kwargs: Any) -> Any:
    try:
        import sounddevice as sounddevice
    except ImportError as exc:  # pragma: no cover - environment dependent
        raise RecorderError(
            "sounddevice is required for live microphone capture. Install sidecar dependencies first."
        ) from exc

    return sounddevice.InputStream(**kwargs)


def flatten_samples(indata: Any) -> list[float]:
    if hasattr(indata, "reshape"):
        reshaped = indata.reshape(-1)
        if hasattr(reshaped, "tolist"):
            return [float(value) for value in reshaped.tolist()]

    if isinstance(indata, (list, tuple)):
        flattened: list[float] = []
        for item in indata:
            if isinstance(item, (list, tuple)):
                flattened.extend(float(value) for value in item)
            else:
                flattened.append(float(item))
        return flattened

    return [float(indata)]


class StubRecorder:
    def __init__(self) -> None:
        self._started_at: float | None = None

    def start(self) -> None:
        if self._started_at is not None:
            raise RecorderError("Recorder is already active")
        self._started_at = time.time()

    def stop(self) -> RecordingResult:
        if self._started_at is None:
            raise RecorderError("Recorder was not active")

        started_at = self._started_at
        self._started_at = None
        return RecordingResult(started_at=started_at, stopped_at=time.time(), samples=[])
