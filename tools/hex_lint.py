#!/usr/bin/env python3
"""Check AssetWeft's Rust boundaries and CI comment rules."""
import hashlib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import NamedTuple

ROOT = Path(__file__).resolve().parents[1]
CORE = Path("crates/assetweft-core/src")
CLI = Path("crates/assetweft-cli/src")
CONTEXTS = {"assurance", "catalog", "consumption", "execution", "production"}
# Composition is confined to these established roots. New application modules
# inside a context are subject to the application layer rule.
CORE_ROOT = {
    "lib": ("shared", "domain"),
    "application": ("shared", "composition"),
    "application/budget_contention": ("consumption", "test"),
    "library": ("shared", "adapter"),
    "sqlite": ("shared", "adapter"),
    "sqlite/acceptance": ("production", "adapter"),
    "sqlite/initialization": ("production", "adapter"),
    "storage": ("shared", "adapter"),
    "motion": ("motion", "adapter"),
    "motion/components": ("motion", "adapter"),
    "motion/stance": ("motion", "adapter"),
    "backup": ("catalog", "facade"),
    "history": ("catalog", "facade"),
    "budget": ("consumption", "facade"),
    "candidate": ("production", "facade"),
    "generation_spec": ("production", "facade"),
    "workflow": ("production", "facade"),
    "executor": ("execution", "facade"),
    "executor_runtime": ("execution", "facade"),
    "geometry": ("assurance", "facade"),
    "rig": ("assurance", "facade"),
    "validation": ("assurance", "facade"),
}
NAMED_LAYERS = {
    "assurance": {
        "checks": "adapter", "file_input": "adapter", "gltf_checks": "adapter",
        "geometry_gltf": "adapter", "rig_gltf": "adapter", "rig_blender": "adapter",
        "rig_blender/landmarks": "adapter", "rig_blender/process": "adapter",
    },
    "catalog": {
        "feedback": "application", "exports": "adapter", "glb_preview": "adapter",
        "preview": "adapter", "wave": "adapter",
    },
    "consumption": {},
    "execution": {
        "byo_gpu/mod": "boundary", "byo_gpu/policy": "domain",
        "byo_gpu/process": "adapter", "consumption_adapter": "adapter",
    },
    "production": {"rigged_simple": "domain", "specification": "domain"},
}
EDGE_PREFIXES = {
    ("execution", "consumption"): ("consumption::domain", "consumption::ports"),
    ("production", "catalog"): ("catalog::domain", "catalog::objects_domain", "catalog::ports"),
    ("production", "assurance"): ("assurance::domain", "assurance::rig_blender::RIGGED_SIMPLE"),
}
ADAPTER_EDGES = {
    "production/staging_sqlite.rs": (
        "catalog::sqlite_metadata::asset_version", "catalog::sqlite_metadata::component_references",
        "catalog::objects_application::managed", "catalog::objects_sqlite::SqliteObjects",
        "assurance::checks", "assurance::file_input::read_bounded",
    ),
    "sqlite/acceptance.rs": (
        "catalog::sqlite_metadata::asset_version", "catalog::sqlite_store::current",
        "catalog::sqlite_store::advance",
    ),
    "sqlite/initialization.rs": (
        "catalog::sqlite_metadata::asset_version", "catalog::sqlite_store::initialize",
    ),
}
WRITERS = {
    "catalog/sqlite_metadata": "aw_assets aw_asset_versions aw_component_references",
    "catalog/objects_sqlite": "aw_objects aw_managed_versions",
    "catalog/sqlite_store": "aw_candidate_current",
    "catalog/sqlite_feedback": "aw_feedback",
    "production/workflow_sqlite": "aw_workflow_runs aw_workflow_steps aw_workflow_dependencies",
    "production/staging_sqlite": "aw_candidates",
    "sqlite/acceptance": "aw_candidate_attempts",
    "consumption/sqlite": "aw_budget_accounts aw_budget_intents aw_budget_receipts",
}
TABLE_WRITERS = {
    table: CORE / f"{owner}.rs" for owner, tables in WRITERS.items() for table in tables.split()
}
INFRA_WRITERS = {
    "aw_schema_version": CORE / "library.rs",
    "parent": CORE / "sqlite.rs",
    "child": CORE / "sqlite.rs",
}
FIX = {
    "HEX-COVERAGE": "在所属上下文注册模块和层，或采用该上下文的 domain/application/ports/SQLite 命名规则",
    "HEX-LAYER": "把 IO、SDK 和事务放入适配器，由用例消费窄端口，端口参数使用业务值",
    "HEX-CONTEXT": "改用契约允许的领域值或窄端口，组合跨上下文操作时从装配层调用",
    "HEX-WRITER": "调用该表所属上下文的写入口，由表主适配器执行 SQL",
    "COMMENT-BANNER": "删掉文件头清单，依赖和归属由代码与上下文契约表达",
    "COMMENT-WORKAROUND": "删掉临时说明并登记 review/p2，确需保留时引用 #issue 编号",
    "HEX-RATCHET": "删除已失效例外，保留原有逐项上限，新边界变更走架构评审",
}

