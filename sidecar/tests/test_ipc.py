from io import StringIO
import unittest

from whisper_sidecar.ipc import ProtocolError, emit_event, parse_command_line


class IpcTests(unittest.TestCase):
    def test_emit_event_writes_json_line(self) -> None:
        stream = StringIO()
        emit_event("ready", stream=stream, model="large-v3-turbo")
        self.assertIn('"event": "ready"', stream.getvalue())

    def test_parse_command_line_accepts_valid_input(self) -> None:
        message = parse_command_line('{"type":"command","version":1,"cmd":"start_recording"}')
        self.assertEqual(message.cmd, "start_recording")

    def test_parse_command_line_rejects_invalid_version(self) -> None:
        with self.assertRaises(ProtocolError):
            parse_command_line('{"type":"command","version":99,"cmd":"start_recording"}')


if __name__ == "__main__":
    unittest.main()