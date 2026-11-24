---
name: assetweft-cli
description: Run AssetWeft's offline BYO GPU fixture recipe and verify its CLI receipts, budget, workflow and managed GLB. Consult the authorization steps before using a real provider.
---

# AssetWeft CLI recipe (AW-034:A)

From the repository root, build the binary from cached Rust dependencies and run the recipe. Humans and agents use the same entry point. The evaluator executes the command block below verbatim, prints each raw CLI receipt, and deletes its private temporary library and fixtures even on failure.

```sh
cargo build --locked --offline -p assetweft-cli && ./skills/assetweft-cli/eval_recipe.sh
```

The evaluator uses POSIX shell, Node.js and the SQLite CLI. It reads the debug executable from `CARGO_TARGET_DIR` when set or from the target directory reported by offline Cargo metadata. Keep the same `CARGO_TARGET_DIR` for the build and evaluation. Set `ASSETWEFT_BIN` to override discovery with an already-built executable. The evaluator prints the executable's absolute path and SHA-256 before the receipts. AssetWeft needs no Python runtime, daemon or MCP server for this recipe. See the [CLI v1 protocol](../../crates/assetweft-cli/README.md) and [feature map](../../docs/FEATURE_MAP.md).

## Authorization

1. **Install and inspect.** Building the binary, `validate`, `inspect`, `plan`, `byo-gpu profiles`, `byo-gpu kaggle-validate` and read-only budget, workflow and Catalog queries submit no jobs, upload no inputs and incur no provider spend. Database queries need an existing library. These are reads of business state and SQLite may still use connection sidecars.
2. **Initialize and declare a local cap.** Use `library init` on a named private library. The operator explicitly supplies the cap to `budget approve`: this recipe declares one `Task` under `ExplicitBounded` plus a separate zero-task rejection control. Local approval grants a ledger allowance only. Provider fees may be unknown (`cash_cost:null`).
3. **Authorize each external execution.** Before a real run, require the operator's explicit approval for the provider, inputs, upload and any external spend. Run `validate` and `inspect` on the base GLB and require `plan` to return `DECLARATION_CONSISTENT` against independently supplied locks. These are recipe gates. The BYO command itself enforces the budget reservation and upload flag. A declaration-only plan checks consistency within declared scope. It does not measure lock preservation or establish rights. Use a trusted adapter and credentials in provider environment variables or official client configuration. Never collect cookies or passwords or pass credentials on the command line. An image or bundle requires explicit `--allow-remote-upload`, including in this offline fixture.

## Command order

The evaluator prepares a checked-in mesh GLB, a synthetic image and private bundle, a pinned declaration fixture, allowances, and one workflow step. `cli ID EXIT STATUS ...` runs `assetweft --call-id ID ...` and stops on any receipt mismatch. `prepare_artifact` derives Catalog metadata and the workflow outcome from the observed output bytes and validation receipt.

```sh recipe
cli input-validate 0 PASS validate base.glb --role static-mesh
cli input-inspect 0 PASS inspect base.glb
cli locked-reject 4 LOCK_VIOLATION plan --request bad-request.json --trusted-context context.json
cli plan 0 DECLARATION_CONSISTENT plan --request request.json --trusted-context context.json
cli init 0 PASS library init --db library.sqlite
cli no-approval 1 FAIL byo-gpu run --db library.sqlite --budget-id approved --intent-id unapproved --profile hf-zerogpu --model trellis2 --out-dir out --adapter-bin ./adapter --image input.png --allow-remote-upload
cli approve-zero 0 PASS budget approve --db library.sqlite --allowance zero.json
cli cap-exhausted 2 INVALID_INPUT byo-gpu run --db library.sqlite --budget-id zero --intent-id exhausted --profile hf-zerogpu --model trellis2 --out-dir out --adapter-bin ./adapter --image input.png --allow-remote-upload
cli approve 0 PASS budget approve --db library.sqlite --allowance allowance.json
cli profiles 0 PASS byo-gpu profiles
cli bundle-check 0 PASS byo-gpu kaggle-validate --bundle bundle
cli no-upload 2 INVALID_INPUT byo-gpu run --db library.sqlite --budget-id approved --intent-id no-upload --profile hf-zerogpu --model trellis2 --out-dir out --adapter-bin ./adapter --image input.png
cli generate 0 PASS byo-gpu run --db library.sqlite --budget-id approved --intent-id once --profile hf-zerogpu --model trellis2 --out-dir out --adapter-bin ./adapter --image input.png --allow-remote-upload
cli receipt-first 0 PASS budget receipts --db library.sqlite --intent-id once
cli replay 0 PASS byo-gpu run --db library.sqlite --budget-id approved --intent-id once --profile hf-zerogpu --model trellis2 --out-dir out --adapter-bin ./adapter --image input.png --allow-remote-upload
cli receipt-replay 0 PASS budget receipts --db library.sqlite --intent-id once
cli output-validate 0 PASS validate out/artifact.glb --role static-mesh
cli workflow-create 0 PASS workflow create --db library.sqlite --run-id recipe --steps steps.json
cli workflow-pending 0 PASS workflow status --db library.sqlite --run-id recipe
cli workflow-claim 0 PASS workflow claim --db library.sqlite --run-id recipe
cli workflow-running 0 PASS workflow status --db library.sqlite --run-id recipe
prepare_artifact
cli workflow-finish 0 PASS workflow finish --db library.sqlite --run-id recipe --step-id admit --outcome outcome.json
cli workflow-settled 0 PASS workflow status --db library.sqlite --run-id recipe
cli cas-commit 0 PASS catalog commit-object --db library.sqlite --version version.json --file out/artifact.glb --expected-len "$bytes"
cli cas-read 0 PASS catalog file-availability --db library.sqlite --version-id recipe-v1
cli catalog-version 0 PASS catalog version --db library.sqlite --version-id recipe-v1
cli balance 0 PASS budget balance --db library.sqlite --budget-id approved
```

