# AssetWeft Workbench

AW-032:A/B is a local desktop adapter for `assetweft-core`. Use an existing AssetWeft SQLite file or initialize one in an existing writable directory. No database server or account is required.

## Run locally

From this directory, with Node 24, the repository-pinned Rust toolchain, and the native Tauri build prerequisites for your OS:

```sh
npm ci --ignore-scripts
npm run build
cargo run --locked --manifest-path src-tauri/Cargo.toml
```

Choose **Open library**, enter the SQLite file path, and enable **Create or initialize this library** when creating a new catalog or migrating an old schema. Use a dedicated directory for each library so the SQLite file and the managed object store stay together. Set `AW_WORKBENCH_DB` to an existing initialized SQLite file to open it at startup.

Choose **Import local file** and enter a regular local path, asset/version IDs, format and declared usage and license. Files are limited to 16 MiB. The catalog reflects Core availability and feedback. GLB displays static geometry and the inspector can request a Core validation report. PNG uses Core thumbnails when supported and labels any raw CAS fallback. WAV uses Core preview validation and plays the local bytes.

The **Capabilities** dialog shows current scope. Skin, morph and animation rendering are `NOT_RUN`. Advanced editing is planned for AW-032:C. Cloud sync and login are disabled.

## Generation & Budgets

Approve a local allowance, reserve a call with an explicit bound, and record provider receipts. Blank quotes and absent charges display as **Unknown**. An UNKNOWN intent still holds its reservation and must be reconciled before another submission. Local cancellation records intent and makes no claim of remote stop or refund.

BYO GPU uses the configured operator-supplied executable and a local Task allowance. Workbench shows the Core profiles, model licenses, credential settings and runtime limitations. Upload authorization starts unchecked each time the dialog opens. A local image or bundle requires explicit opt-in. A settled intent can return its historical hash with `invoked=false`. Cash cost is unknown. No paid provider calls run automatically. Artifact generation and file validation are separate operations.

## Incremental Candidates

Use managed TEST_FIXTURE versions with component references to initialize `TEST_FIXTURE_GLB_STRUCTURE_V1` and propose a BYTE-locked candidate against a specific base version and revision. Core measures the proposal, rejects changed locked components and advances the head atomically only when the base and revision still match. Conflicts preserve the staged branch. The Candidates tab lists up to 100 proposals and their associated heads. Use the Catalog Inspector to check the current head of another asset or record independent Accept/Reject feedback on an exact version.

## Verify

```sh
npm run check:tokens
npm run typecheck
npm run build
npm test
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Set `AW_WORKBENCH_TEST_CLI` to the absolute path of the built `assetweft` executable to include CLI protocol parity in service tests. The native recovery matrix supplies this path after building its CLI tests. Native screenshot review is a separate acceptance step.
