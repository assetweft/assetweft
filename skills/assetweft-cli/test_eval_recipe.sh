#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
ASSETWEFT_TEST_BIN=$(CDPATH= cd -- "$root" && cargo metadata --offline --format-version 1 --no-deps |
    node -e 'const fs = require("node:fs"); process.stdout.write(JSON.parse(fs.readFileSync(0)).target_directory + "/debug/assetweft");')
export ASSETWEFT_TEST_BIN
[ -x "$ASSETWEFT_TEST_BIN" ] || { printf 'Build the assetweft CLI first: %s\n' "$ASSETWEFT_TEST_BIN" >&2; exit 1; }

tmp_parent=$(CDPATH= cd -- "${TMPDIR:-/tmp}" && pwd -P)
sandbox=$(mktemp -d "$tmp_parent/assetweft-recipe-test.XXXXXX")
trap 'rm -rf -- "$sandbox"' EXIT
trap 'exit 1' HUP INT TERM
cd "$sandbox"
mkdir -p skills/assetweft-cli target/debug custom/debug scratch
cp "$root/skills/assetweft-cli/"eval_recipe.sh "$root/skills/assetweft-cli/"SKILL.md skills/assetweft-cli/
ln -s "$root/crates" crates
printf '#!/bin/sh\nprintf "stale binary invoked\\n" >&2\nexit 44\n' > target/debug/assetweft
cat > custom/debug/assetweft <<'SH'
#!/bin/sh
if [ "${FORCE_FAILURE:-0}" = 1 ] && [ "$2" = no-approval ]; then
    test -f library.sqlite && test -s receipts/init.json || exit 45
    printf 'forced failure after init\n' >&2
    exit 42
fi
exec "$ASSETWEFT_TEST_BIN" "$@"
SH
chmod +x target/debug/assetweft custom/debug/assetweft
unset ASSETWEFT_BIN

failed=0
if CARGO_TARGET_DIR=custom TMPDIR="$sandbox/scratch" ./skills/assetweft-cli/eval_recipe.sh > target.log 2>&1 &&
    grep -Fx "assetweft_bin=$sandbox/custom/debug/assetweft" target.log &&
    grep -Ex 'assetweft_sha256=[0-9a-f]{64}' target.log &&
    ! grep -q 'stale binary invoked' target.log; then
    printf 'PASS custom target selected with stale default present\n'
else
    printf 'FAIL custom target selection\n' >&2
    tail -n 6 target.log >&2
    failed=1
fi

for mode in success failure; do
    mkdir "scratch/$mode"
    rc=0
    fail=0
    [ "$mode" = success ] || fail=1
    ASSETWEFT_BIN=custom/debug/assetweft FORCE_FAILURE=$fail TMPDIR="scratch/$mode" \
        ./skills/assetweft-cli/eval_recipe.sh > "$mode.log" 2>&1 || rc=$?
    if [ "$rc" -eq "$fail" ] &&
        { [ "$mode" = success ] || grep -Fq 'forced failure after init' "$mode.log"; } &&
        [ -z "$(ls -A "scratch/$mode")" ]; then
        printf 'PASS relative TMPDIR cleanup (%s)\n' "$mode"
    else
        printf 'FAIL relative TMPDIR cleanup (%s), exit=%s\n' "$mode" "$rc" >&2
        find "scratch/$mode" -maxdepth 2 -print >&2
        tail -n 5 "$mode.log" >&2
        failed=1
    fi
done
exit "$failed"
