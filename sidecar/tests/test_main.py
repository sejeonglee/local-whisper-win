from io import StringIO
from pathlib import Path
import unittest
from unittest.mock import patch

from whisper_sidecar.bootstrap import BootstrapResult
from whisper_sidecar.ipc import CommandMessage, ProtocolError
from whisper_sidecar.recorder import RecordingResult
from whisper_sidecar.transcriber import TranscriptionResult
import whisper_sidecar.__main__ as sidecar_main


class FakeRecorder:
    def __init__(self) -> None:
        self.started = False

    def start(self) -> None:
        self.started = True

    def stop(self) -> RecordingResult:
        self.started = False
        return RecordingResult(started_at=1.0, stopped_at=1.5, samples=[0.1, 0.2, 0.3])


class FakeTranscriber:
    backend = "cuda"
    model_name = "large-v3-turbo"

    def transcribe(self, recording: RecordingResult) -> TranscriptionResult:
        return TranscriptionResult(text="dictated text", backend=self.backend, model=self.model_name)


class MainLoopTests(unittest.TestCase):
    def test_main_emits_ready_and_transcription_flow(self) -> None:
        events = []
        recorder = FakeRecorder()
        transcriber = FakeTranscriber()

        with patch.object(
            sidecar_main,
            "ensure_model_ready",
            return_value=BootstrapResult(cache_dir=Path("cache")),
        ) as bootstrap:
            with patch.object(sidecar_main, "create_runtime", return_value=(recorder, transcriber)):
                with patch.object(
                    sidecar_main,
                    "iter_commands",
                    return_value=iter(
                        [
                            CommandMessage(cmd="start_recording"),
                            CommandMessage(cmd="stop_recording"),
                            CommandMessage(cmd="shutdown"),
                        ]
                    ),
                ):
                    with patch.object(sidecar_main, "emit_event", side_effect=lambda event, **payload: events.append((event, payload))):
                        exit_code = sidecar_main.main()

        self.assertEqual(exit_code, 0)
        self.assertTrue(recorder.started is False)
        self.assertEqual(events[0][0], "ready")
        self.assertEqual(events[1][0], "listening")
        self.assertEqual(events[2][0], "transcribing")
        self.assertEqual(events[3], ("transcription", {"text": "dictated text"}))
        bootstrap.assert_called_once()

    def test_protocol_errors_emit_invalid_command_error(self) -> None:
        errors = []

        with patch.object(
            sidecar_main,
            "ensure_model_ready",
            return_value=BootstrapResult(cache_dir=Path("cache")),
        ):
            with patch.object(sidecar_main, "create_runtime", return_value=(FakeRecorder(), FakeTranscriber())):
                with patch.object(sidecar_main, "iter_commands", return_value=iter([ProtocolError("bad command")])):
                    with patch.object(sidecar_main, "emit_event"):
                        with patch.object(
                            sidecar_main,
                            "emit_error",
                            side_effect=lambda code, message, fatal=False: errors.append((code, message, fatal)),
                        ):
                            exit_code = sidecar_main.main()

        self.assertEqual(exit_code, 0)
        self.assertEqual(errors, [("invalid_command", "bad command", False)])


if __name__ == "__main__":
    unittest.main()