RUST_BANNER = re.compile(r"//!\s*(Responsibility|Dependencies|Callers|Invariants|职责|依赖|调用方|不变量)\s*[:：]")
SCRIPT_BANNER = re.compile(r"\s*#\s*(Responsibility|Dependencies|Callers|Invariants|职责|依赖|调用方|不变量)\s*[:：]")
DEBT = re.compile(r"\b(workaround|hack|todo|fixme)\b", re.IGNORECASE)
ISSUE = re.compile(r"(?<!\w)#[1-9][0-9]*\b")
IDENT = re.compile(r"(?:r#)?[^\W\d]\w*")
LEX = re.compile(
    r'(?P<comment>//[^\n]*|/\*)'
    r'|(?P<raw>(?:br|cr|r)(?P<hashes>\#{0,255})"(?s:.*?)"(?P=hashes))'
    r'|(?P<string>[bc]?"(?:[^"\\]|\\[\s\S])*")'
    r"|(?P<char>b?'(?:[^'\\\n]|\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|.))')"
    r"|(?P<code>(?:r#)?[^\W\d]\w*|::|[^\s])"
)
SQL_ID = r"""(?:"(?:[^"]|"")*"|'(?:[^']|'')*'|\x60[^\x60]+\x60|\[[^\]]+\]|[a-z_]\w*|\{\w*\}|\?)"""
SQL_WORD = re.compile(
    rf"(?P<comment>--[^\n]*|/\*(?s:.*?)\*/)|{SQL_ID}|[^\s]",
    re.IGNORECASE,
)
SQL_START = {
    "insert", "update", "replace", "delete", "select", "with", "create", "alter", "drop",
    "pragma", "begin", "commit", "end", "savepoint", "rollback", "release", "explain",
    ";", "values", "attach", "detach", "analyze", "reindex", "vacuum",
}
SQL_ID_RE = re.compile(SQL_ID, re.IGNORECASE)
CFG_TEST = ["#", "[", "cfg", "(", "test", ")", "]"]
# External module declarations support complementary simple cfg atoms, unix
# versus windows, and different values for single-value target keys. Compound
# predicates and cfg_attr stay potentially overlapping. Every source is scanned.
CFG_ATOM = r'(?:\w+ = "[\w.-]+"|\w+)'
SIMPLE_CFG = re.compile(rf'# \[ cfg \( ({CFG_ATOM}|not \( {CFG_ATOM} \)) \) \]')
TARGET_KEYS = {"target_arch", "target_os", "target_env", "target_vendor",
               "target_endian", "target_pointer_width", "target_abi"}
IO_PREFIXES = (
    "rusqlite", "sqlx", "std::fs", "std::process", "std::env", "std::net",
    "std::os::unix::fs", "std::os::windows::fs", "tempfile", "rustix::fs", "winapi_util",
    "reqwest", "ureq", "gltf", "png",
)


class Token(NamedTuple):
    text: str
    line: int
    start: int
    end: int
    kind: str


def lex(source):
    position, line = 0, 1
    while match := LEX.search(source, position):
        start, end = match.span()
        line += source.count("\n", position, start)
        kind = match.lastgroup
        if match[0] == "/*":
            depth = 1
            while depth and (boundary := BLOCK_BOUNDARY.search(source, end)):
                depth += 1 if boundary[0] == "/*" else -1
                end = boundary.end()
            if depth:
                end = len(source)
        value = source[start:end]
        yield Token(value, line, start, end, kind)
        line += value.count("\n")
        position = end


