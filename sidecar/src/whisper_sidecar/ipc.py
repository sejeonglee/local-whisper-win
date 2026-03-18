from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from typing import Iterator, TextIO

VERSION = 1


class ProtocolError(ValueError):
    """Raised when the JSON-line protocol receives malformed input."""


@dataclass(slots=True)
class CommandMessage:
    cmd: str


def configure_stdio() -> None:
    for stream_name in ("stdout", "stderr"):
        stream = getattr(sys, stream_name, None)
        reconfigure = getattr(stream, "reconfigure", None)
        if callable(reconfigure):
            reconfigure(encoding="utf-8", errors="backslashreplace", newline="\n")


def emit(payload: dict[str, object], stream: TextIO = sys.stdout) -> None:
    stream.write(json.dumps(payload, ensure_ascii=False) + "\n")
    stream.flush()


def emit_event(event: str, stream: TextIO = sys.stdout, **payload: object) -> None:
    emit({"type": "event", "version": VERSION, "event": event, **payload}, stream=stream)


def emit_error(code: str, message: str, *, fatal: bool = False, stream: TextIO = sys.stdout) -> None:
    emit_event("fatal" if fatal else "error", code=code, message=message, stream=stream)


def parse_command_line(raw_line: str) -> CommandMessage:
    try:
        data = json.loads(raw_line)
    except json.JSONDecodeError as exc:
        raise ProtocolError(f"Invalid JSON input: {exc.msg}") from exc

    if data.get("type") != "command":
        raise ProtocolError("Command message must have type='command'")
    if data.get("version") != VERSION:
        raise ProtocolError(f"Unsupported protocol version: {data.get('version')!r}")

    cmd = data.get("cmd")
    if not isinstance(cmd, str) or not cmd:
        raise ProtocolError("Command message must include a non-empty string 'cmd'")

    return CommandMessage(cmd=cmd)


def iter_commands(stream: TextIO = sys.stdin) -> Iterator[CommandMessage | ProtocolError]:
    for raw_line in stream:
        line = raw_line.strip()
        if not line:
            continue
        try:
            yield parse_command_line(line)
        except ProtocolError as exc:
            yield exc
