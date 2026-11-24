#!/bin/sh
set -u

usage() {
    cat <<'EOF'
Usage:
  ./tools/verify.sh --full
  ./tools/verify.sh --crate <pkg> [--test <name>]
  ./tools/verify.sh --contracts
  ./tools/verify.sh --help
EOF
}

mode=""
pkg=""
test_name=""

while [ "$#" -gt 0 ]; do
    case "$1" in
        --full)
            if [ -n "$mode" ]; then
                printf '[verify] FAIL reason="conflicting mode flags"\n' >&2
                usage >&2
                exit 2
            fi
            mode="full"
            shift
            ;;
        --contracts)
            if [ -n "$mode" ]; then
                printf '[verify] FAIL reason="conflicting mode flags"\n' >&2
                usage >&2
                exit 2
            fi
            mode="contracts"
            shift
            ;;
        --crate)
            if [ -n "$mode" ] && [ "$mode" != "crate" ]; then
                printf '[verify] FAIL reason="conflicting mode flags"\n' >&2
                usage >&2
                exit 2
            fi
            if [ "$#" -lt 2 ] || [ -z "$2" ]; then
                printf '[verify] FAIL reason="--crate requires a package name"\n' >&2
                usage >&2
                exit 2
            fi
            mode="crate"
            pkg="$2"
            shift 2
            ;;
        --test)
            if [ "$#" -lt 2 ] || [ -z "$2" ]; then
                printf '[verify] FAIL reason="--test requires a test target name"\n' >&2
                usage >&2
                exit 2
            fi
            test_name="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf '[verify] FAIL reason="unknown argument: %s"\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ -z "$mode" ]; then
    printf '[verify] FAIL reason="missing mode flag"\n' >&2
    usage >&2
    exit 2
fi

if [ -n "$test_name" ] && [ "$mode" != "crate" ]; then
    printf '[verify] FAIL reason="--test is only valid with --crate"\n' >&2
    usage >&2
    exit 2
fi

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd) || exit 2
cd "$root" || exit 2

if [ -n "${TMPDIR:-}" ]; then
    mkdir -p "$TMPDIR" || exit 2
    log_dir="$TMPDIR/assetweft-verify"
else
    log_dir="$root/target/verify-logs"
fi
mkdir -p "$log_dir" || exit 2

log_file=$(mktemp "$log_dir/${mode}.XXXXXX") || exit 2

print_failure_excerpt() {
    excerpt=$(grep -nE '^(error(\[|:)|failures:|test .* FAILED|thread .* panicked|✖ |MISMATCH |ERROR )|(^|[^[:alnum:]_])(FAIL|failed)([^[:alnum:]_]|$)' "$1" | head -n 20)
    if [ -n "$excerpt" ]; then
        printf '%s\n' "$excerpt" >&2
    else
        tail -n 20 "$1" >&2
    fi
}

export CARGO_TERM_COLOR=never

case "$mode" in
    full)
        out_file=$(mktemp "$log_dir/full-stdout.XXXXXX") || exit 2
        ./tools/c1-stdout-contract.sh >"$out_file" 2>"$log_file"
        rc=$?
        verdict=$(tr -d '\n' <"$out_file")
        if [ -s "$out_file" ]; then
            printf '\n--- stdout ---\n' >>"$log_file"
            cat "$out_file" >>"$log_file"
        fi
        rm -f "$out_file"
        if [ "$rc" -eq 0 ]; then
            printf '[verify] PASS mode=full verdict=%s log=%s\n' "${verdict:-ok}" "$log_file"
            exit 0
        fi
        printf '[verify] FAIL mode=full exit=%s verdict=%s log=%s\n' "$rc" "${verdict:-none}" "$log_file" >&2
        print_failure_excerpt "$log_file"
        exit "$rc"
        ;;
    crate)
        if [ -n "$test_name" ]; then
            cargo test --locked -p "$pkg" --test "$test_name" >"$log_file" 2>&1
            rc=$?
            target_desc="pkg=$pkg test=$test_name"
        else
            cargo test --locked -p "$pkg" >"$log_file" 2>&1
            rc=$?
            target_desc="pkg=$pkg"
        fi
        if [ "$rc" -eq 0 ]; then
            summary=$(grep -E '^test result:' "$log_file" | tail -n 3 | tr '\n' '; ' | sed 's/; $//')
            printf '[verify] PASS mode=crate %s summary="%s" log=%s\n' "$target_desc" "${summary:-ok}" "$log_file"
            exit 0
        fi
        printf '[verify] FAIL mode=crate exit=%s %s log=%s\n' "$rc" "$target_desc" "$log_file" >&2
        print_failure_excerpt "$log_file"
        exit "$rc"
        ;;
    contracts)
        if [ ! -f "$root/node_modules/ajv/package.json" ]; then
            printf 'missing node_modules/ajv; run "npm ci" before "./tools/verify.sh --contracts"\n' >"$log_file"
            printf '[verify] FAIL mode=contracts exit=2 reason="missing node_modules/ajv; run npm ci first" log=%s\n' "$log_file" >&2
            exit 2
        fi
        {
            printf '=== node --test tools/contract_check.test.mjs ===\n'
            node --test tools/contract_check.test.mjs
        } >"$log_file" 2>&1
        rc=$?
        if [ "$rc" -eq 0 ]; then
            {
                printf '\n=== node tools/contract_check.mjs ===\n'
                node tools/contract_check.mjs \
                    --manifest contracts/fixtures/manifest.json \
                    --fixtures contracts/fixtures/fixtures.json \
                    --schema contracts/first-delivery-contract.schema.json
            } >>"$log_file" 2>&1
            rc=$?
        fi
        if [ "$rc" -eq 0 ]; then
            printf '[verify] PASS mode=contracts log=%s\n' "$log_file"
            exit 0
        fi
        printf '[verify] FAIL mode=contracts exit=%s log=%s\n' "$rc" "$log_file" >&2
        print_failure_excerpt "$log_file"
        exit "$rc"
        ;;
esac