BLOCK_BOUNDARY = re.compile(r"/\*|\*/")


def classify(path):
    if path.is_relative_to(CORE):
        name = path.relative_to(CORE).with_suffix("").as_posix()
        if name in CORE_ROOT:
            return CORE_ROOT[name]
        context, _, file = name.partition("/")
        if context not in CONTEXTS:
            return None
        layer = NAMED_LAYERS[context].get(file)
        if layer:
            return context, layer
        parts = file.split("/")
        if parts == ["application", "ports"] or (len(parts) == 3 and parts[:2] == ["application", "ports"]):
            return context, "ports"
        if "/" not in file:
            if file == "mod":
                return context, "facade"
            for candidate in ("domain", "ports", "application"):
                if file == candidate or file.startswith(candidate + "_") or file.endswith("_" + candidate):
                    return context, candidate
            if file == "sqlite" or file.startswith("sqlite_") or file.endswith("_sqlite"):
                return context, "adapter"
    if path.is_relative_to(CLI):
        if path.stem in CONTEXTS and path.parent == CLI:
            return path.stem, "delivery"
        if path in (CLI / "main.rs", CLI / "protocol.rs"):
            return "shared", "delivery"
    if len(path.parts) >= 4 and path.parts[1] in {"assetweft-core", "assetweft-cli"}:
        if path.parts[2] == "tests":
            return "test", "test"
        if path.parts[2] == "examples":
            return "example", "example"
    return None


def string_value(token):
    if token.kind == "raw":
        quote = token.text.index('"')
        hashes = token.text[:quote].count("#")
        return token.text[quote + 1:len(token.text) - hashes - 1]
    value = token.text[token.text.index('"') + 1:-1]
    value = re.sub(r"\\\n\s*", "", value)
    return re.sub(
        r"\\(x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|[nrt0\"\\])",
        lambda m: chr(int(m[1][1:], 16)) if m[1].startswith("x") else
        chr(int(m[1][2:-1].replace("_", ""), 16)) if m[1].startswith("u") else
        {"n": "\n", "r": "\r", "t": "\t", "0": "\0"}.get(m[1], m[1]),
        value,
    )


def use_tree(tokens, index, prefix=()):
    """Yield (token, path, binding) from a Rust use tree, and its end index."""
    start = index
    path = list(prefix)
    entries = []
    if tokens[index] == "::":
        index += 1
    while IDENT.fullmatch(tokens[index]) or tokens[index] == "*":
        path.append(tokens[index])
        index += 1
        if tokens[index] != "::":
            break
        index += 1
    if tokens[index] == "{":
        index += 1
        while tokens[index] not in ("}", ""):
            leaves, next_index = use_tree(tokens, index, tuple(path))
            entries.extend(leaves)
            if tokens[next_index] != ",":
                index = next_index
                break
            index = next_index + 1
        return entries, index + 1
    if index == start:
        return [], index + 1
    if len(path) > 1 and path[-1] == "self":
        path.pop()
    if tokens[index] == "as":
        binding = tokens[index + 1]
        index += 2
    else:
        binding = path[-1]
    return [(start, tuple(path), binding)], index


@dataclass
class Scope:
    parent: object
    module: tuple
    test: bool = False
    imports: dict = field(default_factory=dict)
    globs: list = field(default_factory=list)
    names: set = field(default_factory=set)


@dataclass
class Unit:
    path: Path
    tokens: list
    source: str
    modules: list = field(default_factory=list)
    scopes: dict = field(default_factory=dict)
    refs: list = field(default_factory=list)
    strings: list = field(default_factory=list)
    include: list = field(default_factory=list)


