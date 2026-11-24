#!/usr/bin/env python3

import json
import os
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
FORBIDDEN = {
    "sqlx", "tokio-postgres", "postgres", "deadpool-postgres", "libpq", "pq-sys",
    "ollama", "llama-cpp", "ort", "candle", "tch", "pyo3",
}
DISCLOSURES = (
    "#99", "#142", "#28", "Windows", "Linux", "macOS", "Godot", "NOT_RUN",
    "AW-032:C", "AW-029", "AW-030", "AW-004:B", "AW-012:A", "AW-032:B",
    "AW-033:B", "AW-034:A", "AW-035:A", "library backup", "library restore", "SQLite",
)
DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies",
                     "dev_dependencies", "build_dependencies")
SOURCE_SUFFIXES = {".rs", ".js", ".ts", ".jsx", ".tsx", ".mjs", ".cjs", ".sh"}
BUILD_DIRS = {"node_modules", "target", "dist"}
PYTHON = r'(?:python3?(?:\.\d+)?)(?:\.exe)?'
EXECUTABLE = rf'(?:[^"\'\n]*[/\\])?{PYTHON}'
PYTHON_LAUNCH = re.compile(
    rf'\b(?:new|spawn(?:Sync)?|exec(?:File)?(?:Sync)?|shell|command)'
    rf'\s*\(\s*(?:r\#*)?["\'](?:env\s+)?{EXECUTABLE}(?=["\'\s])'
    rf'|(?:^|[;&|])\s*(?:exec\s+|env\s+)*{EXECUTABLE}(?=\s|$)',
    re.MULTILINE | re.IGNORECASE,
)


def files(root, directory):
    for parent, dirs, names in os.walk(root / directory):
        dirs[:] = sorted(set(dirs) - BUILD_DIRS)
        for name in sorted(names):
            yield Path(parent) / name


def cargo_names(manifest):
    package = manifest.get("package", {})
    if "name" in package:
        yield package["name"]
    for scope in (manifest, manifest.get("workspace", {}), *manifest.get("target", {}).values()):
        for table in DEPENDENCY_TABLES:
            for name, spec in scope.get(table, {}).items():
                yield name
                if isinstance(spec, dict) and "package" in spec:
                    yield spec["package"]


def npm_names(manifest):
    if "name" in manifest:
        yield manifest["name"]
    for table in ("dependencies", "devDependencies", "peerDependencies", "optionalDependencies"):
        for name, spec in manifest.get(table, {}).items():
            yield name
            if spec.startswith("npm:"):
                yield spec[4:].rsplit("@", 1)[0]


def findings(root):
    workbench = list(files(root, "apps/workbench"))
    manifests = [root / "Cargo.toml", *sorted(root.glob("crates/*/Cargo.toml"))]
    manifests.extend(path for path in workbench if path.name == "Cargo.toml")
    locks = [root / "Cargo.lock", *(path for path in workbench if path.name == "Cargo.lock")]
    for path in [*manifests, *locks, *(path for path in workbench if path.name == "package.json")]:
        raw = path.read_text(encoding="utf-8")
        if path.name == "package.json":
            names = npm_names(json.loads(raw))
        else:
            cargo = tomllib.loads(raw)
            names = (package["name"] for package in cargo["package"]) if path.name == "Cargo.lock" else cargo_names(cargo)
        for name in sorted(set(names)):
            package = name.rsplit("/", 1)[-1].lower().replace("_", "-")
            if any(package == banned or package.startswith(banned + "-") for banned in FORBIDDEN):
                yield f"{path.relative_to(root)}: forbidden package {name}; use the local SQLite/Core runtime or an optional BYO adapter"

    # A static guard for literal launch sites. Computed commands still require review.
    for path in (*files(root, "crates"), *workbench):
        relative = path.relative_to(root)
        if {"examples", "tests"}.intersection(relative.parts[:-1]):
            continue
        if relative.parts[0] == "apps" and relative.parts[2] not in {"src", "src-tauri"}:
            continue
        if path.suffix in SOURCE_SUFFIXES:
            source = path.read_text(encoding="utf-8")
            for match in PYTHON_LAUNCH.finditer(source):
                line = source.count("\n", 0, match.start()) + 1
                yield f"{relative}:{line}: Python launcher {match[0].strip()}; keep Python in optional scripts"

    readme = (root / "README.md").read_bytes()
    if not re.search(rb"\]\((?:\./)?docs/TECH_PREVIEW\.md\)", readme):
        yield "README.md: add a Markdown link to docs/TECH_PREVIEW.md"
    for path, content, limit in (
        ("README.md", readme, 40),
        ("docs/FEATURE_MAP.md", (root / "docs/FEATURE_MAP.md").read_bytes(), 150),
    ):
        if content.count(b"\n") > limit:
            yield f"{path}: reduce to at most {limit} lines"

    path = "docs/TECH_PREVIEW.md"
    preview = (root / path).read_text(encoding="utf-8")
    for disclosure in DISCLOSURES:
        if disclosure not in preview:
            yield f"{path}: restore disclosure {disclosure}"
    for anchor in ("#99", "Godot", "GPU"):
        if not any(anchor in line and "NOT_RUN" in line for line in preview.splitlines()):
            yield f"{path}: disclose NOT_RUN for {anchor}"
    for prefix in ("/Users/", "/Volumes/", "C:\\"):
        if prefix.casefold() in preview.casefold():
            yield f"{path}: remove machine-local path ({prefix})"


def check(root):
    try:
        errors = list(findings(root))
    except (OSError, ValueError) as error:
        print(f"tech-preview: cannot read repository inputs: {error}", file=sys.stderr)
        return 1
    for error in errors:
        print(f"tech-preview: {error}", file=sys.stderr)
    return int(bool(errors))


if __name__ == "__main__":
    sys.exit(check(ROOT))
