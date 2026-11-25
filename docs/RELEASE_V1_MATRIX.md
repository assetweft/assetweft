# AW-036:B Release V1 验收矩阵

本表基于 `048a365635bdd6d335cc186b5e497082dbfb6d05` 的代码、门禁及当前 closeout 复验。
卡片编号据 [历史任务卡](https://github.com/assetweft/assetweft/blob/af53d782c6c316cb6fc23013efb8c5f61d6d413d/tasks/TASK_CARDS.md)，原卡在当前树已移出。现行入口及路径见下表和 [Feature Map](FEATURE_MAP.md)。

- `VERIFIED_OFFLINE` 表示本地自动化、固定夹具或源码门禁覆盖所列范围。Core/CLI 测试默认归 [C1] 的 workspace gate，示例套件按各自 README 执行。
- `VERIFIED_CI_DUAL_PLATFORM` 仅标注 [基线 Native CI](https://github.com/assetweft/assetweft/actions/runs/45565911681) 的 macOS/Windows 两条实测作业。独立应用步骤受路径筛选，截图不在 CI 门禁内。
- `NOT_RUN` 适用于未实测、未实现及本轮不适用的运行验收，原因随项记录。公开完整 V1 保持 `NOT_RUN`，直到未兑现项验收或负责人明确批准延期。

## 卡片与验收入口

| 卡片 | 范围 | 入口、证据与边界 |
| --- | --- | --- |
| AW-001 | 需求、决策与证据准入 | [开发规范](DEVELOPMENT_STANDARDS.md) 与历史卡片，当前负责人签收 `NOT_RUN` |
| AW-002 | Rust 工程与 GitHub 门禁 | `cargo build --workspace --locked`，[C1] → `VERIFIED_OFFLINE` |
| AW-003 | 资产合同及夹具 | `node tools/contract_check.mjs`，[合同](../contracts/README.md) 与 [Contracts CI](../.github/workflows/contracts.yml) → `VERIFIED_OFFLINE` |
| AW-004 | SQLite 持久化与恢复 | [恢复测试](../crates/assetweft-core/tests/native_recovery.rs)，[Native CI] → `VERIFIED_CI_DUAL_PLATFORM` |
| AW-005 | 桌面视口候选与编辑 | [固定网格应用](../examples/desktop-viewport/README.md) 与 [比较器](../examples/desktop-viewport/compare.mjs)，见 #99；高级编辑 `NOT_RUN` |
| AW-006 | 模型、执行器与许可准入 | `byo-gpu profiles` / `kaggle-validate`，[执行契约] 与 [BYO 测试](../crates/assetweft-cli/tests/byo_gpu.rs) → `VERIFIED_OFFLINE`；外部许可准入 `NOT_RUN` |
| AW-007 | 真实生成、消融与压力实验 | [模型可行性 workflow](../.github/workflows/model_runner_benchmark.yml)；真实素材生成及消融 `NOT_RUN` |
| AW-008 | 共享 Core 与入口生命周期 | `assetweft library init`，[CLI 测试] 与 [应用测试](../crates/assetweft-core/tests/application.rs) → `VERIFIED_OFFLINE`；按需服务 `NOT_RUN` |
| AW-009 | SQLite 模式与迁移 | `library init`，[迁移测试](../crates/assetweft-core/tests/library.rs) → `VERIFIED_OFFLINE` |
| AW-010 | CAS 与版本提交 | `catalog commit-object` / `register-version`，[存储测试](../crates/assetweft-core/tests/storage.rs) → `VERIFIED_OFFLINE` |
| AW-011 | 检索、历史、反馈 | `catalog search` / `feedback-add` / `feedback-get`，[历史测试](../crates/assetweft-core/tests/history.rs) → `VERIFIED_OFFLINE` |
| AW-012 | 整库备份与恢复 | `library backup` / `restore`，[备份测试](../crates/assetweft-core/tests/backup.rs) → `VERIFIED_OFFLINE`；双平台整库备份复验 `NOT_RUN` |
| AW-013 | 规格与属性锁 | `plan --request … --trusted-context …`，[规格测试](../crates/assetweft-core/tests/generation_spec.rs) → `VERIFIED_OFFLINE`；完整 SEMANTIC 路径 `NOT_RUN` |
| AW-014 | 持久工作流 | `workflow create` / `claim` / `finish` / `status`，[工作流测试](../crates/assetweft-core/tests/workflow.rs) → `VERIFIED_OFFLINE` |
| AW-015 | 预算、用量与 UNKNOWN | `budget approve` / `reserve` / `receipts`，[预算测试](../crates/assetweft-core/tests/budget.rs) → `VERIFIED_OFFLINE`；实付 `NOT_RUN` |
| AW-016 | 候选与原子接受 | `candidate propose` / `attempt` / `receipt`，[候选测试](../crates/assetweft-core/tests/candidate.rs) → `VERIFIED_OFFLINE` |
| AW-017 | 独立文件检查 | `inspect` / `validate`，[验证测试](../crates/assetweft-core/tests/validation.rs) → `VERIFIED_OFFLINE` |
| AW-018 | 执行协议与运行桥 | `byo-gpu run`，[执行契约] 与 [运行桥测试] → `VERIFIED_OFFLINE`；见 #28 |
| AW-019 | Agent 接入与兼容 | [CLI Skill](../skills/assetweft-cli/SKILL.md)；真实 Agent/多模型互操作 `NOT_RUN` |
| AW-020 | 首条真实 3D 生成 | `byo-gpu run` / `kaggle-validate`，[BYO 测试](../crates/assetweft-cli/tests/byo_gpu.rs) → `VERIFIED_OFFLINE`；真实执行与生成验收 `NOT_RUN` |
| AW-021 | 语音、SFX、可选 BGM | `audio-connector-inspect`，[授权及音轨测试](../crates/assetweft-core/tests/audio_connector.rs) → `VERIFIED_OFFLINE`；TTS/BGM 生成与试听 `NOT_RUN` |
| AW-022 | 静态几何、Rig/Skin | `validate` / `rig-bend`，[几何测试](../crates/assetweft-core/tests/geometry.rs) 与 [Rig 测试](../crates/assetweft-core/tests/rig.rs) → `VERIFIED_OFFLINE`；交互编辑 `NOT_RUN` |
| AW-023 | 动作、重定向、多角色 | `motion-strike` / `motion-retarget` / `motion-interaction`，[重定向测试](../crates/assetweft-core/tests/motion_retarget.rs) 与 [交互测试](../crates/assetweft-core/tests/motion_interaction.rs) → `VERIFIED_OFFLINE` |
| AW-024 | Sprite 与 UI/卡牌/地块 | `sprite-inspect` / `card-template-check`，[Sprite 测试](../crates/assetweft-core/tests/sprite.rs) 与 [模板测试](../crates/assetweft-core/tests/card_layout.rs) → `VERIFIED_OFFLINE` |
| AW-025 | 面部、刚性挂件、蒙皮服装 | `face-attachment-inspect` / `cloth-inspect`，[挂件测试](../crates/assetweft-core/tests/face_attachment.rs) 与 [服装测试](../crates/assetweft-core/tests/cloth.rs) → `VERIFIED_OFFLINE`；编辑器/GPU 布料 `NOT_RUN` |
| AW-026 | VFX 与动作表现 | `vfx-inspect` / `action-presentation-inspect`，[VFX 测试](../crates/assetweft-core/tests/vfx.rs) 与 [动作表现测试](../crates/assetweft-core/tests/action_presentation.rs) → `VERIFIED_OFFLINE`；引擎播放 `NOT_RUN` |
| AW-027 | 本地 WAV 与循环 | `audio-inspect`，[音频测试](../crates/assetweft-core/tests/audio.rs) → `VERIFIED_OFFLINE`；ASR/试听 `NOT_RUN` |
| AW-028 | 建筑和植被套件 | `static-kit-inspect`，[套件测试](../crates/assetweft-core/tests/static_kit.rs) → `VERIFIED_OFFLINE`；引擎视觉 `NOT_RUN` |
| AW-029 | Unity A–D | `engine-contract-check`，[引擎测试] → `VERIFIED_OFFLINE`；live Unity editor 导入、渲染与表现 `NOT_RUN` |
| AW-030 | Unreal A–D | `engine-contract-check`，[引擎测试] → `VERIFIED_OFFLINE`；live Unreal editor 导入、渲染与表现 `NOT_RUN` |
| AW-031 | Godot A–D | `engine-contract-check`，[引擎测试] → `VERIFIED_OFFLINE`；[静态探针](../examples/godot-static/README.md)、live Godot editor 导入、渲染与表现 `NOT_RUN`（本轮） |
| AW-032 | Workbench A–C | [本地 Workbench](../apps/workbench/README.md) 的 Catalog、Generation & Budgets、Incremental Candidates、Capabilities；[Native CI] 执行适用的 `check:tokens` / `npm test` / Tauri 检查，实时专业编辑与产品验收 `NOT_RUN` |
| AW-033 | CLI、共享入口与可选 MCP/SDK | [CLI 文档](../crates/assetweft-cli/README.md) 与 [CLI 测试] → `VERIFIED_OFFLINE`；可选 MCP/SDK `NOT_RUN` |
| AW-034 | Skill 与两条操作配方 | [离线配方](../skills/assetweft-cli/eval_recipe.sh) 与 [复用恢复配方](../skills/assetweft-cli/eval_extended_recipe.sh)，相关 Core/CLI 由 [C1] 测试；本轮独立配方执行及真实 Agent 观察 `NOT_RUN` |
| AW-035 | 角色闭环、表现包与 BM-01..BM-06 | `rigged-simple-components`，[角色闭环](../examples/rigged-simple/verify_character_loop.sh)、[表现包]、[非角色套件]；B/C 夹具 `VERIFIED_OFFLINE`；引擎、独立 TA 与用户验收 `NOT_RUN` |
| AW-036 | 技术预览与公开发行 | [技术预览](TECH_PREVIEW.md)、[预览门禁](../tools/tech_preview_check.py)、[矩阵门禁](../tools/test_release_v1_matrix.py) → `VERIFIED_OFFLINE`；公开完整 V1、installer code-signing/notarization 与签名更新 `NOT_RUN` |
| AW-037 | 可选账户与自愿统计 | [Workbench](../apps/workbench/README.md) 启动无需账户；账户与遥测入口 `NOT_RUN` |
| AW-038 | 可选 GitHub 项目工作流 | [本地生产契约](../crates/assetweft-core/src/production/CONTRACT.md)；产品内 GitHub 远端任务 `NOT_RUN` |
| AW-039 | 未来云端同步 | [Catalog 契约](../crates/assetweft-core/src/catalog/CONTRACT.md) 提供版本/对象基础；双端同步 `NOT_RUN`，见 R13 |
| AW-040 | 市场、许可与积分 | [非角色套件] 单列市场假设；商业规则准入与市场运行 `NOT_RUN` |

## #142 八项不变量

| 项 | 当前检查与剩余边界 |
| --- | --- |
| 1. G-DET-1 / DeterminismPolicy | [规范化实现](../crates/assetweft-core/src/assurance/determinism_domain.rs) 与 [确定性测试](../crates/assetweft-core/tests/determinism.rs) → `VERIFIED_OFFLINE`，覆盖负零、非有限数、1e-6 ties-even、四元数半球和样本哈希。全链路 SEMANTIC 执行及禁 FMA/libm 跨平台保证 `NOT_RUN`。 |
| 2. Q6 | [表现包] 的 BYTE 锁挂点对照 0.5 mm PASS、0.6 mm FAIL，另核固定挂点网格摘要 → `VERIFIED_OFFLINE`。完整 Mesh/Material 语义锁、旋转 ≤ 0.1° 与同一 Q6 A/B 脚滑不恶化 `NOT_RUN`。 |
| 3. EngineTransformContract | `engine-contract-check` 与 [引擎测试] → `VERIFIED_OFFLINE`。校验 glTF 到三引擎的轴/单位变换、Unity/Unreal 手性与绕序、逆绑定矩阵共轭、ORM/MetallicSmoothness 与线性数据纹理，真实编辑器结果 `NOT_RUN`。 |
| 4. Q1/E08/E11 | `audio-connector-inspect` 与 [许可测试](../crates/assetweft-core/tests/audio_connector.rs) → `VERIFIED_OFFLINE`，区分代码、权重、数据集、ToS 与各轨来源，拒绝进程内 GPL piper1-gpl。Apache-2.0 OR MIT 双许可发版、闭源 Autodesk FBX SDK 排除的完整发布审计、生产 CAS 去重后溯源与实际供应商权利核验 `NOT_RUN`。 |
| 5. HY4-P0-02 | [隔离前端](../examples/desktop-viewport/src/main.ts) 与 [Tauri 命令](../examples/desktop-viewport/src-tauri/src/main.rs) 使用 ArrayBuffer / raw IPC Response，复用单渲染器并释放几何、材质和渲染器。Workbench [预览组件](../apps/workbench/src/ui/patterns/preview-stage.ts) 遍历释放纹理、材质、几何与渲染器，GLB 仍经 [桥](../apps/workbench/src/bridge.ts) 的 JSON Base64 并解码为字节。静态检查 `VERIFIED_OFFLINE`；异步 URI + Range、大资产流式传输、Workbench 二进制 IPC 与显存压力验收 `NOT_RUN`。 |
| 6. Q8 | [Workbench token 入口](../apps/workbench/README.md) 与 [Native CI] 提供 token、组件测试及构建检查。设计定稿、前置独立评审及用户观察的发布证据 `NOT_RUN`。 |
| 7. V1 Non-Goals + BM-01..BM-06 | [非角色套件] 记录静态网格、骨骼、2D、静态套件、表现包、恢复六项 → `VERIFIED_OFFLINE`。游戏运行时 AI、商业资源提取、完整 DCC 替代与强制云账号为 V1 非目标。实际用户任务观察、live engine 视口与市场假设 `NOT_RUN`。 |
| 8. R13 | [Catalog 契约](../crates/assetweft-core/src/catalog/CONTRACT.md) 与 [备份测试](../crates/assetweft-core/tests/backup.rs) 提供本地不可变对象/版本与恢复验证。将来同步需显式启用，仅用不可变版本/哈希/清单/对象，禁止自动上传与直接同步 SQLite 原目录。同步实现及双端故障验证 `NOT_RUN`。 |

## #99 视口与 #28 金额精度

- #99 的 [隔离视口](../examples/desktop-viewport/README.md) 使用固定三角网格。macOS Tauri 窗口的 canvas 导出通过比较器，`--wrong-bounds` 被拒后用同一对文件恢复 PASS。macOS OS screenshot/ICC、Windows/Linux native viewport captures、缩放及独立窗口验收仍为 `NOT_RUN`。
- #28 的 [运行桥测试] 与 [运行桥实现](../crates/assetweft-core/src/execution/runtime_application.rs) → `VERIFIED_OFFLINE`。协议支持 0–18 位，预算上限 9 位；10–18 位保留原观察并报告 UnsupportedScale，超出 i64 上限报告 AmountOverflow。[协议入口](../crates/assetweft-core/src/execution/application.rs) 拒绝 19 位，专门的 19 位复验 `NOT_RUN`。离线桥验证预留保留、拒绝入账及调用边界，真实供应商结算与授权 `NOT_RUN`。

## 运行时与发布边界

核心运行时为本地 Rust CLI/Core 和嵌入式 SQLite/CAS。桌面增加对应系统 WebView（macOS WKWebView、Windows WebView2，Linux 原生验收 `NOT_RUN`）。构建和复验使用 Rust、Node.js、Python 3，非角色套件另用 sqlite3 CLI。仓库 [预览门禁](../tools/tech_preview_check.py) 检查禁用依赖及字面 Python 启动入口。PostgreSQL、常驻守护进程、LLM、Python 运行时与云账号均无核心运行时要求；BYO Python、Blender 和外部引擎按独立检查选用。

- Live Unity/Unreal/Godot editors、live paid GPU/TTS providers、real TA/user observations、installer code-signing/notarization 和 Windows/Linux native viewport captures → `NOT_RUN`。
- 复验执行 `python3 -m unittest discover -s tools -p 'test_*.py'`、[C1]、[表现包]、[非角色套件] 与 #99 比较器正反对照。公开 V1 另需 AW-036:B 的三引擎、原生恢复、签名更新及许可准入证据。

[C1]: ../.github/workflows/c1.yml
[Native CI]: ../.github/workflows/native-recovery.yml
[执行契约]: ../crates/assetweft-core/src/execution/CONTRACT.md
[运行桥测试]: ../crates/assetweft-core/tests/executor_runtime.rs
[CLI 测试]: ../crates/assetweft-cli/tests/cli.rs
[引擎测试]: ../crates/assetweft-core/tests/engine_contract.rs
[表现包]: ../examples/presentation-bundle/README.md
[非角色套件]: ../examples/non-character-suite/README.md
