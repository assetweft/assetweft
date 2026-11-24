#!/bin/sh

export CARGO_TERM_COLOR=never

run_step() {
    name=$1
    shift
    printf '[c1] %s: %s\n' "$name" "$*" >&2
    # 子命令 stdout 一并转 stderr：stdout 只保留最后一行机器 JSON。
    "$@" >&2
    rc=$?
    if [ "$rc" -ne 0 ]; then
        printf '{"status":"FAIL","step":"%s","exit":%s}\n' "$name" "$rc"
        exit "$rc"
    fi
}

run_step hex-lint python3 tools/hex_lint.py
run_step tech-preview python3 tools/tech_preview_check.py
run_step fmt cargo fmt --all --check
run_step clippy cargo clippy --workspace --all-targets --locked -- -D warnings
run_step test cargo test --workspace --locked
run_step build cargo build --workspace --locked

printf '{"status":"PASS","steps":["fmt","clippy","test","build"]}\n'
