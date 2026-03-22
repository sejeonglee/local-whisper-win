import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import whisper_sidecar.bootstrap as bootstrap
from whisper_sidecar.bootstrap import (
    BACKEND_CUDA,
    BACKEND_QNN,
    MODEL_NAME,
    QUALCOMM_MODEL_FILE_NAME,
    cache_marker_path,
    ensure_model_ready,
    resolve_backend,
    resolve_default_backend,
)


class BootstrapTests(unittest.TestCase):
    def test_first_run_creates_cache_marker(self) -> None:
        events: list[str] = []
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_root = Path(temp_dir)
            with patch.dict("os.environ", {"WHISPER_WINDOWS_RUNTIME": "scaffold"}):
                ensure_model_ready(lambda event, **payload: events.append(event), cache_dir=cache_root)
            marker = cache_marker_path(cache_root)
            self.assertTrue(marker.exists())
            self.assertIn("model_download_started", events)
            self.assertIn("loading_model", events)

    def test_marker_path_points_to_model_specific_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            marker = cache_marker_path(Path(temp_dir))
            self.assertIn(MODEL_NAME, str(marker))

    def test_live_runtime_skips_download_when_model_is_already_cached(self) -> None:
        events: list[str] = []

        with tempfile.TemporaryDirectory() as temp_dir:
            cache_root = Path(temp_dir)
            with patch.dict("os.environ", {"WHISPER_WINDOWS_RUNTIME": "live"}):
                with patch.object(bootstrap, "is_live_model_cached", return_value=True) as cached:
                    with patch.object(bootstrap, "download_live_model") as download:
                        result = ensure_model_ready(
                            lambda event, **payload: events.append(event),
                            cache_dir=cache_root,
                        )

        self.assertEqual(events, ["starting", "loading_model"])
        self.assertFalse(result.stub)
        self.assertEqual(result.model_path, cache_root / MODEL_NAME)
        cached.assert_called_once_with(cache_root)
        download.assert_not_called()

    def test_live_runtime_downloads_before_loading_when_cache_is_missing(self) -> None:
        events: list[str] = []

        def fake_download(emit_event, _cache_dir, model_name=MODEL_NAME) -> None:
            emit_event("model_download_started", model=model_name, total_bytes=42)

        with tempfile.TemporaryDirectory() as temp_dir:
            cache_root = Path(temp_dir)
            with patch.dict("os.environ", {"WHISPER_WINDOWS_RUNTIME": "live"}):
                with patch.object(bootstrap, "is_live_model_cached", return_value=False):
                    with patch.object(bootstrap, "download_live_model", side_effect=fake_download) as download:
                        result = ensure_model_ready(
                            lambda event, **payload: events.append(event),
                            cache_dir=cache_root,
                        )

        self.assertEqual(events, ["starting", "model_download_started", "loading_model"])
        self.assertFalse(result.stub)
        self.assertEqual(result.model_path, cache_root / MODEL_NAME)
        download.assert_called_once()

    def test_live_runtime_resolves_qnn_model_from_env_path(self) -> None:
        events: list[tuple[str, dict]] = []
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_root = Path(temp_dir)
            qnn_model = cache_root / QUALCOMM_MODEL_FILE_NAME
            qnn_model.write_bytes(b"")
            with patch.dict(
                "os.environ",
                {
                    "WHISPER_WINDOWS_RUNTIME": "live",
                    "WHISPER_WINDOWS_BACKEND": BACKEND_QNN,
                    "WHISPER_WINDOWS_QUALCOMM_MODEL_PATH": str(qnn_model),
                },
            ):
                result = ensure_model_ready(
                    lambda event, **payload: events.append((event, payload)),
                    cache_dir=cache_root,
                )

        self.assertEqual(result.backend, BACKEND_QNN)
        self.assertEqual(result.model_path, qnn_model)
        self.assertEqual(events[0][0], "starting")
        self.assertEqual(events[1][0], "qualcomm_model_found")
        self.assertEqual(events[2][0], "loading_model")
        self.assertEqual(events[2][1]["backend"], BACKEND_QNN)

    def test_default_backend_auto_maps_to_cuda_on_amd64_host(self) -> None:
        with patch.object(bootstrap.platform, "machine", return_value="AMD64"):
            self.assertEqual(resolve_backend(), BACKEND_CUDA)

    def test_default_backend_auto_maps_to_qnn_on_arm64_host(self) -> None:
        with patch.object(bootstrap.platform, "machine", return_value="aarch64"):
            self.assertEqual(resolve_default_backend(), BACKEND_QNN)

    def test_download_live_model_emits_byte_progress_from_snapshot_download(self) -> None:
        events: list[tuple[str, dict]] = []

        def fake_snapshot_download(**kwargs):
            if kwargs.get("dry_run"):
                return [
                    SimpleNamespace(file_size=12, will_download=False),
                    SimpleNamespace(file_size=30, will_download=True),
                ]

            progress = kwargs["tqdm_class"](
                total=30,
                initial=0,
                disable=True,
                name="huggingface_hub.snapshot_download",
            )
            progress.update(10)
            progress.update(20)
            progress.close()
            return "cache/snapshot"

        with patch.object(bootstrap, "huggingface_snapshot_download", side_effect=fake_snapshot_download):
            bootstrap.download_live_model(
                lambda event, **payload: events.append((event, payload)),
                Path("cache"),
            )

        self.assertEqual(
            events[0],
            ("model_download_started", {"model": MODEL_NAME, "total_bytes": 30}),
        )
        self.assertIn(
            ("model_download_progress", {"model": MODEL_NAME, "received_bytes": 10, "total_bytes": 30}),
            events,
        )
        self.assertEqual(events[-1][0], "model_download_progress")
        self.assertEqual(events[-1][1]["received_bytes"], 30)


if __name__ == "__main__":
    unittest.main()
