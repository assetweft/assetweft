# AssetWeft（游织）v1.0 多模型架构联合审计与业主决策问题清单

| 文档属性 | 内容 |
|---|---|
| 文档编号 | `AW-DOC-AUDIT-001` |
| 唯一维护位置 | `docs/AUDIT_AND_OPEN_QUESTIONS.md` |
| 基线依据 | `docs/DEVELOPMENT_BASELINE.md` (`AW-DEV-001`, SHA-256: `b15d51b3048a0d95424b514e59cc6331a620f302905277b5801e78ae86cd7fe1`) |
| 参与审计通道 | 主控（Antigravity Gemini 4 Argon）+ Orca Worker 1（`DSH` 车道，`task_210d1dd9aaa6`）+ Orca Worker 2（**腾讯官方 `CodeBuddy / Hy4` 车道**，`task_b2f3207a0d10` / `ctx_f4c916e98d59`，`hy4free high`）+ 早期 `OpenCode` 车道（`task_4543a26b81b9`） |
| 核查日期 | 2026-11-11 |

---

## 一、 多模型联合审计执行记录与证据链

按照指令（不调用 Claude / Codex，通过 Orca 编排 `DSH` 与官方 `Hy4 / CodeBuddy` 车道，遵循唯一权威），本次对 `DEVELOPMENT_BASELINE.md` 及 `docs/ARCHITECTURE.md` 进行了多模型独立交叉审计与一手外部规范核验：

| 审计通道 | Orca 任务 / Dispatch ID | 负责模块与维度 | 产出报告与 SHA-256 | 发现统计 |
|---|---|---|---|---|
| **DSH 车道** | `task_210d1dd9aaa6` / `ctx_a5c89b52698a` | M1/M2/M4、内置 PG18 生命周期、单核心进程租约与 IPC、CAS 事务与 GC、DAG 预算与 `REMOTE_UNKNOWN` 对账、表归属与 `sqlx` | `work/arch-audit-20261111/reports/AUDIT-DSH.md` (`cdd04badab392f156a3ccad2ea47061bc8086c562fdf14557e97adf957a723ed`) | **5 P0 / 9 P1 / 9 P2**（共 23 项） |
| **官方 Hy4 (`CodeBuddy`) 车道** | `task_b2f3207a0d10` / `ctx_f4c916e98d59` (`--agent codebuddy`, `hy4free high`) | M3 跨平台浮点确定性、M5 三引擎坐标/手性/PBR/IBM 转换数学契约、`ExportManifest v2`、CAS 状态机与 GC 锁内隔离复核、WebGL 显存释放、外部进程沙箱 | `work/arch-audit-20261111/reports/AUDIT-HY4-CODEBUDDY.md` (`12066d05f09085e98dfcd50fa6f466f37a322dd879b529f4ff884063d8462d3e`) | **4 P0 / 10 P1 / 9 P2**（共 23 项） |
| **OpenCode 辅助车道** | `task_4543a26b81b9` / `ctx_e3dc56106924` | M3/M5/M6、Unreal FBX 许可证冲突、Tauri+Three.js 二进制传输、许可证准入隔离、40 卡 DAG 拓扑与 GitHub CI 门禁 | `work/arch-audit-20261111/reports/AUDIT-HY4.md` (`65e71dc2f74742f740bee183b205d6f2335ef4767e2371d3336e4dee0654e82d`) | **3 P0 / 8 P1 / 5 P2**（共 16 项） |
| **主控权威实测** | Humaux `1164456b` & `9cb6c535` | 穿透核查 E01–E11 官方文档与 GitHub API 仓库精确 HEAD SHA / SPDX 许可证，汇总并落地 `PRODUCT.md`、`ARCHITECTURE.md`、`sources.yaml`、`capabilities.yaml` | `evidence/sources.yaml` · `docs/ARCHITECTURE.md` | 锁定 11 项外部权威与 21 表架构契约 |

---

## 二、 核心架构不完善点与完善结论汇总（12 项 P0 级关键缺口全量闭环）

