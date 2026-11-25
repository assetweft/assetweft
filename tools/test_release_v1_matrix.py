from pathlib import Path
import re
import unittest
from urllib.parse import urlsplit


ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/RELEASE_V1_MATRIX.md"


class ReleaseV1MatrixTest(unittest.TestCase):
    def test_release_traceability_and_viewport_contract(self):
        matrix = MATRIX.read_text(encoding="utf-8")
        cards = re.findall(r"^\| (AW-\d{3}) \|", matrix, re.MULTILINE)
        self.assertEqual(cards, [f"AW-{number:03d}" for number in range(1, 41)])

        items = (
            "G-DET-1", "Q6", "EngineTransformContract", "Q1/E08/E11",
            "HY4-P0-02", "Q8", "V1 Non-Goals + BM-01..BM-06", "R13",
        )
        for number, name in enumerate(items, 1):
            self.assertRegex(matrix, rf"(?m)^\| {number}\. {re.escape(name)}(?:[ |])")
        for issue in (99, 28):
            self.assertRegex(matrix, rf"(?m)^- #{issue} .+")

        for boundary in (
            "live Unity editor", "live Unreal editor", "live Godot editor",
            "live paid GPU/TTS providers", "installer code-signing/notarization",
            "Windows/Linux native viewport captures", "real TA/user observations",
        ):
            self.assertTrue(
                any(boundary in line and "NOT_RUN" in line for line in matrix.splitlines()),
                boundary,
            )

        feature_map = ROOT / "docs/FEATURE_MAP.md"
        feature_text = feature_map.read_text(encoding="utf-8")
        self.assertLessEqual(len(feature_text.splitlines()), 150)
        self.assertIn("](RELEASE_V1_MATRIX.md)", feature_text)
        for path in (MATRIX, feature_map, Path(__file__)):
            self.assertNotRegex(path.read_text(encoding="utf-8"), r"(?i)/(?:Users|Volumes)/|[a-z]:\\")

        links = re.findall(r"\]\(([^)\s]+)\)|(?m:^\[[^]]+\]:[ \t]+(\S+))", matrix)
        self.assertTrue(links)
        for inline, reference in links:
            destination = inline or reference
            url = urlsplit(destination)
            if not url.scheme:
                target = (MATRIX.parent / url.path).resolve()
                self.assertTrue(target.is_relative_to(ROOT) and target.is_file(), destination)

        viewport_sources = {
            "examples/desktop-viewport/src/main.ts": (
                "new THREE.WebGLRenderer", "scene.traverse", "o.geometry.dispose()",
                "m.dispose()", "renderer.dispose()", "invoke<ArrayBuffer>('load_fixture'",
            ),
            "examples/desktop-viewport/src-tauri/src/main.rs": ("tauri::ipc::Response::new(bytes)",),
            "apps/workbench/src/ui/patterns/preview-stage.ts": (
                "new WebGLRenderer", "root.traverse", "material.dispose()",
                "texture.dispose()", "geometry.dispose()", "renderer.dispose()",
                "Uint8Array.from(atob(preview.glbBase64)", "parseAsync(bytes.buffer",
            ),
        }
        for relative, markers in viewport_sources.items():
            source = (ROOT / relative).read_text(encoding="utf-8")
            for marker in markers:
                with self.subTest(file=relative, marker=marker):
                    self.assertIn(marker, source)