def collect(unit, module):
    values = [t.text.removeprefix("r#") if t.kind == "code" else t.text for t in unit.tokens]
    values.extend([""] * 8)
    scope = Scope(None, module)
    unit.scopes[module] = scope
    index = 0
    start = 0
    while index < len(unit.tokens):
        value = values[index]
        token = unit.tokens[index]
        if value in (";", "{", "}"):
            start = index + 1
        if value == "{":
            scope = Scope(scope, scope.module, scope.test)
        elif value == "}":
            scope = scope.parent or scope
        elif value == "mod" and values[index + 2] in (";", "{"):
            attrs = values[start:index]
            cfg = any(attrs[i:i + len(CFG_TEST)] == CFG_TEST for i in range(len(attrs)))
            name = values[index + 1]
            test = scope.test or cfg
            if values[index + 2] == "{":
                scope = Scope(scope, (*scope.module, name), test)
                if not test:
                    unit.scopes[scope.module] = scope
            else:
                explicit = next(
                    (string_value(unit.tokens[i + 3]) for i in range(start, index)
                     if values[i:i + 3] == ["[", "path", "="]), None,
                )
                conditions = tuple(SIMPLE_CFG.findall(" ".join(attrs)))
                unit.modules.append((token, scope.module, name, explicit, test, conditions))
            index += 2
            start = index + 1
        elif scope.test:
            pass
        elif value in {"fn", "struct", "enum", "union", "trait", "type", "const", "static"}:
            name = values[index + 1]
            if IDENT.fullmatch(name):
                scope.names.add(name)
            if value == "type" and values[index + 2] == "=":
                first, end = index + 3, index + 4
                while values[end] == "::" and IDENT.fullmatch(values[end + 1]):
                    end += 2
                if IDENT.fullmatch(values[first]) and end - first > 1:
                    scope.imports[name] = tuple(values[first:end:2])
        elif value == "use" and values[index - 1] != "<":
            leaves, index = use_tree(values, index + 1)
            for at, path, binding in leaves:
                unit.refs.append((unit.tokens[at], scope, path, binding))
                if binding == "*":
                    scope.globs.append(path[:-1])
                elif binding != "_":
                    scope.imports[binding] = path
            continue
        elif value == "extern" and values[index + 1] == "crate":
            name = values[index + 2]
            binding = values[index + 4] if values[index + 3] == "as" else name
            scope.imports[binding] = (name,)
            unit.refs.append((token, scope, (name,), binding))
        elif token.kind in ("raw", "string"):
            value = string_value(token)
            if BEGIN_SQL.search(value):
                unit.strings.append((token, value))
        elif value in ("include", "include_str") and values[index + 1] == "!":
            argument = unit.tokens[index + 3] if index + 3 < len(unit.tokens) else token
            literal = string_value(argument) if argument.kind in {"raw", "string"} else None
            unit.include.append((token, literal))
        elif value == "concat" and values[index + 1:index + 3] == ["!", "("]:
            end = index + 3
            parts = []
            while end < len(unit.tokens) and unit.tokens[end].kind in ("raw", "string"):
                parts.append(string_value(unit.tokens[end]))
                end += 1
                if values[end] != ",":
                    break
                end += 1
            if values[end] == ")" and parts:
                unit.strings.append((token, "".join(parts)))
                index = end
        elif IDENT.fullmatch(value) and values[index + 1] == "::":
            end = index + 1
            while values[end] == "::" and IDENT.fullmatch(values[end + 1]):
                end += 2
            unit.refs.append((token, scope, tuple(values[index:end:2]), None))
            index = end - 1
        index += 1


BEGIN_SQL = re.compile(r"\b(insert|replace|update|delete)\b", re.IGNORECASE)


def mutually_exclusive(left, right):
    for a in left:
        for b in right:
            if a == f"not ( {b} )" or b == f"not ( {a} )" or {a, b} == {"unix", "windows"}:
                return True
            key, _, value = a.partition(" = ")
            other_key, _, other_value = b.partition(" = ")
            if key == other_key and key in TARGET_KEYS and value != other_value:
                return True
    return False


def module_file(path, module, parent, name, explicit, owns_directory):
    directory = path.parent
    if not owns_directory and path.name != "mod.rs":
        directory /= path.stem
    for item in parent[len(module):]:
        directory /= item
    if explicit is not None:
        base = path.parent if parent == module else directory
        return (ROOT / base / explicit).resolve().relative_to(ROOT)
    flat = directory / f"{name}.rs"
    return flat if (ROOT / flat).is_file() else directory / name / "mod.rs"


def module_prefix(path, modules):
    return next((path[:length] for length in range(len(path), 0, -1)
                 if path[:length] in modules), ())


class AlternateModule(Exception):
    pass


