import unittest

from whisper_sidecar.recorder import RecordingResult
from whisper_sidecar.transcriber import MODEL_NAME, QualcommQnnTranscriber, TranscriberError, WhisperTranscriber


class Segment:
    def __init__(self, text: str) -> None:
        self.text = text


class FakeModel:
    def __init__(self) -> None:
        self.calls = []

    def transcribe(self, audio, vad_filter=True):
        self.calls.append((audio, vad_filter))
        return [Segment("안녕하세요"), Segment(" world ")], {"language": "ko"}


class TranscriberTests(unittest.TestCase):
    def test_short_recording_returns_empty_result_without_calling_model(self) -> None:
        model = FakeModel()
        transcriber = WhisperTranscriber(model, audio_preparer=lambda samples: samples)
        recording = RecordingResult(
            started_at=1.0,
            stopped_at=1.1,
            samples=[0.1, 0.2],
        )

        result = transcriber.transcribe(recording)

        self.assertIsNone(result.text)
        self.assertEqual(model.calls, [])

    def test_transcribe_joins_segments_and_exposes_metadata(self) -> None:
        model = FakeModel()
        transcriber = WhisperTranscriber(model, backend="cuda", model_name=MODEL_NAME, audio_preparer=lambda samples: samples)
        recording = RecordingResult(
            started_at=1.0,
            stopped_at=1.5,
            samples=[0.1, 0.2, 0.3],
        )

        result = transcriber.transcribe(recording)

        self.assertEqual(result.text, "안녕하세요 world")
        self.assertEqual(result.backend, "cuda")
        self.assertEqual(result.model, MODEL_NAME)
        self.assertEqual(model.calls, [([0.1, 0.2, 0.3], True)])

    def test_qualcomm_transcriber_load_requires_model_path(self) -> None:
        with self.assertRaises(TranscriberError):
            QualcommQnnTranscriber.load(model_source=None)


if __name__ == "__main__":
    unittest.main()
