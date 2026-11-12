# AW-003:A 契约说明 — 最小资产与生成合同

- 职责：给出 `../docs/CONTRACTS.md`（SHA-256 `655602458d59c67f44f245c38ca112f97291361e13cf05af0f0f991ea059c9b3`）§10「首条交付合同 AW-CONTRACT-001」所需的**最小**机读合同与样例，供 `../tools/contract_check.mjs` 消费。
- 依赖：JSON Schema draft-07；开发期校验器 Ajv 8.17.1（registry 版本/integrity 已核对）。Node 仅是开发校验工具，不是 Rust 产品必需运行时。
- 调用方：`npm run check`；`.github/workflows/contracts.yml`（定义已准备，本卡未触发 → NOT_RUN）。
- 不变量：`TEST_FIXTURE/metadata_only` 溯源在 `fixtures.json` 外包装 `test_fixture`（或文档内可选 `fixture` annotation），**schema 不强制产品合同带该字段**，故非 fixture 消费者可用；`UNKNOWN/NOT_RUN` 不得默认 `PASS`；`PARTIAL` 不得 Whole-Mesh 语义 `PASS`；未实现规范化器只能 `NOT_RUN`；公共引用不得暴露磁盘路径/UNC/凭据 URL；引擎版本 `UNKNOWN` 不阻断 S1/S2 文件/契约检查。形状校验不授予任何授权、不证明 F0/AI/引擎成功。

## 1. 范围（只做首条实际消费者）

单份 schema `first-delivery-contract.schema.json` 描述一份「首条交付合同实例」。字段仅来自现行 CONTRACTS 中真正被输入、组件引用、锁定声明、增量规格、检查报告消费的部分；不为空想对象造空 schema，不造生成器平台/SDK 矩阵/完整媒体模型。合同独立于 DB、模型、厂商与 Agent；公共引用字段不接受本地绝对路径、SQL 或凭据。

| 合同字段 | 回指现行 CONTRACTS | 为什么必要 |
|---|---|---|
| `asset.{asset_id,version_id,content_hash}` | §1 | 区分资产身份、不可变版本、确切文件字节 |
| `input.{input_id,authorization,license,files,source_format,units,known_correct}` | §10.3 S1、§3 | 授权与已知正确性不得由 fixture/占位补齐；未声明记 UNKNOWN |
| `components[]` (source 必须指向已声明文件且 content_hash 相符) | §2 | 静态资产只含所需组件；不存在组件不得用空文件伪装 |
| `lock_declaration.{editable_components,locked_components[]}` | §3、§10.2 | 可写/锁定范围预先声明；BYTE/SEMANTIC 先声明后执行 |
| `locked_components[].{canonicalization,digest_scope,baseline_digest,after_digest}` | §10.2 规则1-3 | 语义锁须声明 canonicalization；PARTIAL 不得 Whole-Mesh PASS |
| `increment.{base_version,input_refs,output_contract,budget,allowed_environment}` | §3、§10.4 S3 | 单次动作增量；未知费用记 unknown 不填 0 |
| `validation_report.{checker_version,engine_version,scope,items[],not_run,manual_handling,artifact_refs}` | §8、§10.5 | 报告须能回答范围/逐项结果/未执行项/原始产物引用 |

字段按消费段拆分，`lock_declaration`（S2 起）与 `increment`（S3 起）可选；缺失只阻断该段及其下游。S3 额外要求具体 `engine_version`；S1/S2 的 `UNKNOWN` 明确允许。`fixture` 为可选 annotation，不再是产品合同必需字段。

## 2. 错误语义（回指 §10.5）

`tools/contract_check.mjs` 对每个 fixture 输出以下结果名；gate 退出码沿用 §10.5 命名：

| 结果名 | 触发 |
|---|---|
| `VALID` | 结构(Ajv) + 显式语义均通过；**仅文档级通过**，不是执行 PASS |
| `INVALID_INPUT` (2) | 缺字段/不可解析/非法数值；未知费用填 0；公共引用越界；信封非法 |
| `INPUT_INCOMPLETE` (3) | 该段必需字段缺失（如 S3 无 increment 或 engine UNKNOWN） |
| `LOCK_VIOLATION` (4) | 锁定/可编辑冲突；按 lock_mode 比较前后摘要不等 |
| `FAIL` (1) | 坏引用、NOT_RUN 冒充 PASS、PARTIAL 冒充 Whole-Mesh 语义 PASS |

`NOT_RUN ≠ FAIL ≠ WAIVED ≠ PASS`；`PASS` 只属于实际声明范围。stdout 只输出单个机器 JSON，日志走 stderr；非零退出不得当 PASS。

## 3. 显式语义规则（Ajv 只做结构，不重写通用引擎）