Check `results.state:SUCCEEDED`, `invoked:true` and `artifact.{file,sha256}`. Reusing the same settled intent and inputs yields identical artifact information, `invoked:false` and the same persisted usage receipt. If execution returns `UNKNOWN/1` or an output error makes a write ambiguous (`exit 6`), query the existing intent, receipts or workflow status before acting. Avoid resubmission while state is uncertain. Workflow claim and finish persist state; the finish acknowledgment echoes caller-supplied evidence.

The three execution rejection controls assert zero adapter invocations, no output or staging residue, and identical SQLite dumps before and after each rejection. They use a precreated output directory because a budget rejection can follow temporary local staging and directory creation. Positive checks bind the output GLB hash to the usage receipt, Catalog row and stored CAS bytes, and read the finished step back from SQLite. The fixture adapter only copies a real synthetic GLB. Live remote GPU calls, paid APIs, full engine rendering and the optional AW-033:C MCP adapter are `NOT_RUN`.

## AW-034:B Incremental reuse and backup/restore

After the same CLI build, run `./skills/assetweft-cli/eval_extended_recipe.sh`. The evaluator uses the same binary overrides, prints each CLI receipt and cleans its private temporary directory. It seeds a managed GLB from the checked-in static mesh fixture and executes the command block below.

- **Reuse.** Supply the library, exact version ID, asset ID and expected SHA-256. Require `AVAILABLE`, matching Catalog metadata and CAS bytes, and a matching current head when one exists. Direct commits can leave the candidate head unset. An unknown version stops at the reuse gate even when the query itself returns `PASS`. After admission, validate the managed mesh and record exact-version feedback. The recipe skips generation and duplicate commit.
- **Restore.** Supply the original library and fresh snapshot and restore directories. Require backup and restore `PASS`, matching version, availability, SHA-256 and feedback after isolation of the original library, then append and read back new feedback in the restored database. A same-length corruption in a snapshot copy must fail hash verification without publishing a destination manifest.

Live paid remote GPU, full Unity/Unreal/Godot 4.7.2 rendering and AW-033:C MCP are `NOT_RUN`.

```sh extended-recipe
cli init 0 PASS library init --db "$db"
cli seed 0 PASS catalog commit-object --db "$db" --version version.json --file seed.glb --expected-len "$bytes"
cli before-current 0 PASS catalog current --db "$db" --asset-id "$asset"
cli before-version 0 PASS catalog version --db "$db" --version-id "$version"
cli before-availability 0 PASS catalog file-availability --db "$db" --version-id "$version"
cli missing-availability 0 PASS catalog file-availability --db "$db" --version-id uncommitted
missing_rc=0
reuse_gate missing 2> missing.log || missing_rc=$?
[ "$missing_rc" -eq 3 ]
grep -Fx 'reuse refused: uncommitted is UNKNOWN_VERSION' missing.log
reuse_gate before
cli validate-existing 0 PASS validate "$object" --role static-mesh
cli review 0 PASS catalog feedback-add --db "$db" --asset-id "$asset" --version-id "$version" --feedback-id review-1 --decision ACCEPT --reason 'Validated existing static mesh'
cli before-feedback 0 PASS catalog feedback-latest --db "$db" --asset-id "$asset" --version-id "$version"

cli backup 0 PASS library backup --db "$db" --out "$tmp/backup-snap"
[ -f backup-snap/manifest.json ]
mv original retired
[ ! -e "$db" ]
db="$tmp/restored-lib/library.sqlite"
object="$tmp/restored-lib/objects/$digest"
cli restore 0 PASS library restore --from "$tmp/backup-snap" --dest "$tmp/restored-lib"
[ -f restored-lib/manifest.json ]
cli after-current 0 PASS catalog current --db "$db" --asset-id "$asset"
cli after-version 0 PASS catalog version --db "$db" --version-id "$version"
cli after-availability 0 PASS catalog file-availability --db "$db" --version-id "$version"
cli after-feedback 0 PASS catalog feedback-latest --db "$db" --asset-id "$asset" --version-id "$version"
reuse_gate after
cli resume 0 PASS catalog feedback-add --db "$db" --asset-id "$asset" --version-id "$version" --feedback-id review-2 --decision ACCEPT --reason 'Reviewed after restore'
cli resumed-feedback 0 PASS catalog feedback-latest --db "$db" --asset-id "$asset" --version-id "$version"

cp -R backup-snap corrupt-snap
node -e 'const fs = require("node:fs"); const file = process.argv[1]; const bytes = fs.readFileSync(file); bytes[0] ^= 1; fs.writeFileSync(file, bytes)' "corrupt-snap/objects/$digest"
cli corrupt-restore 2 INVALID_INPUT library restore --from "$tmp/corrupt-snap" --dest "$tmp/failed-lib"
[ ! -e failed-lib/manifest.json ]
check_resume
[ ! -e "$ASSETWEFT_RECIPE_INVOCATIONS" ]
printf 'PASS reused %s sha256=%s invocations=0; restored and resumed; corrupt snapshot refused\n' "$version" "$digest"
```
