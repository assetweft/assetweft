#!/bin/sh
# 职责: C1 快检查的唯一本地入口（fmt -> clippy -> test -> build）。
# 依赖: cargo、rustfmt、clippy；版本由 rust-toolchain.toml 固定。
# 调用方: .github/workflows/c1.yml 的 C1 步骤、开发者本地、红绿对照实验。
# 不变量: 任一步骤失败即以该步骤退出码退出；stdout 只输出一行机器 JSON，过程日志走 stderr。
#         不跳过任何步骤，不把文档检查(docs/tools/verify_bundle.mjs)当作产品构建。

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

run_step fmt cargo fmt --all --check
run_step clippy cargo clippy --workspace --all-targets --locked -- -D warnings
run_step test cargo test --workspace --locked
run_step build cargo build --workspace --locked

printf '{"status":"PASS","steps":["fmt","clippy","test","build"]}\n'