基线文档（`AW-DEV-001`）在产品方向、模块边界、反腐败层和风险前置（W1 三大原型）上设计非常扎实，但在进入代码编写前，经 `DSH` 与官方 `CodeBuddy (Hy4)` 深度审计共识别出 **12 项“原则已写、但缺少可执行工程/数学契约”的 P0 级缺口**。现已在 [`docs/ARCHITECTURE.md`](file:///Volumes/data/assetweft_delivery/docs/ARCHITECTURE.md) 与 [`evidence/sources.yaml`](file:///Volumes/data/assetweft_delivery/evidence/sources.yaml) 中完成闭环补齐：

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
   - *已完善方案*：确立 `assetweft://blob/<sha256>` 自定义二进制协议（含 `OPTIONS` CORS 预检、`206 Range`、CSP 放行与递归 `disposeScene` 显存释放）；修正 `AW-016`、`AW-033a/b` 与 `AW-029~031` 的 DAG 依赖，并保持 `AW-022`（纯计算层）不逆向依赖 `AW-005`（UI 视口原型）（修补 `HY4-CB-P1-10`）。
9. **【P0 · 跨平台确定性】`macOS ARM64` 与 `Windows x86_64` 浮点计算哈希漂移（官方 Hy4 新增 `HY4-CB-P0-01`）**：
   - *缺陷根因*：ARM64 NEON FMA 硬件乘加融合、平台 `libm` 超越函数与 SIMD 归约顺序差异，会导致同一模型在 Mac 与 Windows 上算出不同的浮点末位，使 `component_manifest` 语义哈希与 `AW-016` 增量判据跨平台失效。
   - *已完善方案*：在 `ARCHITECTURE.md §3.4` 确立 `DeterminismPolicy`：所有进入语义哈希的浮点属性统一经过 `domain::canonicalize`（`-0.0 -> +0.0`、四元数强制 `w >= 0` 半球符号、`1e-6` 定点量化 `round_ties_even`、禁用 FMA、纯 Rust `libm`），并纳入双平台 CI 必跑门禁 `G-DET-1`。
10. **【P0 · 三引擎转换数学】坐标/手性/`inverseBindMatrices`/切线 `w`/厘米缩放缺少显式数学契约（官方 Hy4 新增 `HY4-CB-P0-02`）**：
    - *缺陷根因*：glTF（右手系 +Y Up 米）转 Unity（左手系 +Y Up 米）与 Unreal（左手系 +Z Up 厘米 $\times 100$）是手性反射变换（$\det(C) < 0$）。若只改坐标不反转三角形顶点绕序 `(i0, i2, i1)`、不翻转 MikkTSpace 切线 `tangent.w = -tangent.w`、不按相似变换 $\text{IBM}' = C \cdot \text{IBM} \cdot C^{-1}$ 重算蒙皮逆绑定矩阵，会导致“导入不报错但模型内翻、法线贴图反向、蒙皮糖果纸扭曲”。
    - *已完善方案*：在 `ARCHITECTURE.md §3.5` 确立 `EngineTransformContract`，锁定换基矩阵、绕序反转、切线手性翻转、IBM 相似变换与 Unreal $\times 100$ 全通道缩放公式。
11. **【P0 · 材质与色彩空间】PBR 贴图通道重打包与 sRGB/Linear 标志缺失（官方 Hy4 新增 `HY4-CB-P0-03`）**：
    - *缺陷根因*：glTF/Unreal 使用 `ORM (R=AO, G=Roughness, B=Metallic)`，而 Unity URP/HDRP 使用 `MetallicSmoothness (R=Metallic, A=1-Roughness)`；直接共用同一张贴图会导致金属度与粗糙度全错。
    - *已完善方案*：M5 导出时按目标引擎自动重打包贴图通道，并在 `ExportManifest v2` 中强制标记数据贴图（法线/ORM/MetallicSmoothness）为 `sRGB = false` 线性空间。
12. **【P0 · 交付清单完整性与 CAS 锁内删除】`ExportManifest v2` 扩展与 GC Phase C 锁内隔离（官方 Hy4 新增 `HY4-CB-P0-04 / P1-09 / P2-08`）**：
    - *已完善方案*：① 将 `gc_tombstones` 纳入第 2 章 **21 张表归属矩阵**（归属 M1），锁键改用 SHA-256 前 8 字节 64-bit 整数替代 `hashtext`，且 GC Phase C 在持有事务锁期间先原子 `rename` 到 `work/trash/` 再提交 `unlink`；② 将 `ExportManifest` 升级为 `v2`，补齐 `event_tracks`、`attach_points`、`root_motion`、`skeleton_mapping`、`material_channel_map`、`morph_target_curves` 与 Artifact 级 `license_chain`。

---

## 三、 业主（Owner）拍板决议与专项优化设计（2026-11-11 更新）

针对首轮提出的 8 个关键问题（`Q1–Q8`），业主已给出明确批复与深化要求，现将决议状态及针对 `Q2 / Q3 / Q6 / Q7 / Q8` 的专项升级设计正式锁定如下：

### A. 8 项核心问题决议总表

| 编号 | 决策领域 | 业主批复 | 现行锁定方案（已同步写入 `docs/ARCHITECTURE.md`） |
|---|---|---|---|
| **Q1** | **主仓库开源许可证（R02）** | **拍板：`A`** | 锁定 **`Apache-2.0 OR MIT` 双许可**（Rust 生态标准，含专利授权条款，商业友好）。 |
| **Q2** | **Unreal Engine 交付管线与 FBX 策略** | **要求给出更优、无需用户纠结底层格式的设计** | 升级为 **“三层无感开源交付架构”**（见下方 §B.1）：用户只需点「一键发送到 Unreal」，第 1 层走 Interchange 原生 glTF + C++ 插件内存补齐 UE 5.5/5.6+ 表情曲线；第 2 层内置纯 Rust 开源 FBX 写出器；第 3 层可选桥接本机 Blender CLI。**100% 零闭源 SDK 依赖、用户端零感知**。 |
| **Q3** | **桌面端内置托管 PostgreSQL 18 的风险与降级策略** | **询问具体有什么问题，并要求妥善防护** | 明确桌面端内嵌 PG18 的 **4 大真实痛点**并建立 **“自动自愈 + 外接 PG18 逃生阀”**（见下方 §B.2）：零固定端口冲突、残留 `postmaster.pid` 自动识别清理、中文/同步盘路径防护，且支持配置外部 `DATABASE_URL` 作为杀毒软件极端拦截下的逃生通道。 |
| **Q4** | **首期（W2–W4）优先打通的外部生成 Provider 清单** | **拍板：`A`** | 锁定：2D 优先接入 **OpenAI Images / Stability / Flux**；3D 优先接入 **Tripo3D / Meshy API + TRELLIS.2**；骨骼绑定接入 **UniRig**；音频接入 **ElevenLabs SFX + ACE-Step-1.5 / Kokoro-82M / piper1-gpl（外部隔离进程）**。 |
| **Q5** | **重型开源 AI 模型（UniRig / TRELLIS.2 / ACE-Step）部署形态** | **拍板：`A`** | 锁定：桌面客户端保持**纯 Rust 零 Python 依赖**；重型 Python/GPU 模型通过标准 Docker 镜像或远端推理 Endpoint（HTTP + OpenAPI）按需连接。 |
| **Q6** | **增量修改中“刚体挂点（Socket）”与“骨骼拓扑变更”边界** | **拍板：进行校准对比测试** | 升级为 **`AW-016 / AW-019` 强制姿态与挂点校准对比测试门禁（A/B Calibration Gate）**（见下方 §B.3）：锁定组件语义哈希必须 0 误差；挂点与重定向动画关键帧世界空间位置偏移 $\le 0.5\text{ mm}$、旋转误差 $\le 0.1^\circ$，生成量化校准报告。 |
| **Q7** | **预算控制（Budget）的计价粒度与法币单位** | **要求用直白方式讲清楚并简化设计** | 升级为 **“按平台原单位直显 + 双开关防刷爆”**（见下方 §B.4）：不搞跨币种自动汇率换算，美元显示 `$`、人民币显示 `¥`、积分平台显示 `Credits`；用户只需设「单次任务上限」和「项目总上限」，底层用整数存金额防小数误差。 |
| **Q8** | **桌面工作台（M6）首期视觉设计与流程** | **拍板：Mobbin $\to$ Eagle $\to$ Figma $\to$ 业主审核** | 锁定强制视觉门禁（见下方 §B.5）：先在 **Mobbin** 找对应工作台素材 $\to$ 全部存入 **Eagle** 并读回核验 $\to$ 在 **Figma** 中完成工作台页面设计 $\to$ **提交业主审核通过后**，才允许进入 `AW-025` 前端代码实现。 |

---

### B. 针对 Q2 / Q3 / Q6 / Q7 / Q8 的大白话拆解与更优工程设计

#### 1. 关于 `Q2`（Unreal 虚幻引擎交付与 FBX）：为什么会有问题？怎么设计更好？
- **大白话讲清痛点**：
  1. 3D 模型导进游戏引擎主要靠两种格式：**glTF 2.0**（现代开放标准，完全免费开源）和 **FBX**（老牌格式，归 Autodesk 公司私有）。
  2. 生成传统 FBX 文件通常要调用 Autodesk 官方的 **FBX SDK**，但它是**闭源专有软件**——如果把它的代码直接打包进我们开源的 AssetWeft（Apache-2.0/MIT）里，会违反开源协议，别人也没法合法分发我们的软件。
  3. 以前大家去 Unreal 习惯用 FBX，是因为早期 Unreal（如 UE 5.4/5.5）自带的 glTF 导入器有个小缺陷（捏脸表情动画曲线会漏导）；而在最新的 **UE 5.6+** 中，Epic 官方的新管线（Interchange）已经原生支持把 glTF 2.0 的网格、骨骼、蒙皮、表情完整导进去了。
- **更好的设计（“三层无感开源交付架构”——用户不用懂格式，点一下就行）**：
  用户在 AssetWeft 工作台里只管点 **「一键发送到 Unreal」**，不需要做任何格式选择，由系统自动按三层处理：
  - **第 1 层（默认主通道，零外部软件依赖）**：AssetWeft 直接输出标准 `glTF 2.0 (.glb)` + `ExportManifest.json` 清单。我们的 Unreal C++ 插件调用 Unreal 自带的 **Interchange 原生管线**导入；如果插件检测到用户用的是老版本 UE 5.5，**我们的 C++ 插件直接读取 `ExportManifest.json`，在 Unreal 引擎内存里自动把表情动画曲线补齐**！这样无论是 UE 5.5 还是 UE 5.6+，都不用转 FBX 就能完美导入！
  - **第 2 层（纯 Rust 开源 FBX 写出器）**：如果用户是美术外包团队，甲方硬性规定必须交 `.fbx` 文件，我们使用纯 Rust 开源二进制写出器生成静态网格与基础骨骼 `.fbx`，完全不碰 Autodesk 闭源 SDK。
  - **第 3 层（本机 Blender 无感桥接）**：如果遇到极复杂的自定义约束 FBX，且检测到用户电脑装了 Blender，自动在后台调用 `blender --background` 静默导出工业级 FBX 2020.2。

#### 2. 关于 `Q3`（桌面端内置托管 PostgreSQL 18）：到底有什么问题？怎么解决？
- **大白话讲清把 PostgreSQL 塞进桌面软件会遇到的 4 个真问题**：
  PostgreSQL 本来是给服务器设计的数据库。把它打包进普通用户的 Windows / Mac 桌面软件里跑，现实中会遇到 4 个坑：
  1. **Windows 杀毒软件拦截/锁死文件（最常见）**：Windows Defender、360、火绒看到一个桌面软件在后台悄悄启动 `postgres.exe` 十几个子进程并监听端口，经常会弹窗拦截，或者扫描时锁死数据库文件导致软件打不开。
  2. **中文电脑用户名 / 云盘目录报错**：如果 Windows 用户名带中文（如 `C:\Users\张三\...`）或目录在 OneDrive/iCloud 同步盘里，PostgreSQL 初始化很容易因编码或文件锁直接崩溃。
  3. **断电/强退后残留 `postmaster.pid` 导致打不开**：用户直接关机或强杀软件后，PostgreSQL 会留下一个记录进程号（PID）的锁文件。下次开机时如果这个进程号刚好被浏览器等别的软件占了，PostgreSQL 会误以为自己还在运行而拒绝启动。
  4. **端口冲突**：如果用户自己是个程序员，电脑上本来就装了 PostgreSQL 占用了默认的 `5432` 端口，固定端口就会冲突。
- **我们的闭环防护设计（自动自愈 + 外接逃生阀）**：
  1. **绝不占固定 5432 端口**：Mac 上直接走本地文件套接字（Unix Socket），Windows 上每次启动自动挑一个空闲端口（`49152–65535`）；
  2. **启动前自动体检与清理**：每次打开软件，Rust 守护进程先检查 `postmaster.pid` 里的进程是不是真的是 `postgres`，不是就自动备份清理；遇到中文路径或云盘自动映射到安全本地目录；
  3. **留一个“外接数据库”逃生开关**：万一某台公司电脑被安全策略锁死、绝对不允许桌面程序启动 `postgres.exe`，用户只需在设置里填一个外部 PostgreSQL 地址（比如本机已有的 PG 或 Docker 容器），软件就能正常跑，而且底层 SQL 代码 100% 通用，不用写两套数据库代码。

#### 3. 关于 `Q6`（刚体挂点 Socket 与骨骼变更边界）：强制校准对比测试门禁（Calibration Gate）
- 在 `AW-016`（增量提交）与 `AW-019`（骨骼重定向）中内置**自动化 A/B 姿态与挂点校准对比测试**：
  1. **未修改组件零漂移**：被锁定的 `Mesh`（网格顶点/拓扑）与 `Material`（材质参数）规范化语义哈希必须 **100% 相等（0 误差）**；
  2. **挂点与骨骼关键帧定量校准**：自动在动画关键帧（0% / 25% / 50% / 75% / 100% 帧）采样对比修改前后的世界空间坐标，要求**武器/特效挂点位置偏移 $\le 0.5\text{ mm}$、旋转角度误差 $\le 0.1^\circ$、脚底接触点滑动（Foot Skate）不恶化**，并自动生成校准对比表写入 `ValidationReport`。

#### 4. 关于 `Q7`（预算控制 Budget 的计价粒度与法币单位）：大白话解释与最简设计
- **大白话解释为什么之前写得绕**：
  因为不同 AI 平台收费方式完全不同：OpenAI 按**美元（`$`）**收，国内模型按**人民币（`¥`）**收，Tripo/Meshy 等 3D 平台按**积分/点数（`Credits`）**收，本地开源模型则是**免费（`0`）**。如果软件强行把美元、人民币、积分自动换算成同一种货币，一旦汇率波动或用户买了打折积分包，软件算的账就会和用户的真实账单对不上。
- **最简单好懂的设计（原单位直显 + 两个限额开关）**：
  1. **平台用什么单位，界面就显示什么单位**：美元通道显示 `$`，人民币通道显示 `¥`，积分通道显示 `积分（Credits）`，绝不强行跨币种换算；
  2. **用户只需设两个简单数字**：每个通道下只设 **「单次任务最高允许花多少」** 和 **「项目总共允许花多少」**，超了自动拦截，防止循环 Bug 把 API 额度刷爆；
  3. **底层整数存储（对用户透明）**：程序内部用整数（类似微信支付底层按“分/微元”存整数，不用小数点）记账，防止计算机小数点误差，界面上展示给用户的依然是直观的 `¥ 10.00`、`$ 5.00` 或 `200 积分`。

#### 5. 关于 `Q8`（M6 桌面工作台设计）：Mobbin $\to$ Eagle $\to$ Figma $\to$ 业主审核流程
- 严格按照您的指令执行工作台设计（对应 `AW-025` 前置门禁）：
  1. **Step 1 — Mobbin 素材搜集与 Eagle 入库**：围绕工作台 6 大核心页面（① 资产库与多模态检索、② 3D/2D/音频主视口与参数检查器、③ DAG 生产编排画布、④ 增量修改 A/B 锁定对比视图、⑤ 三引擎导出与质检报告面板、⑥ Provider 与预算上限设置），在 Mobbin 查找对应标杆界面素材，全部导入本地 **Eagle** 素材库并执行 `GET /api/item/list` 核验，输出素材清单；
  2. **Step 2 — Figma 工作台设计**：基于 Eagle 素材，在 Figma 中建立设计令牌（Variables）、组件库与完整工作台页面；
  3. **Step 3 — 业主审核（Owner Review Gate）**：将 Figma 设计稿与页面预览交由您审核，**经您审核通过后**再启动前端代码实现。

---

### B. 工程团队已设定的技术契约默认值（无需逐项回复，如有异议请指出）

以下纯工程参数已在 `docs/ARCHITECTURE.md` 中设定默认值：
1. **数据与对象目录默认位置**：默认存放在系统应用数据目录（macOS `~/Library/Application Support/AssetWeft/`，Windows `%LOCALAPPDATA%\AssetWeft\`），支持用户在设置中迁移 `objects/` 大文件库到本地高速硬盘（强制拦截网络盘与云同步盘）。
2. **多项目共享 CAS 去重**：同一资产库（`Library`）下的多个项目共享底层 `blobs` 物理去重，但通过 `blob_sources` 独立记录每次生成/导入的来源与许可证版本。
3. **核心进程空闲退出时间**：当无桌面 UI、无 CLI、无活跃 MCP 会话且无运行中/待对账任务时，核心进程与托管 PG18 在空闲 **300 秒**后优雅停止退出（支持配置为 `0` 在 MCP 宿主连接期间常驻）。
4. **CAS 孤儿文件延迟回收窗口**：未提交成功的临时文件 1 小时清理；无数据库引用的孤儿对象进入墓碑标记 **7 天 + 24 小时宽限期**后安全回收。
5. **全局主键格式**：统一使用时间有序的 **UUIDv7**，兼顾 PostgreSQL B-Tree 索引局部性与未来离线/云端同步免冲突需求。