class ModuleIndex(dict):
    def __missing__(self, logical):
        ancestor = self.variants.get(logical, ())
        if len(ancestor) > 1:
            raise AlternateModule(logical, ancestor)
        return ancestor[0] if ancestor else None

    def __contains__(self, logical):
        return logical in self.variants

    def get(self, logical):
        return self[logical]

    def __init__(self, variants):
        super().__init__()
        self.variants = variants


def resolve_ref(ref, scope, index):
    # Branch only on bindings actually consulted by this reference.
    try:
        dependency = expand(ref, scope, index)
        yield dependency, index.get(module_prefix(dependency, index))
    except AlternateModule as branch:
        logical, scopes = branch.args
        try:
            for candidate in scopes:
                index[logical] = candidate
                yield from resolve_ref(ref, scope, index)
        finally:
            del index[logical]


def expand(path, scope, modules, seen=frozenset()):
    if not path or path[-1] == "*":
        return expand(path[:-1], scope, modules, seen) if path else ()
    key = (id(scope), path[0])
    if key in seen:
        return path
    seen = seen | {key}
    first = path[0]
    if first in {"self", "super", "crate", "assetweft_core"}:
        base = scope.module if first in {"self", "super"} else ()
        offset = 1
        while path[offset - 1] == "super":
            base = base[:-1]
            if offset == len(path) or path[offset] != "super":
                break
            offset += 1
        path = (*base, *path[offset:])
    else:
        owner = scope
        while owner:
            imported = owner.imports.get(first)
            if imported and imported[0] != first:
                return expand((*imported, *path[1:]), owner, modules, seen)
            if (*owner.module, first) in modules:
                path = (*owner.module, *path)
                break
            for glob in owner.globs:
                base = expand(glob, owner, modules, seen)
                if defines((*base, first), modules):
                    return expand((*base, *path), owner, modules, seen)
            owner = owner.parent

    prefix = module_prefix(path, modules)
    rest = path[len(prefix):]
    if prefix in modules and rest:
        exported_scope = modules[prefix]
        if rest[0] in exported_scope.imports and (id(exported_scope), rest[0]) not in seen:
            return expand(rest, exported_scope, modules, seen)
        for glob in exported_scope.globs:
            base = expand(glob, exported_scope, modules, seen)
            if defines((*base, rest[0]), modules):
                return expand((*base, *rest), scope, modules, seen)
    return path


def defines(path, modules):
    if path in modules:
        return True
    owner = modules.get(path[:-1])
    if owner and (path[-1] in owner.names or path[-1] in owner.imports):
        return True
    name = "::".join(path)
    return any(prefix == name or prefix.startswith(name + "::") for prefix in IO_PREFIXES)


class Finding(NamedTuple):
    rule: str
    path: Path
    line: int
    reason: str
    evidence: str

    def key(self):
        digest = hashlib.sha256(self.evidence.strip().encode()).hexdigest()
        return self.rule, self.path.as_posix(), self.reason, digest


def starts(name, choices):
    return any(name == prefix or name.startswith(prefix + "::") for prefix in choices)


def check_ref(path, source, dependency, target):
    name = "::".join(dependency)
    _, layer = source
    adapter = target and target[1] in {"adapter", "composition"}
    io = starts(name, IO_PREFIXES)
    if layer == "domain":
        return io or adapter or (target and target[1] == "application")
    if layer == "application":
        return io or adapter
    if layer == "ports":
        return adapter or starts(name, (
            "rusqlite", "std::process", "application::Application",
        ))
    return path == CORE / "execution/byo_gpu/mod.rs" and starts(name, ("library::LibraryError",))


def sql_names(value, writes):
    words = [m for m in SQL_WORD.finditer(value) if not m["comment"]]
    names = [word[0].lower() for word in words] + [""] * 6
    if names[0] not in SQL_START:
        return
    statement_start = True
    diagnostic = False
    for i, word in enumerate(words):
        verb = names[i]
        if verb == ";":
            statement_start = True
            diagnostic = False
            continue
        if statement_start:
            diagnostic = writes and verb == "explain"
            statement_start = False
        if diagnostic:
            continue
        end = i + 1
        if not writes:
            if names[i:i + 2] != ["create", "table"]:
                continue
            end += 1
            if names[end:end + 3] == ["if", "not", "exists"]:
                end += 3
        elif verb in {"insert", "replace", "update", "delete"}:
            if verb in {"insert", "update"} and names[end] == "or":
                end += 2
            if verb != "update":
                if names[end] != ("from" if verb == "delete" else "into"):
                    continue
                end += 1
            elif names[end] in {"set", "of", "on"}:
                continue
        else:
            continue
        if names[end + 1] == ".":
            end += 2
        if not SQL_ID_RE.fullmatch(names[end]):
            continue
        table = names[end].strip("'\"[]\x60")
        if verb == "update" and names[end + 1] not in {"set", "as", "indexed", "not"}:
            continue
        yield word.start(), words[end].end(), table


