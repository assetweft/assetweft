#!/bin/sh
# 职责: 回归验证 C1 入口的 stdout 机器协议，并把底层 C1 的真实结果原样传出。
# 依赖: POSIX sh；node 仅作为仓库已有的离线 JSON 解析工具（不进产品运行时）。
# 调用方: .github/workflows/c1.yml 的唯一 C1 步骤、本地回归、红绿对照实验。
# 不变量: 不修改源码；底层 stderr 始终转发到本脚本 stderr（不吞诊断）；
#         底层非零时本脚本也非零且沿用该退出码，绝不把失败报成 PASS；
#         只回收自己创建的 mktemp 目录，不碰历史日志。

c1=${1:-./tools/c1.sh}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/c1-stdout.XXXXXX") || exit 2
out="$tmp/out"
err="$tmp/err"
cleanup() { rm -rf "$tmp"; }
trap cleanup 0 HUP INT TERM

"$c1" >"$out" 2>"$err"
rc=$?
lines=$(wc -l <"$out" | tr -d ' ')

# 诊断始终可见：无论成败都转发底层 stderr。
if [ -s "$err" ]; then
    cat "$err" >&2
fi

if [ "$rc" -eq 0 ]; then
    expected_status="PASS"
else
    expected_status="FAIL"
fi

node -e '
const fs = require("fs");
const [file, rcArg, want] = process.argv.slice(1);
const raw = fs.readFileSync(file, "utf8");
const lines = raw.split("\n").filter((l) => l.length > 0);
if (lines.length !== 1) {
  console.error(`stdout must contain exactly one non-empty line, got ${lines.length}`);
  process.exit(3);
}
let doc;
try { doc = JSON.parse(lines[0]); } catch (e) {
  console.error(`stdout line is not JSON: ${e.message}`);
  process.exit(4);
}
if (doc.status !== want) {
  console.error(`status ${JSON.stringify(doc.status)} != ${JSON.stringify(want)} for exit ${rcArg}`);
  process.exit(5);
}
if (want === "FAIL" && String(doc.exit) !== String(rcArg)) {
  console.error(`FAIL record exit ${doc.exit} != process exit ${rcArg}`);
  process.exit(6);
}
if (want === "PASS" && (!Array.isArray(doc.steps) || doc.steps.length !== 4)) {
  console.error("PASS record must list four steps");
  process.exit(7);
}
' "$out" "$rc" "$expected_status"
node_rc=$?

if [ "$node_rc" -ne 0 ]; then
    printf '[c1-protocol] FAIL rc=%s lines=%s node_rc=%s\n' "$rc" "$lines" "$node_rc" >&2
    printf '{"status":"FAIL","protocol":"single-json","protocol_ok":false,"c1_exit":%s,"lines":%s}\n' "$rc" "$lines"
    if [ "$rc" -ne 0 ]; then
        exit "$rc"
    fi
    exit 1
fi

if [ "$rc" -ne 0 ]; then
    printf '[c1-protocol] protocol ok but C1 failed: c1_exit=%s lines=%s\n' "$rc" "$lines" >&2
    printf '{"status":"FAIL","protocol":"single-json","protocol_ok":true,"c1_exit":%s,"lines":%s}\n' "$rc" "$lines"
    exit "$rc"
fi

printf '[c1-protocol] ok: c1_exit=%s lines=%s\n' "$rc" "$lines" >&2
printf '{"status":"PASS","protocol":"single-json","protocol_ok":true,"c1_exit":%s,"lines":%s}\n' "$rc" "$lines"
exit 0
