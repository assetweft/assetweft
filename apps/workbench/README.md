# AssetWeft Workbench

AW-032:A is a local desktop adapter for `assetweft-core`. Use an existing AssetWeft SQLite file or initialize one in an existing writable directory. No database server or account is required.

## Run locally

From this directory, with Node 24, the repository-pinned Rust toolchain, and the native Tauri build prerequisites for your OS:

```sh
npm ci --ignore-scripts
npm run build
cargo run --locked --manifest-path src-tauri/Cargo.toml
```

Choose **Open library**, enter the SQLite file path, and enable **Create or initialize this library** when creating a new catalog or migrating an old schema. Use a dedicated directory for each library so the SQLite file and the managed object store stay together. Set `AW_WORKBENCH_DB` to an existing initialized SQLite file to open it at startup.

Choose **Import local file** and enter a regular local path, asset/version IDs, format and declared usage and license. Files are limited to 16 MiB. The catalog reflects Core availability and feedback. GLB displays static geometry and the inspector can request a Core validation report. PNG uses Core thumbnails when supported and labels any raw CAS fallback. WAV uses Core preview validation and plays the local bytes.

The **Capabilities** dialog shows current scope. Skin, morph and animation rendering are `NOT_RUN`. Generation is planned for AW-032:B, advanced editing for AW-032:C, and cloud sync and login are disabled.

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
