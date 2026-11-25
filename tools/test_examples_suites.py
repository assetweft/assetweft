import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class ExampleSuitesTest(unittest.TestCase):
    def test_suite_entrypoints_without_cli(self):
        cases = {
            "presentation-bundle": (
                "live_unity_editor_render", "live_unreal_editor_render",
                "live_godot_editor_render", "independent_ta_review_status",
                "optional_bgm_generation", "asr_tts_playback",
            ),
            "non-character-suite": (
                "full_game_runtime_ai", "commercial_asset_ripping",
                "full_dcc_replacement", "mandatory_cloud_account",
                "user_task_observations", "SEPARATE_UNVALIDATED_NOT_RUN",
                "live_engine_viewports",
            ),
        }
        for name, markers in cases.items():
            with self.subTest(suite=name), tempfile.TemporaryDirectory() as tmp:
                directory = ROOT / "examples" / name
                script = directory / f"verify_{name.replace('-', '_')}.sh"
                readme = directory / "README.md"
                self.assertTrue(os.access(script, os.X_OK))
                subprocess.run(["sh", "-n", script], check=True, timeout=1)
                self.assertIn(script.name, readme.read_text(encoding="utf-8"))
                source = script.read_text(encoding="utf-8")
                for marker in (*markers, "NOT_RUN"):
                    self.assertIn(marker, source)
                env = dict(os.environ, TMPDIR=tmp, ASSETWEFT_BIN=str(Path(tmp) / "missing"))
                for args, diagnostic in ((["--invalid"], "usage:"), ([], "not executable")):
                    reply = subprocess.run(
                        [script, *args], env=env, capture_output=True, text=True, timeout=1
                    )
                    self.assertEqual(reply.returncode, 2)
                    self.assertEqual(reply.stdout, "")
                    self.assertIn(diagnostic, reply.stderr)
                self.assertEqual(list(Path(tmp).iterdir()), [])
