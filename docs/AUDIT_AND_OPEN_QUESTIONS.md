# AssetWeft（游织）v1.0 多模型架构联合审计与业主决策问题清单

| 文档属性 | 内容 |
|---|---|
| 文档编号 | `AW-DOC-AUDIT-001` |
| 唯一维护位置 | `docs/AUDIT_AND_OPEN_QUESTIONS.md` |
| 基线依据 | `docs/DEVELOPMENT_BASELINE.md` (`AW-DEV-001`, SHA-256: `b15d51b3048a0d95424b514e59cc6331a620f302905277b5801e78ae86cd7fe1`) |
| 参与审计通道 | 主控（Antigravity Gemini 4 Argon）+ Orca Worker 1（`DSH` 车道，`task_210d1dd9aaa6`）+ Orca Worker 2（`Hy4 / OpenCode` 车道，`task_4543a26b81b9`） |
| 核查日期 | 2026-11-11 |

---

## 一、 多模型联合审计执行记录与证据链

按照指令（不调用 Claude / Codex，通过 Orca 编排 `DSH` 与 `Hy4` 车道，遵循唯一权威），本次对 `DEVELOPMENT_BASELINE.md` 进行了三方独立交叉审计与一手外部规范核验：

| 审计通道 | Orca 任务 / Dispatch ID | 负责模块与维度 | 产出报告与 SHA-256 | 发现统计 |
|---|---|---|---|---|
| **DSH 车道** | `task_210d1dd9aaa6` / `ctx_a5c89b52698a` | M1/M2/M4、内置 PG18 生命周期、单核心进程租约与 IPC、CAS 事务与 GC、DAG 预算与 `REMOTE_UNKNOWN` 对账、表归属与 `sqlx` | `work/arch-audit-20261111/reports/AUDIT-DSH.md` (`cdd04badab392f156a3ccad2ea47061bc8086c562fdf14557e97adf957a723ed`) | **5 P0 / 9 P1 / 9 P2**（共 23 项） |
| **Hy4 / OpenCode 车道** | `task_4543a26b81b9` / `ctx_e3dc56106924` (自 `ctx_f8945e6c2b81` 套餐墙切免费车道续跑) | M3/M5/M6、Unreal FBX 许可证冲突、Tauri+Three.js 二进制传输、许可证准入隔离、40 卡 DAG 拓扑与 GitHub CI 门禁 | `work/arch-audit-20261111/reports/AUDIT-HY4.md` (`65e71dc2f74742f740bee183b205d6f2335ef4767e2371d3336e4dee0654e82d`) | **3 P0 / 8 P1 / 5 P2**（共 16 项） |
| **主控权威实测** | Humaux `1164456b-5d65-44f6-bc00-9ca824190a9a` | 穿透核查 E01–E11 官方文档与 GitHub API 仓库精确 HEAD SHA / SPDX 许可证，汇总并落地 `PRODUCT.md`、`ARCHITECTURE.md`、`sources.yaml`、`capabilities.yaml` | `evidence/sources.yaml` · `docs/ARCHITECTURE.md` | 锁定 11 项外部权威与 20 表架构契约 |

---

## 二、 核心架构不完善点与完善结论汇总（8 项 P0 级关键缺口）

