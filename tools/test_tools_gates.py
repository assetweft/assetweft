#!/usr/bin/env python3
"""Offline regression probes for the verification wrapper and HEX gate."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
import hex_lint as hex_gate

ROOT = Path(__file__).resolve().parents[1]


class ToolsGates(unittest.TestCase):
    def test_verify_reports_late_failure_and_keeps_full_log(self):
        with TemporaryDirectory() as temp:
            cargo = Path(temp) / "cargo"
            success = "test result: ok. 1 passed; 0 failed\n"
            failure = "test probe_late_failure ... FAILED"
            log = success * 25 + failure + "\n"
            cargo.write_text(f"#!/bin/sh\nprintf '%s' '{log}'\nexit 101\n")
            cargo.chmod(0o755)
            result = subprocess.run(
                [ROOT / "tools/verify.sh", "--crate", "fixture"],
                env={**os.environ, "TMPDIR": temp, "PATH": f"{temp}:{os.environ['PATH']}"},
                capture_output=True, text=True, timeout=10,
            )
            self.assertEqual(result.returncode, 101)
            header = result.stderr.splitlines()[0]
            path = Path(header.split(" log=", 1)[1])
            self.assertEqual(path.read_text(), log)
            self.assertIn(failure, result.stderr)
            self.assertNotIn("0 failed", result.stderr)

    def test_ports_layout_and_boundary(self):
        for context in sorted(hex_gate.CONTEXTS):
            for file in ("ports.rs", "ports/mod.rs", "ports/reader.rs"):
                with self.subTest(context=context, file=file):
                    path = hex_gate.CORE / context / "application" / file
                    role = hex_gate.classify(path)
                    self.assertEqual(role, (context, "ports"))
                    self.assertTrue(hex_gate.check_ref(path, role, ("rusqlite",), None))
            self.assertIsNone(hex_gate.classify(hex_gate.CORE / context / "application/extra/reader.rs"))

    def test_cargo_target_module_roots_and_cfg(self):
        with TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            # Copy the baseline sources so all writer and coverage checks still run.
            shutil.copytree(ROOT / "crates", root / "crates")
            shutil.copyfile(ROOT / "Cargo.toml", root / "Cargo.toml")
            shutil.copyfile(ROOT / "Cargo.lock", root / "Cargo.lock")
            with patch.object(hex_gate, "ROOT", root):
                baseline, _ = hex_gate.scan()
            crate = root / hex_gate.CORE.parent
            for file in ("tests/common/mod.rs", "tests/helper/mod.rs",
                         "tests/alternate/nested/mod.rs",
                         "examples/probe/helper/mod.rs"):
                path = crate / file
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("")
            (crate / "tests/alternate/entry.rs").write_text("mod nested;\n")
            test = crate / "tests/probe.rs"
            test.write_text(
                '#[cfg(feature = "probe")]\nmod common;\n'
                '#[cfg(not(feature = "probe"))]\n#[path = "alternate/entry.rs"]\nmod common;\n'
                'mod helper;\n'
            )
            (crate / "tests/second_probe.rs").write_text(
                '#[cfg(target_os = "macos")]\nmod common;\n'
                '#[cfg(target_os = "linux")]\n#[path = "alternate/entry.rs"]\nmod common;\n'
                'mod helper;\n'
            )
            (crate / "examples/probe/main.rs").write_text("mod helper;\n")
            with patch.object(hex_gate, "ROOT", root):
                self.assertEqual(hex_gate.scan()[0], baseline)
                test.write_text(test.read_text().replace('not(feature = "probe")', 'feature = "other"'))
                findings, _ = hex_gate.scan()
                self.assertTrue(any(f.rule == "HEX-COVERAGE" and "多个声明" in f.reason
                                    for f in findings), findings)

    def test_cfg_module_resolution_checks_every_reexport(self):
        logical = ("catalog", "choice")
        ref = ("crate", *logical, "Selected")
        domain = ("catalog", "domain")
        adapter = ("catalog", "sqlite_store")
        variants = tuple(
            hex_gate.Scope(None, logical, imports={"Selected": ("crate", *role, "Entry")})
            for role in (domain, adapter)
        )
        index = hex_gate.ModuleIndex({logical: variants})
        resolved = {dependency for dependency, _ in hex_gate.resolve_ref(
            ref, hex_gate.Scope(None, ()), index
        )}
        self.assertEqual(resolved, {(*domain, "Entry"), (*adapter, "Entry")})

    def test_diagnostics_are_read_only_but_actual_wrong_writes_fail(self):
        wrong = hex_gate.CORE / "execution/sqlite.rs"
        writes = (
            "INSERT INTO aw_objects VALUES (';')",
            "UPDATE aw_objects SET x = 1",
            "DELETE FROM aw_objects",
            "REPLACE INTO aw_objects VALUES ('x')",
        )
        for sql in writes:
            with self.subTest(sql=sql):
                findings = []
                add = lambda *finding: findings.append(finding)
                for prefix in ("EXPLAIN ", "EXPLAIN /* plan */ QUERY PLAN "):
                    hex_gate.check_sql(wrong, 1, prefix + sql, add, set())
                self.assertEqual(findings, [])
                seen = set()
                hex_gate.check_sql(wrong, 1, f"EXPLAIN {sql};\n{sql};", add, seen)
                self.assertEqual([f[:3] for f in findings], [("HEX-WRITER", wrong, 2)])
                self.assertEqual(seen, set())
                owner = hex_gate.TABLE_WRITERS["aw_objects"]
                hex_gate.check_sql(owner, 1, "EXPLAIN " + sql, add, seen)
                self.assertEqual(seen, set())
                hex_gate.check_sql(owner, 1, sql, add, seen)
                self.assertEqual(seen, {"aw_objects"})


if __name__ == "__main__":
    unittest.main(verbosity=2)