def check_sql(path, line, value, add, seen_writers):
    for start, end, table in sql_names(value, writes=True):
        owner = TABLE_WRITERS.get(table) or INFRA_WRITERS.get(table)
        at = line + value.count("\n", 0, start)
        if owner and path == owner:
            if table in TABLE_WRITERS:
                seen_writers.add(table)
        else:
            reason = f"表 {table} 的写入口为 {owner}" if owner else f"写入目标 {table} 缺少唯一表主"
            add("HEX-WRITER", path, at, reason, value[start:end])


ALLOWLIST = Path("tools/hex_lint_allowlist.json")


def load_allowlist(content):
    entries = {}
    for row in json.loads(content):
        key = (row["rule"], row["file"], row["reason"], row["sha256"])
        if (key in entries or row["class"] not in {"X", "H"} or
                not isinstance(row["basis"], str) or not row["basis"].strip() or
                (row["class"] == "H" and not ISSUE.search(row["basis"])) or
                not re.fullmatch(r"[a-f0-9]{64}", row["sha256"])):
            raise ValueError(f"无效或重复的例外 {key[:3]}")
        entries[key] = row
    return entries


def base_allowlist(ref):
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=ROOT, text=True, stderr=subprocess.PIPE)

    tree = git("ls-tree", "--name-only", ref, "--", "tools/hex_lint.py", str(ALLOWLIST)).splitlines()
    if str(ALLOWLIST) in tree:
        return load_allowlist(git("show", f"{ref}:{ALLOWLIST}"))
    return {} if "tools/hex_lint.py" in tree else None


def ratchet(findings):
    path = ROOT / ALLOWLIST
    try:
        entries = load_allowlist(path.read_text()) if path.exists() else {}
        base = os.environ.get("ASSETWEFT_HEX_BASE")
        ceiling = base_allowlist(base) if base else None
    except (KeyError, TypeError, ValueError, OSError, subprocess.CalledProcessError) as error:
        return findings + [Finding("HEX-RATCHET", ALLOWLIST, 1, f"无法验证例外清单：{error}", "")]
    remaining = []
    if ceiling is not None:
        for key, row in sorted(entries.items()):
            if ceiling.get(key) != row:
                remaining.append(Finding("HEX-RATCHET", ALLOWLIST, 1, f"增加或修改例外：{key[:3]}", ""))
    for finding in findings:
        if finding.key() in entries:
            del entries[finding.key()]
        else:
            remaining.append(finding)
    for rule, file, reason, _ in sorted(entries):
        remaining.append(Finding("HEX-RATCHET", Path(file), 1, f"{rule} 例外已失效：{reason}", ""))
    return remaining

def check_cycles(edges, add):
    graph = {context: set() for context in CONTEXTS}
    for source, target in edges:
        graph[source].add(target)

    def route(source, goal, visited):
        if source == goal:
            return [goal]
        for target in sorted(graph[source] - visited):
            path = route(target, goal, visited | {source})
            if path:
                return [source, *path]
        return []

    for (source, target), (file, line, evidence) in sorted(edges.items()):
        if path := route(target, source, set()):
            add("HEX-CONTEXT", file, line, "依赖环 " + " -> ".join([source, *path]), evidence)