- 信封（前置）：`fixtures[]`/`expectations[]` 非空、ID 唯一、`expect` 合法、一一覆盖、外包装 `test_fixture` 溯源齐全；否则 `INVALID_INPUT`(2)，空测试集绝不返回 PASS/0。
- R-fixture：测试溯源在外包装；可选 annotation 必须一致；metadata-only 不得冒充真实授权或 known_correct F0。
- R0：按消费段拆分字段；S1/S2 允许 engine UNKNOWN。
- R-public：公共引用最小 deny-list（拒本地绝对 Unix/Windows/UNC、`file:`、路径遍历、URL authority 带 userinfo 的凭据形式；保留 opaque/relative 与合成 `metadata://`）；不造存储协议或通用 secret scanner。
- R1：组件角色、组件 `source`→已声明文件且 `content_hash` 等于该文件声明 sha256（不读文件字节）、`input_refs`/`artifact_refs` 引用完整性。
- R2：`editable_components` 与 `locked_components` 交集必须为空。
- R3：`cost_status=unknown` 时 `cost_value` 必须为 `null`（不填 0）。
- R4：`claims_whole_mesh_semantics=true` 只许 `status=PASS` 且 `digest_scope=FULL`；`NOT_RUN/INPUT_INCOMPLETE/NONDETERMINISTIC/WAIVED` 不得宣称该语义。
- R5/R6：每个锁定组件按预声明 `lock_mode` 比较自身 `baseline_digest`/`after_digest`；BYTE 直接比字节哈希，SEMANTIC 仅在 `canonicalization.status=IMPLEMENTED` 时比；未实现规范化器、缺摘要或 `PARTIAL` 只能 `NOT_RUN`，报告不得比计算结果更强。

## 4. Fixtures

集中在 `fixtures/fixtures.json`，期望值在 `fixtures/manifest.json`（文档内不含期望）。每条外包装带 `test_fixture.{marker=TEST_FIXTURE,metadata_only=true,note}`；文档内 `fixture` 仅为可选 annotation。H_A/H_B/H_C 是占位 64 位十六进制，不是任何真实字节哈希。共 14 例（4 正 / 10 反）：3 个原正例、7 个原负例，加 4 个定向回归（非 fixture 产品形状、未声明组件 source、组件 content_hash 不符、本地绝对公共引用）。信封回归探针在 `../work/aw003-contracts/envelope-probes/`（空/重复/孤儿/非法 expect/缺覆盖，均须 `INVALID_INPUT`(2)）。

**Fixtures 严格为 TEST_FIXTURE/metadata-only**：不代表真实 GLB/Rig/Skin/Motion、F0 输入、AI 生成、引擎导入或视觉验收成功。`engine_import`、`real_generation`、`gltf_validator` 一律 NOT_RUN。未知规范化器不自造实现。

## 5. 复跑

```sh
npm ci
npm run check     # 等价: node tools/contract_check.mjs --manifest ... --fixtures ... --schema ...
```

stdout 为机器结果，stderr 为日志；exit 0 表示 14/14 期望匹配，非 0 表示 gate 失败。本卡实际命令、原始退出码、stdout/stderr 文件与 schema/source hash 见 `../work/aw003-contracts/REPORT.md`。

## 6. 可选 CI 定义（本卡 NOT_RUN）

`.github/workflows/contracts.yml`：已移除 workflow 级 `paths` 过滤（必过检查不得被 paths 跳过）、加 `permissions: contents: read` 与显式 `timeout-minutes: 10`、action 固定到经官方 GitHub API 核验的 commit SHA（`actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683` 沿用 AW002 官方核验并 API 复核；`actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020` 由 `GET /repos/actions/setup-node/git/ref/tags/v4` 解析，记录见 `../work/aw003-contracts/ci-action-verification.json`）。**未触发、未由本卡启用**；是否接入必过检查由协调者后续决定，不声称 CI 就绪。

## 7. 未覆盖 / NOT_RUN

- 无 CI/GPU/Windows/引擎/外网生成执行；引擎导入、真实生成、glTF Validator、Rust 产品构建、C1 gate、语义规范化器实现 → NOT_RUN / NOT_DEFINED。
- `tools/verify_bundle.mjs --content-only` 本卡 NOT_RUN（遍历整棵树含新装 `node_modules/`；未修改该工具）。
- 外网执行仅 npm registry 只读核对 ajv 与 GitHub API 只读核验 action SHA；无生成/付费/API 调用。
- 官方 CodeBuddy 客户端实际 model/effort 档位 UNKNOWN，未声称设定、未为此搜工具。
- 费用：unknown（0 次付费调用；未知费用字段保持 null，不填 0）。
- `ponytail` 插件不可用（NOT_FOUND）：按 需求→已有代码→标准库→平台原生→已装依赖→最小充分变更 选择 Ajv；代码索引/why 关联由协调者等价处理。
- 本阶段零 Git 写入；未修改现行 `docs/CONTRACTS.md`、任务状态、Cargo*/crates/、`tools/c1.sh`、`.github/workflows/c1.yml`、原始 archive/evidence 与父工作区。