基线文档（`AW-DEV-001`）在产品方向、模块边界、反腐败层和风险前置（W1 三大原型）上设计非常扎实，但在进入代码编写前，存在 **8 项“原则已写、但缺少可执行工程契约”的 P0/P1 级缺口**。现已在 [`docs/ARCHITECTURE.md`](file:///Volumes/data/assetweft_delivery/docs/ARCHITECTURE.md) 与 [`evidence/sources.yaml`](file:///Volumes/data/assetweft_delivery/evidence/sources.yaml) 中完成闭环补齐：

1. **【P0 · 资金安全】并发任务预算缺少“原子预留（Reservation）”表与单条 SQL 锁**：
   - *原基线缺陷*：仅要求“并发任务不会绕过预算”（L644），但 §8.3 的 11 张表中无预算表，若实现为“先查余额再提交”必然在并发下超支。
   - *已完善方案*：新增 `budget_accounts` 与 `budget_reservations` 表（归属 M2），强制使用单条 `UPDATE ... WHERE reserved + spent + overdraft + :cost <= limit` 原子预扣。
2. **【P0 · 资金安全】远端生成缺少“写前意图（Write-Ahead Intent）”，崩溃后易重复提交扣费**：
   - *原基线缺陷*：定义了 `REMOTE_UNKNOWN` 状态，但未规定在 HTTP POST 发送前必须先落盘意图记录。若 POST 发出后、收到 `remote_job_id` 前客户端崩溃，重启后无法对账，极易二次提交扣双份钱。
   - *已完善方案*：新增 `remote_jobs` 表（归属 M4），强制 `INTENT 落库 -> HTTP POST (带 idempotency_key) -> 回填 remote_job_id` 时序；`REMOTE_UNKNOWN` 状态下预扣额度保持 `HELD` 冻结，不释放为 0。
3. **【P0 · 数据安全】单核心进程缺少会话租约（Lease）、单实例文件锁与版本握手协议**：
   - *原基线缺陷*：规定桌面 UI、CLI、`MCP stdio` 共用单一按需启动核心进程，但未定义何时退出、如何互斥，以及客户端升级后旧核心未退出的协议冲突。同时未规定 `MCP stdio` 的 `stdout` 纯净性。
   - *已完善方案*：设计 `core.lock` 互斥 + `Hello/Welcome` 握手（校验 `proto` 与 `schema_version`）+ 会话租约（Lease）+ `DrainRequest` 平滑重启；`MCP stdio` 定为纯转发瘦代理，核心库开启 `#![deny(clippy::print_stdout)]`。
4. **【P0 · 数据安全】CAS 内容寻址文件库与 PG 事务跨介质崩溃窗口及 GC 误删竞态**：
   - *原基线缺陷*：先落盘文件再写数据库，若 GC 在写入事务提交前扫描 `objects/`，会把正在写入的新文件当成“无引用孤儿”删掉。
   - *已完善方案*：新增 `blob_intents` 表与三阶段 GC（Mark 墓碑 7 天 -> Grace 宽限 24 小时 -> Sweep 删除），且写入与 GC 删除共用 `pg_advisory_xact_lock(hashtext(hash))` 事务级互斥锁。
5. **【P0 · 架构契约】§8.3 的 11 张表与 §4.1/§6.1 实体冲突，且组件语义哈希与文件哈希混淆**：
   - *原基线缺陷*：11 张表缺失 `remote_jobs`、`validation_reports`、`export_manifests`、`reference_sets`、`blob_sources`；且 `AW-016` 要求“只改动作时 Mesh/材质/Rig 哈希不变”，若用 GLB 文件字节哈希判定，重导出时必然变化导致验收永远失败。
   - *已完善方案*：扩展为 **20 张表完整归属矩阵**（每表唯一写入模块）；在 `asset_versions` 中引入 `component_manifest`（按规范化顶点/骨骼/材质计算**组件级语义哈希**，与物理文件 `blob` 哈希解耦）。
6. **【P0 · 开源合规】Unreal FBX 2020.2 管线（E08/AW-030）与开源定位（R02）的许可证冲突**：
   - *原基线缺陷*：Autodesk FBX SDK 为闭源专有许可，严禁静态链接或打包进开源主仓库分发。
   - *已完善方案*：确立 Unreal 双轨交付——主路径采用 **UE 5.6+ 原生 Interchange Framework（glTF 2.0）+ Unreal C++ Editor 插件读取 `ExportManifest`**（经查证 UE 5.6 已修复 5.5 中 glTF Morph Target 动画曲线导入缺陷）；若需 FBX 2020.2，仅通过外部隔离进程（如用户本地 Blender CLI）转换。
7. **【P0 · 开源合规】Piper TTS（GPL-3.0）传染隔离与外部权威 SHA 锁定**：
   - *原基线缺陷*：`OHF-Voice/piper1-gpl` 为 GPL-3.0 强传染许可，不能内嵌或静态链接。
   - *已完善方案*：通过 GitHub API 实测锁定全部 11 项外部仓库精确 Commit SHA 与 SPDX 许可证（写入 `evidence/sources.yaml`）；将 `Kokoro-82M`（Apache-2.0）作为默认 TTS 候选，将 `Piper` 严格限制为“用户可选安装的外部独立进程”。
8. **【P0 · 视口与 DAG】Tauri 2 视口二进制传输协议与 40 卡 DAG 依赖修正**：
   - *原基线缺陷*：未规定 Rust 到 Three.js 的大资产传输方式（默认 JSON `invoke` 会内存翻倍卡死）；`AW-016` 漏掉了对 `AW-022`（M3 语义哈希）的依赖；`AW-033`（CLI/MCP）被 `AW-016` 硬阻塞导致介入过晚。
   - *已完善方案*：确立 `assetweft://blob/<sha256>` 自定义二进制协议直传 `ArrayBuffer`；修正 `AW-016`、`AW-022`、`AW-033a/b` 与 `AW-029~031` 的 DAG 依赖关系。

---

## 三、 需向业主（你）确认与拍板的决策问题清单

为了确保后续进入 `W0/W1` 代码实现时不猜需求、不偏离你的预期，我们将审计中识别出的问题分为 **“需要你拍板的核心产品与边界决策（共 8 题）”** 与 **“工程团队已设定的推荐技术默认值（若你无异议即按此执行）”** 两部分：

### A. 需要你拍板的 8 个核心决策问题

| 编号 | 决策领域 | 问题描述 | 备选方案与我们的推荐建议 |
|---|---|---|---|
| **Q1** | **主仓库开源许可证（R02）** | AssetWeft 主仓库（Rust 核心 + Tauri 桌面 + 三引擎插件源码）准备采用哪种开源许可证发布？ | **推荐：`Apache-2.0` 或 `MIT OR Apache-2.0` 双许可**（Rust 生态标准，且包含专利授权条款，商业友好，与 `rmcp`、`glTFast`、`Kokoro` 完全兼容）。 |
| **Q2** | **内置 PostgreSQL 18 的熔断退路（AW-004）** | 在 W1 阶段的 `AW-004` 原型验证中，如果发现完整打包 PostgreSQL 18 在受限 Windows 环境（如强力杀毒软件拦截子进程、安装包体积增加 ~150MB+）故障率超标，你更倾向哪种策略？ | **选项 A**：坚持内置原生 PostgreSQL 18，提供杀毒软件白名单引导，V1 不带跨大版本双份二进制（省体积）；<br>**选项 B**：通过 `persistence` 端口抽象，允许在桌面单机端退回嵌入式单文件引擎（或 SQLite 写队列），将 PG18 作为团队/专业模式后端。<br>*(当前 `AW-004` 默认按你的基线先实测原生 PG18)* |
| **Q3** | **Unreal Engine 目标版本与 FBX 策略（AW-030）** | 由于 Autodesk FBX SDK 闭源不可开源再分发，而 **UE 5.6+** 原生 Interchange 已大幅完善 glTF 2.0 骨骼/Morph/动画导入，你是否同意将 **UE 5.6+ Interchange (glTF 2.0) + C++ 插件** 作为 V1 默认主路线，而将 FBX 2020.2 定为“调用用户本机已安装的 Blender CLI 外部转换”的可选路线？ | **推荐：同意该双轨方案**（既 100% 守住 R02 纯开源红线，又能通过可选本地 Blender CLI 兜底传统 FBX 2020.2 需求）。同时请确认 Unity（建议 **Unity 6 / 6000.0 LTS**）与 Godot（建议 **Godot 4.3/4.4 Stable**）的最低支持版本。 |
| **Q4** | **V1 面部动画（Morph）与服装蒙皮的交付深度（AW-025 / AW-029~031）** | 在 V1 首版交付中，三引擎导入验收（`AW-029~031`）是否必须包含**面部 Morph Target 轨道与服装蒙皮迁移**，还是先以“标准人形 + 简化混沌 Boss 的骨骼动画、刚性挂点、特效与配音”作为 V1 必过门槛？ | **推荐**：将骨骼动画、刚性挂点与 `ActionPresentation`（动作+特效+音效+事件）作为 `AW-029~031` 的首批必过门禁；Morph 面部轨道与服装蒙皮在 `AW-025` 完成后作为增量兼容矩阵子项接入，避免阻塞基础交付。 |
| **Q5** | **增量修改冲突时的交互策略（AW-016）** | 当用户基于旧版本 `V1` 发起增量生成（如“新增攻击动作”），但在生成完成前该资产已被另一次操作更新到了 `V2`：系统应当如何处理？ | **推荐**：① 若 `V2` 未修改本次锁定的组件（如 Mesh 和 Skeleton 语义哈希与 `V1` 一致），允许自动 Rebase 挂载到 `V2`；② 若锁定组件已变，**不丢弃已生成的产物**，将其保存为分支候选版本并在 UI 并排提示用户选择，绝不静默覆盖。 |
| **Q6** | **`REMOTE_UNKNOWN` 与未知费用透支策略（AW-015）** | 当外部付费 API 超时或本地崩溃导致任务处于 `REMOTE_UNKNOWN`（无法自动确认服务商是否已扣费），或实际返回费用略高于预估预留额度时，如何处理？ | **推荐**：① `REMOTE_UNKNOWN` 的预扣额度保持 `HELD` 冻结占用，UI 提供请求摘要与时间戳供用户核对，满 24h 允许用户手动标记“已确认未扣费/已扣费”；② 实际费用超预估时如实记入 `overdraft_minor`（欠账），拦截后续新任务提交直到用户确认或调高预算。 |
| **Q7** | **首批实测生成 Provider 凭据与优先级（AW-006 / AW-007 / AW-019）** | 在推进 `AW-007`（真实生成可行性实验）与 `AW-019~021` 时，你手头目前已有哪些图像/3D/动作/音频服务商的 API Key（例如 Meshy、Tripo、自托管 TRELLIS.2/UniRig、OpenAI/Gemini/DeepSeek/Hy4 等）希望第一批优先实测接入？ | 请告知你目前希望优先接入的 **3D 生成与绑定服务商**，没有凭据的我们将严格保持 `NOT_RUN` 状态，不伪造测试结果。 |
| **Q8** | **目标硬件基线与 AW-005 视口性能 Go/No-Go 阈值** | 针对 `AW-005`（Tauri + Three.js 视口原型），我们拟定的通过阈值为：**50MB~100MB 含骨骼动画 GLB 首帧加载 < 3 秒、峰值内存增量 < 文件体积 2.5 倍、连续切换卸载 10 次后显存回落误差 ≤ 10%、60 FPS 预览（M1 Mac / 核显 Windows 11 基准）**。你对目标用户最低硬件配置或该性能门槛是否有调整？ | 若你觉得合适，即以此作为 `AW-005` 决定“保留 Three.js 还是替换原生渲染”的硬门禁。 |

---

### B. 工程团队已设定的技术契约默认值（无需逐项回复，如有异议请指出）

以下纯工程参数已在 `docs/ARCHITECTURE.md` 中设定默认值：
1. **数据与对象目录默认位置**：默认存放在系统应用数据目录（macOS `~/Library/Application Support/AssetWeft/`，Windows `%LOCALAPPDATA%\AssetWeft\`），支持用户在设置中迁移 `objects/` 大文件库到本地高速硬盘（强制拦截网络盘与云同步盘）。
2. **多项目共享 CAS 去重**：同一资产库（`Library`）下的多个项目共享底层 `blobs` 物理去重，但通过 `blob_sources` 独立记录每次生成/导入的来源与许可证版本。
3. **核心进程空闲退出时间**：当无桌面 UI、无 CLI、无活跃 MCP 会话且无运行中/待对账任务时，核心进程与托管 PG18 在空闲 **300 秒**后优雅停止退出（支持配置为 `0` 在 MCP 宿主连接期间常驻）。
4. **CAS 孤儿文件延迟回收窗口**：未提交成功的临时文件 1 小时清理；无数据库引用的孤儿对象进入墓碑标记 **7 天 + 24 小时宽限期**后安全回收。
5. **全局主键格式**：统一使用时间有序的 **UUIDv7**，兼顾 PostgreSQL B-Tree 索引局部性与未来离线/云端同步免冲突需求。