def scan():
    findings = []
    units = {}
    roles = {}
    edges = {}
    seen_writers = set()

    def add(rule, path, line, reason, evidence):
        findings.append(Finding(rule, path, line, reason, evidence))

    for file in sorted((ROOT / "crates").rglob("*.rs")):
        path = file.relative_to(ROOT)
        source = file.read_text()
        tokens = list(lex(source))
        for token in tokens:
            if token.kind != "comment":
                continue
            for offset, line in enumerate(token.text.splitlines()):
                at = token.line + offset
                if RUST_BANNER.match(line):
                    add("COMMENT-BANNER", path, at, "遗留文件头模板", line)
                if DEBT.search(line) and not ISSUE.search(line):
                    add("COMMENT-WORKAROUND", path, at, "临时注释缺少关联 issue", line)
        role = classify(path)
        if role is None:
            add("HEX-COVERAGE", path, 1, "Rust 文件未登记上下文和层", "")
            continue
        roles[path] = role
        units[path] = Unit(path, [t for t in tokens if t.kind != "comment"], source)

    try:
        metadata = subprocess.run(
            ["cargo", "metadata", "--offline", "--locked", "--no-deps", "--format-version", "1"],
            cwd=ROOT, capture_output=True, text=True, check=True,
        )
        targets = {
            Path(target["src_path"]).resolve().relative_to(ROOT.resolve())
            for package in json.loads(metadata.stdout)["packages"]
            for target in package["targets"]
            if {"test", "example"}.intersection(target["kind"])
        }
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        add("HEX-COVERAGE", Path("Cargo.toml"), 1,
            f"无法解析 Cargo 目标：{getattr(error, 'stderr', None) or error}", "")
        targets = set()

    core = {}
    cli = {}
    index_roles = {}
    visited = set()
    linked = {}
    target_roots = targets | {CORE / "lib.rs", CLI / "main.rs"}

    def visit(path, module, index, origin=None, conditions=(), explicit_path=False):
        if path not in units:
            file, line = origin or (path, 1)
            add("HEX-COVERAGE", file, line, f"模块 {path} 无法分类或不存在", "")
            return
        if path in visited:
            if origin and roles[origin[0]][1] not in {"test", "example"}:
                file, line = origin
                add("HEX-COVERAGE", file, line, f"模块 {path} 重复链接", "")
            return
        visited.add(path)
        unit = units[path]
        collect(unit, module)
        for logical, scope in unit.scopes.items():
            variants = index.setdefault(logical, [])
            if any(not mutually_exclusive(conditions, other) for _, other in variants):
                add("HEX-COVERAGE", path, 1, f"模块 {'::'.join(logical)} 有多个源文件", "")
            variants.append((scope, conditions))
            index_roles[id(scope)] = roles[path]
        declarations = {}
        for token, parent, name, explicit, test, cfg in unit.modules:
            try:
                child = module_file(path, module, parent, name, explicit,
                                    explicit_path or path in target_roots)
            except (OSError, ValueError):
                add("HEX-COVERAGE", path, token.line, "模块路径超出仓库范围", token.text)
                continue
            if test:
                if child not in roles or roles[child][1] != "test":
                    add("HEX-COVERAGE", path, token.line, f"测试模块 {child} 未登记测试层", token.text)
                else:
                    visit(child, (*parent, name), {}, explicit_path=explicit is not None)
                continue
            if child in roles and roles[path][1] not in {"test", "example"} and roles[child][1] in {"test", "example"}:
                add("HEX-COVERAGE", path, token.line, f"生产模块引用了 {child} 的测试或示例代码", token.text)
            else:
                logical = (*parent, name)
                # A physical test helper may be shared across Cargo targets.
                # Check repeated declarations even when that file was visited.
                previous = declarations.setdefault(logical, [])
                if any(not mutually_exclusive(cfg, other) for other in previous):
                    add("HEX-COVERAGE", path, token.line, f"模块 {'::'.join(logical)} 有多个声明", token.text)
                previous.append(cfg)
                visit(child, logical, index, (path, token.line), (*conditions, *cfg),
                      explicit_path=explicit is not None)
                if child in roles and roles[path][0] != roles[child][0] and roles[path][0] in CONTEXTS:
                    linked.setdefault(path, []).append((token, unit.scopes.get(parent), ("crate", *logical), None))

    visit(CORE / "lib.rs", (), core)
    visit(CLI / "main.rs", (), cli)
    for path in sorted(targets):
        visit(path, (), {})

    core_index = ModuleIndex({logical: tuple(scope for scope, _ in variants)
                              for logical, variants in core.items()})
    cli_index = ModuleIndex({logical: tuple(scope for scope, _ in variants)
                             for logical, variants in cli.items()})
    for path, unit in units.items():
        context, layer = roles[path]
        if path not in visited:
            add("HEX-COVERAGE", path, 1, "已分层的 Rust 文件未链接到 crate 的模块树", "")
            collect(unit, tuple(path.with_suffix("").parts[3:]))
        if layer == "test":
            continue

        destination = core_index if path.is_relative_to(CORE) or layer == "example" else cli_index
        lines = unit.source.splitlines()
        for token, scope, ref, _ in [*unit.refs, *linked.get(path, ())]:
            if scope is None:
                continue
            resolutions = {(dep, index_roles.get(id(owner)))
                           for dep, owner in resolve_ref(ref, scope, destination)}
            for dependency, target in sorted(resolutions, key=repr):
                name = "::".join(dependency)
                evidence = lines[token.line - 1]
                if check_ref(path, (context, layer), dependency, target):
                    add("HEX-LAYER", path, token.line, f"{layer} 层依赖 {name}", evidence)
                if not target or context not in CONTEXTS or layer == "delivery":
                    continue
                other = target[0]
                if other not in CONTEXTS or context == other:
                    continue
                edge = (context, other)
                edges.setdefault(edge, (path, token.line, evidence))
                allowed = EDGE_PREFIXES.get(edge, ())
                if path.is_relative_to(CORE):
                    allowed += ADAPTER_EDGES.get(path.relative_to(CORE).as_posix(), ())
                borrowed_head_module = path == CORE / "sqlite/acceptance.rs" and name == "catalog::sqlite_store"
                if not borrowed_head_module and not starts(name, allowed):
                    add("HEX-CONTEXT", path, token.line, f"{context} 到 {other} 的 {name} 未列入契约边", evidence)

        for token, value in unit.strings:
            check_sql(path, token.line, value, add, seen_writers)
        for token, name in unit.include:
            if token.text == "include":
                add("HEX-COVERAGE", path, token.line, "include! 生成模块绕过架构归属", name or "")
            elif name is None:
                add("HEX-WRITER", path, token.line, "include_str! 的动态路径无法检查 SQL 写主", "")
            else:
                sql = (ROOT / path.parent / name).resolve()
                if not sql.is_relative_to(ROOT) or not sql.is_file():
                    add("HEX-WRITER", path, token.line, f"无法扫描嵌入文件 {name} 的 SQL 写入", name)
                else:
                    check_sql(path, token.line, sql.read_text(), add, seen_writers)

    check_cycles(edges, add)
    migrations = {}
    for file in sorted((ROOT / CORE / "library").glob("[0-9][0-9][0-9]_*.sql")):
        sql = file.read_text()
        for start, _, table in sql_names(sql, writes=False):
            at = sql.count("\n", 0, start) + 1
            migrations[table] = file.relative_to(ROOT), at
    for table in sorted(migrations.keys() - TABLE_WRITERS.keys()):
        file, line = migrations[table]
        add("HEX-WRITER", file, line, f"迁移中的表 {table} 没有登记写主", table)
    for table in sorted(TABLE_WRITERS.keys() - migrations.keys()):
        add("HEX-WRITER", TABLE_WRITERS[table], 1, f"登记的表 {table} 未出现在迁移中", table)
    for table in sorted((migrations.keys() & TABLE_WRITERS.keys()) - seen_writers):
        add("HEX-WRITER", TABLE_WRITERS[table], 1, f"登记的写主没有表 {table} 的 SQL 写入口", table)

    for directory in ("tools", ".github/workflows"):
        for file in sorted((ROOT / directory).rglob("*")):
            if not file.is_file():
                continue
            try:
                content = file.read_text()
            except UnicodeDecodeError:
                continue
            for line, content_line in enumerate(content.splitlines(), 1):
                if SCRIPT_BANNER.match(content_line):
                    add("COMMENT-BANNER", file.relative_to(ROOT), line, "遗留文件头模板", content_line)
    return sorted(findings), len(units)


def main():
    findings, checked = scan()
    violations = sorted(ratchet(findings))
    for item in violations:
        print(f"[{item.rule}] {item.path}:{item.line} | 根因: {item.reason} | 正确入口/修法: {FIX[item.rule]}",
              file=sys.stderr)
    if violations:
        return 1
    print(f"HEX PASS ({checked} Rust files, {len(TABLE_WRITERS)} tables, "
          f"{len(findings)} exact legacy exceptions)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
