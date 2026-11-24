"""Run the AW015 default-handler test against a private, hash-pinned SQLite copy."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TEST = "application::budget_contention::default_busy_wait"
# Observe only a completed first native sleep. The connection's test trace
# rendezvous lets the winner commit before SQLite resumes its original retry.
SLEEP = b"  sqlite3OsSleep(db->pVfs, delay*1000);"
TRACE = b"""
  if( count==0 && tmout==200 && delay==1 && db->mTrace==SQLITE_TRACE_STMT
      && sqlite3_stmt_busy((sqlite3_stmt*)db->pVdbe) ){
    db->trace.xV2(SQLITE_TRACE_STMT, db->pTraceArg, db->pVdbe,
                  "-- AW015 native sleep returned");
  }"""


def main():
    metadata = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT
    )
    package, = (p for p in json.loads(metadata)["packages"] if p["name"] == "libsqlite3-sys")
    with tempfile.TemporaryDirectory(prefix="aw015-") as tmp:
        source = Path(tmp, "libsqlite3-sys")
        shutil.copytree(Path(package["manifest_path"]).parent, source)
        amalgamation = source / "sqlite3/sqlite3.c"
        original = amalgamation.read_bytes()
        if hashlib.sha256(original).hexdigest() != "0c657666d819f261cc4007526ad57e1669de9949df7a8ea847aa284a2bf0ee52":
            raise ValueError("SQLite source changed: review the sleep observer")
        if original.count(SLEEP) != 1:
            raise ValueError("expected one native timeout sleep site")
        amalgamation.write_bytes(original.replace(SLEEP, SLEEP + TRACE))
        # Cargo's path override preserves the locked graph and the registry cache.
        result = subprocess.run(
            ["cargo", "--config", "paths=" + json.dumps([source.as_posix()]),
             "test", "--locked", "-p", "assetweft-core", "--lib", TEST,
             "--", "--ignored", "--exact", "--nocapture"],
            cwd=ROOT, env={**os.environ, "CARGO_TARGET_DIR": str(Path(tmp, "target"))},
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=600
        )
        print(result.stdout, end="", flush=True)
        result.check_returncode()
        if f"test {TEST} ... ok" not in result.stdout:
            raise RuntimeError("named native wait test did not pass")


if __name__ == "__main__":
    main()
