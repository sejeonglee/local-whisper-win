import tempfile
import unittest
from pathlib import Path

from whisper_sidecar.bootstrap import MODEL_NAME, cache_marker_path, ensure_model_ready


class BootstrapTests(unittest.TestCase):
    def test_first_run_creates_cache_marker(self) -> None:
        events: list[str] = []
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_root = Path(temp_dir)
            ensure_model_ready(lambda event, **payload: events.append(event), cache_dir=cache_root)
            marker = cache_marker_path(cache_root)
            self.assertTrue(marker.exists())
            self.assertIn("model_download_started", events)
            self.assertIn("loading_model", events)

    def test_marker_path_points_to_model_specific_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            marker = cache_marker_path(Path(temp_dir))
            self.assertIn(MODEL_NAME, str(marker))


if __name__ == "__main__":
    unittest.main()
