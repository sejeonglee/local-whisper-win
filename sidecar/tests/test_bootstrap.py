import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import whisper_sidecar.bootstrap as bootstrap
from whisper_sidecar.bootstrap import MODEL_NAME, cache_marker_path, ensure_model_ready


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

    def test_qwen_live_runtime_resolves_primary_model_on_8gb_gpu(self) -> None:
        with patch.object(bootstrap, "detect_qwen_gpu_memory", return_value=8 * 1024 * 1024 * 1024):
            selection = bootstrap.resolve_live_selection(bootstrap.ASR_ENGINE_QWEN3)

        self.assertEqual(selection.model, bootstrap.QWEN_PRIMARY_MODEL_NAME)

    def test_qwen_live_runtime_resolves_fallback_model_below_8gb(self) -> None:
        with patch.object(bootstrap, "detect_qwen_gpu_memory", return_value=6 * 1024 * 1024 * 1024):
            selection = bootstrap.resolve_live_selection(bootstrap.ASR_ENGINE_QWEN3)

        self.assertEqual(selection.model, bootstrap.QWEN_FALLBACK_MODEL_NAME)

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
        self.assertEqual(result.model_path, cache_root / bootstrap.ASR_ENGINE_WHISPER / MODEL_NAME)
        cached.assert_called_once_with(
            cache_root,
            engine=bootstrap.ASR_ENGINE_WHISPER,
            model_name=MODEL_NAME,
        )
        download.assert_not_called()

    def test_live_runtime_downloads_before_loading_when_cache_is_missing(self) -> None:
        events: list[str] = []

        def fake_download(emit_event, _cache_dir, selection) -> None:
            emit_event("model_download_started", engine=selection.engine, model=selection.model, total_bytes=42)

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
        self.assertEqual(result.model_path, cache_root / bootstrap.ASR_ENGINE_WHISPER / MODEL_NAME)
        download.assert_called_once()

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
                bootstrap.ResolvedSelection(
                    engine=bootstrap.ASR_ENGINE_WHISPER,
                    model=MODEL_NAME,
                ),
            )

        self.assertEqual(
            events[0],
            (
                "model_download_started",
                {
                    "engine": bootstrap.ASR_ENGINE_WHISPER,
                    "model": MODEL_NAME,
                    "total_bytes": 30,
                },
            ),
        )
        self.assertIn(
            (
                "model_download_progress",
                {
                    "engine": bootstrap.ASR_ENGINE_WHISPER,
                    "model": MODEL_NAME,
                    "received_bytes": 10,
                    "total_bytes": 30,
                },
            ),
            events,
        )
        self.assertEqual(events[-1][0], "model_download_progress")
        self.assertEqual(events[-1][1]["received_bytes"], 30)

    def test_prune_stale_model_dirs_keeps_only_active_selection(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_root = Path(temp_dir)
            active_dir = cache_root / bootstrap.ASR_ENGINE_QWEN3 / "Qwen3-ASR-1.7B"
            stale_same_engine = cache_root / bootstrap.ASR_ENGINE_QWEN3 / "Qwen3-ASR-0.6B"
            stale_other_engine = cache_root / bootstrap.ASR_ENGINE_WHISPER / MODEL_NAME
            unrelated_dir = cache_root / "custom"

            active_dir.mkdir(parents=True)
            stale_same_engine.mkdir(parents=True)
            stale_other_engine.mkdir(parents=True)
            unrelated_dir.mkdir(parents=True)

            bootstrap.prune_stale_model_dirs(
                cache_root,
                bootstrap.ResolvedSelection(
                    engine=bootstrap.ASR_ENGINE_QWEN3,
                    model=bootstrap.QWEN_PRIMARY_MODEL_NAME,
                ),
            )

            self.assertTrue(active_dir.exists())
            self.assertFalse(stale_same_engine.exists())
            self.assertFalse(stale_other_engine.exists())
            self.assertTrue(unrelated_dir.exists())


if __name__ == "__main__":
    unittest.main()
