import unittest

from whisper_sidecar.recorder import MAX_RECORDING_SECONDS, Recorder, RecorderError


class FakeStream:
    def __init__(self, callback, **kwargs):
        self.callback = callback
        self.kwargs = kwargs
        self.started = False
        self.stopped = False
        self.closed = False

    def start(self) -> None:
        self.started = True

    def stop(self) -> None:
        self.stopped = True

    def close(self) -> None:
        self.closed = True


class RecorderTests(unittest.TestCase):
    def test_start_then_stop_returns_flattened_samples(self) -> None:
        times = iter([10.0, 10.4])
        created_streams: list[FakeStream] = []

        def stream_factory(**kwargs):
            stream = FakeStream(**kwargs)
            created_streams.append(stream)
            return stream

        recorder = Recorder(clock=lambda: next(times), stream_factory=stream_factory)
        recorder.start()
        created_streams[0].callback([[0.1], [0.2], [0.3]], 3, None, None)
        result = recorder.stop()

        self.assertTrue(created_streams[0].started)
        self.assertTrue(created_streams[0].stopped)
        self.assertTrue(created_streams[0].closed)
        self.assertEqual(result.samples, [0.1, 0.2, 0.3])
        self.assertEqual(result.duration_ms, 400)

    def test_double_start_raises_error(self) -> None:
        recorder = Recorder(clock=lambda: 1.0, stream_factory=lambda **kwargs: FakeStream(**kwargs))
        recorder.start()

        with self.assertRaises(RecorderError):
            recorder.start()

    def test_stop_without_start_raises_error(self) -> None:
        recorder = Recorder(clock=lambda: 1.0, stream_factory=lambda **kwargs: FakeStream(**kwargs))

        with self.assertRaises(RecorderError):
            recorder.stop()

    def test_stop_clears_internal_samples_after_success(self) -> None:
        times = iter([10.0, 10.3])
        created_streams: list[FakeStream] = []

        def stream_factory(**kwargs):
            stream = FakeStream(**kwargs)
            created_streams.append(stream)
            return stream

        recorder = Recorder(clock=lambda: next(times), stream_factory=stream_factory)
        recorder.start()
        created_streams[0].callback([[0.1], [0.2]], 2, None, None)

        result = recorder.stop()

        self.assertEqual(result.samples, [0.1, 0.2])
        self.assertEqual(recorder._samples, [])

    def test_duration_guardrail_raises_error_when_recording_runs_too_long(self) -> None:
        times = iter([2.0, 2.0 + MAX_RECORDING_SECONDS + 1])
        recorder = Recorder(clock=lambda: next(times), stream_factory=lambda **kwargs: FakeStream(**kwargs))
        recorder.start()

        with self.assertRaises(RecorderError):
            recorder.stop()
        self.assertEqual(recorder._samples, [])


if __name__ == "__main__":
    unittest.main()
