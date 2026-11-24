# AssetWeft（游织）

开源、本地优先的 Rust 游戏资产工作台：嵌入式 SQLite + 本地 CAS 对象库 + 共享核心库（`assetweft-core`）+ 版本化 JSON CLI（`assetweft-cli`）。

## 权威导航

| 主题 | 现行入口 |
|---|---|
| 能力地图与正常判据 | [docs/FEATURE_MAP.md](docs/FEATURE_MAP.md) |
| 开发规范（DDD × 六边形 · 去注释化） | [docs/DEVELOPMENT_STANDARDS.md](docs/DEVELOPMENT_STANDARDS.md) |
| CLI v1 协议、命令与退出码 | [crates/assetweft-cli/README.md](crates/assetweft-cli/README.md) |
| Agent Skill 与可复跑配方 | [skills/assetweft-cli/SKILL.md](skills/assetweft-cli/SKILL.md) |
| 交付合同 Schema 与媒体兼容 | [contracts/README.md](contracts/README.md) · [contracts/media-compatibility.md](contracts/media-compatibility.md) |
| Assurance 限界上下文契约 | [crates/assetweft-core/src/assurance/CONTRACT.md](crates/assetweft-core/src/assurance/CONTRACT.md) |
| Catalog 限界上下文契约（7 表写主） | [crates/assetweft-core/src/catalog/CONTRACT.md](crates/assetweft-core/src/catalog/CONTRACT.md) |
| Consumption 限界上下文契约（3 表写主） | [crates/assetweft-core/src/consumption/CONTRACT.md](crates/assetweft-core/src/consumption/CONTRACT.md) |
| Execution 限界上下文契约（BYO GPU） | [crates/assetweft-core/src/execution/CONTRACT.md](crates/assetweft-core/src/execution/CONTRACT.md) |
| Production 限界上下文契约（5 表写主） | [crates/assetweft-core/src/production/CONTRACT.md](crates/assetweft-core/src/production/CONTRACT.md) |

## 本地验证入口

- 定向与全量验证 CLI：[tools/verify.sh](tools/verify.sh)（`--crate <pkg> [--test <name>]` · `--contracts` · `--full`）
- C1 协议门禁（fmt / clippy / test / build）：[tools/c1-stdout-contract.sh](tools/c1-stdout-contract.sh)
- 合同 Fixture 门禁：[tools/contract_check.mjs](tools/contract_check.mjs)（`npm test && npm run check`）

## 历史归档

- 早期 PG18 与守护进程（daemon）旧架构文档及 W0 快照已归档至 `docs/archive/`（非现行规范，已由 `.rgignore` 隔离出默认检索）；其中 `docs/archive/INTEGRITY.json` 经协调者批准保持原始 JSON 字节不变以留存原始完整性凭证。
- 原归档基线中仍适用的 8 项未落地业务不变量（含跨平台浮点规范化 `G-DET-1`、三引擎变换与 PBR 通道打包、开源许可隔离、Tauri 二进制协议等）已统一登记追踪于 [#142](https://github.com/assetweft/assetweft/issues/142)。
