from __future__ import annotations

import os
import sys

__version__ = "0.1.0"
__all__ = ["__version__"]


def _normalize_verbatim_windows_path(value: str) -> str:
    if os.name != "nt":
        return value
    prefix = "\\\\?\\"
    if value.startswith(prefix):
        return value[len(prefix):]
    return value


def _normalize_windows_paths() -> None:
    if os.name != "nt":
        return

    pythonpath = os.environ.get("PYTHONPATH")
    if pythonpath is not None:
        os.environ["PYTHONPATH"] = os.pathsep.join(
            _normalize_verbatim_windows_path(item)
            for item in pythonpath.split(os.pathsep)
            if item
        )

    for index, value in enumerate(sys.path):
        sys.path[index] = _normalize_verbatim_windows_path(value)


_normalize_windows_paths()
