# AssetWeft（游织）v1.0 系统架构与可执行工程契约

| 文档属性 | 内容 |
|---|---|
| 文档编号 | `AW-DOC-ARCH-001` |
| 唯一维护位置 | `docs/ARCHITECTURE.md` |
| 上位基线 | `docs/DEVELOPMENT_BASELINE.md` (`AW-DEV-001`, SHA-256: `b15d51b3048a0d95424b514e59cc6331a620f302905277b5801e78ae86cd7fe1`) |
| 多模型审计输入 | `AUDIT-DSH` (`cdd04bad...`) + 官方 `AUDIT-HY4-CODEBUDDY` (`12066d05...`, 腾讯官方 CodeBuddy `hy4free high`) + `AUDIT-HY4` (`65e71dc2...`) + 主控一手权威文献实测 |
| 核查日期 | 2026-11-11 |
| 状态 | V1.1 架构基线锁定（已合入 Owner Q1–Q8 决议与官方 CodeBuddy Hy4 独立复核修补，对应 `AW-001`） |

---

## 1. 模块划分与单向依赖拓扑

### 1.1 六大业务模块边界

| 模块 | 核心职责 | 公开接口边界 | 唯一写入的数据表（见第 2 章，共 21 张） | 严禁越界行为 |
|---|---|---|---|---|
| **M1 资产与素材库** | 导入、CAS 对象管理、版本 DAG、依赖追踪、用户反馈、一致性备份与恢复 | `AssetLibraryService` | `libraries`, `projects`, `assets`, `asset_versions`, `blobs`, `blob_intents`, `blob_sources`, `gc_tombstones`, `asset_dependencies`, `reference_sets`, `feedback` | 严禁调用任何外部 AI 模型或计费 API |
| **M2 生产编排** | `GenerationSpec` 解析、`Recipe` DAG 调度、预算原子预留、状态机流转、增量冲突 CAS 校验 | `OrchestrationService` | `generation_runs`, `run_steps`, `recipe_versions`, `budget_accounts`, `budget_reservations` | 严禁直接耦合特定 Provider 的 HTTP 报文格式 |
| **M3 确定性资产处理** | 3D 几何/法线、骨骼无环与权重归一化、FK/IK 与程序化动作、2D 图集排版、参数化 VFX、本地音频裁切混音 | `ProcessingPipeline` (纯函数/无状态或本地文件输入输出) | 无数据库表（输入输出由 M2/M1 持久化） | 严禁依赖 PostgreSQL 表结构或网络 SDK |
| **M4 生成连接器** | 外部 LLM / 2D / 3D / 骨骼 / 动作 / 音频 Provider 协议适配、写前意图登记、远端状态轮询与对账 | `ProviderGateway` | `provider_profiles`, `remote_jobs`, `price_catalog_versions` | 严禁自行判定资产最终质量合格或绕过 M2 预算锁 |
| **M5 验证与交付** | 技术检查（glTF Validator、骨骼/权重/预算检查）、`ValidationReport` 生成、Unity/Unreal/Godot 导出包与 `ExportManifest` 组装 | `ValidationService`, `DeliveryService` | `validation_reports`, `export_manifests` | 严禁包含游戏运行时业务逻辑；严禁静态链接 GPL/闭源专有 SDK |
| **M6 工作台与开放入口** | Tauri 桌面 UI、Three.js 视口、CLI、`MCP stdio` 瘦代理、Agent Skills 路由 | IPC Client (`core.sock` / Named Pipe) | 无数据库表（严禁依赖 `persistence` crate） | 严禁直接连接 PostgreSQL、直接操作 CAS 目录或直调付费 Provider |

### 1.2 Rust Workspace 源码分层与编译期依赖规则

```text
crates/
  domain/          # 核心值对象、状态枚举、语义哈希契约（零外部 IO / DB / HTTP 依赖）
  contracts/       # JSON Schema 双向绑定类型（Rust <-> TypeScript 生成源）
  processing/      # M3 确定性资产处理（仅依赖 domain、contracts 与纯计算库）
  persistence/     # sqlx 编译期校验数据访问层（按模块子目录隔离写入权限）
  application/     # M1 / M2 / M5 应用服务编排（依赖 domain、processing、端口 trait）
  adapters/        # M4 Provider 实现、OS Keychain、CAS 文件系统实现、PG Supervisor
  delivery/        # M5 Unity / Unreal / Godot 导出器与引擎工程桥接清单生成
apps/
  core-daemon/     # 本地单一核心守护进程（装配 application + persistence + adapters）
  desktop/         # Tauri 2 + React + Three.js 桌面端（通过 IPC + assetweft:// 与 core-daemon 通信）
  cli/             # 命令行与 MCP stdio 瘦代理（共享 IPC 客户端协议）
```

**门禁守卫（C1 必查）**：
1. `cargo tree` 静态断言：`domain` 与 `processing` 不得出现 `sqlx`、`tokio-postgres`、`reqwest`、`tauri`；`apps/desktop` 与 `apps/cli` 不得直接依赖 `persistence`。
2. 表写入归属检查脚本（`scripts/check_table_ownership`）：扫描所有 SQL 语句，确保每张表的 `INSERT / UPDATE / DELETE` 仅出现在其所属模块的 Repository 文件中，禁止 `SELECT *`。

---

## 2. 数据库核心表归属矩阵（21 张表完整契约）

针对基线第 8.3 节 11 张表无法完整承载 §6.1–6.3 及 AW-015/016/017 承诺的问题，以及官方 CodeBuddy (`hy4free high`) 审计指出的 `gc_tombstones` 归属问题（`HY4-CB-P2-08`），V1 正式确立 **21 张表**的唯一写入属主：

| 序号 | 表名 | 唯一写入模块 | 核心职责与关键不变量 |
|---|---|---|---|
| 1 | `libraries` | **M1** | 本地资产库根目录登记；卸载应用不删除库目录 |
| 2 | `projects` | **M1** | 承载 `ProjectProfile`（风格、单位米、右手坐标系、目标引擎、多边形/贴图预算） |
| 3 | `assets` | **M1** | 逻辑资产主表；含 `head_version_id`（M2 增量提交必须通过调用 M1 `AssetLibraryService::commit_version` 更新，严禁跨模块直写） |
| 4 | `asset_versions` | **M1** | 不可变版本记录；含 `version_no`、`parent_version_id`、`component_manifest`（组件级语义哈希 JSONB，承担逻辑同一性判据） |
| 5 | `blobs` | **M1** | 内容寻址物理文件表（主键 `content_hash` SHA-256，承担物理存储去重）；行存在则 `objects/` 文件必存在 |
| 6 | `blob_intents` | **M1** | CAS 写前意图与心跳表（`TMP_WRITTEN -> HASHED -> RENAMED -> COMMITTED` 逐步落库），支撑崩溃幂等重放与 GC 互斥 |
| 7 | `blob_sources` | **M1** | 记录同一 `content_hash` 下不同来源/语音包/模型权重的 `origin_kind`、`provider_id`、`license_ref`、`terms_version`（物理去重不合并许可） |
| 8 | `gc_tombstones` | **M1** | CAS 三阶段垃圾回收墓碑表（记录 `content_hash`、`marked_at`、`grace_expires_at`），写入端命中时同事务撤销 |
| 9 | `asset_dependencies` | **M1** | 资产版本间的引用有向无环图（Mesh -> Material -> Texture 等），阻止被引用对象被误删 |
| 10 | `reference_sets` | **M1** | 承载 `ReferenceSet`（参考图/参考音频版本、用途、优先级、允许变化范围） |
| 11 | `feedback` | **M1** | 用户评价（`UNRATED / ACCEPTED / NEEDS_CHANGES / REJECTED`），强制绑定具体 `asset_version_id` |
| 12 | `recipe_versions` | **M2** | 不可变生产配方定义（步骤、参数 Schema、执行器要求、校验规则） |
| 13 | `generation_runs` | **M2** | 生产运行实例；绑定 `GenerationSpec`、`base_version_id`、`budget_account_id` 与总状态 |
| 14 | `run_steps` | **M2** | DAG 节点执行状态（6 公开状态 + 内部子状态 + 独立 `billing_state` 轴）；单写者租约保护 |
| 15 | `budget_accounts` | **M2** | 预算账户（全局/项目/Provider 级）：`limit_minor`, `spent_minor`, `reserved_minor`, `overdraft_minor` |
| 16 | `budget_reservations` | **M2** | 每次远端调用前的原子预扣费记录（`HELD / SETTLED / RELEASED / DEBT`） |
| 17 | `provider_profiles` | **M4** | 外部服务配置、能力声明（含是否支持幂等键/按客户端 ID 查任务）及 OS 凭据库引用（严禁存明文密钥） |
| 18 | `price_catalog_versions` | **M4** | 版本化计费单价快照；预留时钉住 `price_version`，保障结算可复现 |
| 19 | `remote_jobs` | **M4** | 远端异步任务写前意图表（`INTENT -> SUBMITTED -> POLLING -> DONE / LOST`），防崩溃后重复提交计费 |
| 20 | `validation_reports` | **M5** | 承载 `ValidationReport`（检查项、工具版本、`NOT_RUN / PASS / FAIL / WAIVED`、证据指针） |
| 21 | `export_manifests` | **M5** | 承载 `ExportManifest v2`（文件列表、语义哈希、引擎版本、事件轨道 `event_tracks`、挂点 `attach_points`、根运动 `root_motion`、骨骼映射 `skeleton_mapping`、材质通道映射 `material_channel_map`、能力限制、Artifact 级 `license_chain`） |

---

## 3. 五大关键工程机制设计

### 3.1 桌面端嵌入式存储架构：SQLite 3 (WAL 模式) 默认 + 可选外接 PostgreSQL 18（彻底消除进程托管脆弱性）

针对客户端打包原生 PostgreSQL 18 存在的四大现实痛点（安装包膨胀 150MB+、Windows 杀毒软件对后台 `postgres.exe` 子进程误报拦截、固定 5432 端口冲突、断电残留 `postmaster.pid` 需复杂状态机自愈），V1.2 架构正式将**单机默认存储彻底切换为工业界绝对黄金标准的嵌入式 SQLite 3**，同时保留外部 PostgreSQL 18 作为局域网大团队协作的逃生通道：

1. **单机默认：纯 Rust 静态嵌入式 SQLite 3（WAL 模式）**：
   - **零子进程、零网络端口、零杀毒拦截**：通过 Rust `rusqlite` / `sqlx(sqlite)` 直接以静态库形式编译进客户端单一二进制文件。客户端不再拉起任何外部数据库进程，彻底消除操作系统杀毒软件的进程与网络拦截。
   - **单文件工程库（Local-First）**：每个项目或资产库表现为一个独立的本地数据库文件（如 `project.assetweft` 或 `assetweft.db`），如同移动 Word 或 Photoshop 文档一样，备份、重命名、拷贝极为直观。
   - **WAL 模式（Write-Ahead Logging）高并发**：启用 `PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;`，实现单机“一写多读”。桌面 UI、CLI 与 `MCP stdio` 多个读客户端完全并发互不阻塞，事务提交达到微秒级。
   - **完整支持 21 张表与 JSONB 语义索引**：SQLite 内置的 `JSON1` 扩展与 JSON 提取函数完美承载 `component_manifest` 与 `ReferenceSet` 的动态结构，性能与表现力完全满足单机游戏资产管理需求。
2. **团队协作逃生通道（可选外接 PostgreSQL 18）**：
   - 针对 5 人以上游戏工作室的局域网集中存储与团队协同需求，`persistence` 层通过 Repository 抽象保留外部 PostgreSQL 驱动。
   - 用户可在设置中配置外部 `DATABASE_URL`（连接局域网自建专用服务器或 Docker 内的 PostgreSQL 18）。**单机个人模式默认 100% 走 SQLite 3，零配置开箱即用；团队模式按需连接中心 PG**。

### 3.2 单一核心进程（`core-daemon`）多入口租约与 IPC 握手（AW-008 / AW-033）

1. **单实例互斥与寻址**：
   - 核心进程通过 `runtime_dir/core.lock`（`flock(LOCK_EX|LOCK_NB)` / Windows 命名互斥）保证全局唯一。
   - 桌面 UI、CLI、`MCP stdio` 均作为 IPC 客户端连接核心进程；**`MCP stdio` 是纯转发瘦代理（Proxy）**，且核心库启用 `#![deny(clippy::print_stdout)]`，确保 `stdout` 100% 留给 MCP JSON-RPC 帧。
2. **会话租约（Lease）与优雅退出谓词**：
   - 每个入口连接在握手中申请 `Lease`（默认 TTL 30s，每 10s 心跳续期）。
   - 核心进程在无连接、无运行中生成任务且空闲超 300s 后自动关闭退出；单机 SQLite 模式下无任何后台常驻守护服务负担。
3. **版本握手与平滑升级（Drain 防死锁协议）**：
   - 握手消息 `Hello / Welcome` 采用独立于业务消息的轻量版本协商帧，校验 `(proto_min, proto_max)`、`core_build` 与 `db.schema_version`。
   - 客户端升级后发现旧核心版本不匹配时，在不申请业务 `Lease` 的前提下直接发送 `DrainRequest`，旧核心等待在飞任务归零后退出并完成 SQLite 检查点，由新客户端拉起新版本。

### 3.3 CAS 内容寻址存储与三阶段无竞态 GC（AW-010 / AW-012）

1. **写前意图四态逐步落库与原子落盘（修补 `HY4-CB-P1-09`）**：
   - 锁键规范（修补 `HY4-CB-P2-08`）：弃用跨 PG 版本不稳定的 32-bit `hashtext()`，统一取 `content_hash`（SHA-256）前 8 字节大端序转换为有符号 64-bit 整数 `lock_key_i64` 调用 `pg_advisory_xact_lock(lock_key_i64)`。
   - 写入状态机严格分步提交，确保任意时刻崩溃均可幂等恢复：
     1. `INSERT blob_intents(state='TMP_WRITTEN', tmp_path)` 提交；
     2. 流式写 `work/tmp/<uuid>.part` 并同步计算 SHA-256，`fsync(file)` 后提交 `UPDATE blob_intents SET state='HASHED', content_hash=:hash`；
     3. 获取 `pg_advisory_xact_lock(lock_key_i64)`，将 `.part` 原子 `rename` 至 `objects/aa/bb/<sha256>` 并 `fsync(dir)`，在同一 PG 事务内写入 `blobs` + `blob_sources` + `DELETE FROM gc_tombstones WHERE content_hash=:hash` + `UPDATE blob_intents SET state='COMMITTED'`。
2. **三阶段垃圾回收（Mark -> Grace -> In-Lock Rename/Sweep）**：
   - **Phase A（标记）**：扫描 `objects/` 中无 `blobs` 引用、无活跃 `blob_intents` 且 `mtime` 早于窗口 `W`（默认 7 天）的对象，记入 `gc_tombstones` 墓碑表（归属 M1），不删文件。
   - **Phase B（宽限期 G = 24h）**：宽限期内若任何新写入命中该哈希，直接在写入事务内删除 `gc_tombstones` 记录并复用。
   - **Phase C（锁内隔离与删除，修补 `HY4-CB-P1-09`）**：在 PG 事务内获取相同的 `pg_advisory_xact_lock(lock_key_i64)`，二次确认 `blobs` 与活跃 `blob_intents` 均无引用且墓碑未撤销后，**在持有数据库事务锁期间**先将 `objects/aa/bb/<sha256>` 原子 `rename` 到 `work/trash/<sha256>.<txid>`，随后提交事务并执行物理 `unlink`。彻底消除“事务提交释放锁后、物理 `unlink` 前并发写入同哈希文件被误删”的竞态窗口。

### 3.4 预算控制（Budget）与 API 费用本质、跨平台浮点确定性契约（AW-014 / AW-015 / AW-016 / AW-022 / AW-023）

1. **什么是 API？为什么会产生费用？什么时候费用为 0？**：
   - **API 的定义与用途**：API（Application Programming Interface，应用程序接口）是 AssetWeft 软件向外界机房发送制作请求的“电话热线”。
     - **纯本地模式（完全免费，费用 = 0）**：当用户电脑配置足够高时，运行本地开源模型（如本地安装的 TRELLIS 跑 3D、UniRig 绑骨骼、本地算法裁切音频与处理切线），**全部在用户本地显卡运行，不联网、不调外部接口，一分钱都不用花，预算占用恒等于 0**。
     - **云端商业生成模式（按次计费，产生 API 费用）**：当用户电脑是普通办公本（跑不动 24GB 显存的大模型）、或者追求行业顶尖的商业 AI 效果（如 Midjourney / Flux 出 2D 原画、Tripo3D / Meshy 生成高精度 3D 网格、ElevenLabs 生成好莱坞级音效、OpenAI / Claude 生成复杂策划设计规格）时，模型运行在对应云端厂商的超算中心。AssetWeft 通过网络将参数发给对方（调用 API），对方机房算好后把 3D 或音频文件传回。**因为对方提供了昂贵的云端 GPU 算力，对方会按次收取费用（例如生成一个 3D 模型收 0.2 美元，一段音效收 0.05 美元），这笔由网络调用外部服务产生的花销即为 API 费用**。
   - **预算控制（Budget）的本质作用**：
     - 游戏资产通常是自动化、DAG 批量化生产（例如一键为全关卡 20 种怪物批量生成 5 套动作与音效）。
     - 若程序发生逻辑死循环、或用户不小心误点超大批量倍数，后台可能会在几分钟内狂发几千次云端网络请求，导致用户的 API 账户或信用卡被瞬间刷爆。
     - **预算控制就是“防刷爆电气保险丝”**：用户在软件中设定直观的「单次任务最高限额」与「项目总限额」。一旦达到红线，软件底层原子 SQL 立即切断网络请求并弹窗警告，绝不让用户承受意料之外的扣费风险。
   - **底层直观计价与整数防漏**：各平台原币种原单位独立建账（美元通道 `USD $`、人民币通道 `CNY ¥`、积分平台 `Credits`），底层使用 `minor_units` 整数微单位记账杜绝浮点计算误差。付费前执行单条原子更新 SQL：
     `UPDATE budget_accounts SET reserved_minor = reserved_minor + :cost WHERE id = :acct AND reserved_minor + spent_minor + overdraft_minor + :cost <= limit_minor`，行数为 0 立即阻断。
2. **防重复付费写前意图（Write-Ahead Intent）**：
   - 发起外部 HTTP POST 前，**必须先在数据库提交 `remote_jobs(state='INTENT', idempotency_key, request_hash)`**。
   - 若在收到响应前进程崩溃，重启后该步骤状态为 `REMOTE_UNKNOWN`（其预算预留 `HELD` 保持冻结、不释放为 0），优先通过 Provider 的 `idempotency_key` 或任务列表接口对账，严禁盲目重发第二次付费请求。
3. **跨平台浮点确定性与组件级语义哈希契约（`DeterminismPolicy`，修补 `HY4-CB-P0-01`）**：
   - **物理去重 vs 逻辑同一性分离**：`blobs.content_hash`（文件级 SHA-256）负责 CAS 物理去重；`asset_versions.component_manifest`（组件级语义哈希）负责增量契约（AW-016）与跨引擎逻辑同一性验证。
   - **跨平台规范化字节边界（`canonicalize`）**：为消除 `macOS ARM64`（NEON FMA / Apple libm）与 `Windows x86_64`（AVX / UCRT）之间的末位 ULP 漂移，所有进入 `component_manifest` 的浮点属性必须先通过 `domain::canonicalize` 量化为规范化小端字节流再计算 SHA-256：
     - 顶点坐标/位移、法线/切线、UV、蒙皮权重：统一执行 `-0.0 -> +0.0`、拒绝 `NaN/Inf`、按 `1e-6`（定点量化步长 `round_ties_even(x * 1_000_000.0) as i64`）规约；
     - 四元数旋转：归一化后强制半球符号规范（第一个非零分量或 `w >= 0`，若 `w < 0` 则取 `-q`）再定点量化；
     - 哈希计算路径统一禁用 FMA 融合（不调硬件 `mul_add`）并使用纯 Rust `libm`。
   - **跨模块提交边界**：M2 完成增量编排后，通过调用 M1 `AssetLibraryService::commit_version` 执行乐观锁提交（`UPDATE assets SET head_version_id = :new WHERE id = :id AND head_version_id = :base_version`），严禁 M2 直接写 `assets` 表。
4. **刚体挂点（Socket）与骨骼拓扑变更的校准对比测试门禁（A/B Calibration Gate）**：
   - 针对增量修改中涉及 `Sockets`（武器挂点/特效锚点，基线 `AW-025`）微调或 `Rig`（骨骼拓扑重绑定 + 基线 `AW-023` 动作重定向）的场景，强制设立自动化**前后同帧姿态校准对比测试（Pose & Socket Drift Calibration）**：
     1. 锁定组件（`Mesh / Material`）规范化语义哈希必须 **100% 恒等（0 差异）**；
     2. 对挂点与重定向动画采样关键帧（0% / 25% / 50% / 75% / 100%），对比修改前后世界空间变换矩阵：**挂点位置偏移（Position Drift）$\le 0.5\text{ mm}$、旋转误差（Angular Error）$\le 0.1^\circ$、脚底接触点滑动（Foot Skate）不劣化**，并输出量化校准对比报告写入 `validation_reports`，未达标直接拦截提交。

### 3.5 视口二进制协议、Unreal 极速交付（OpenUSD + glTF + Remote Control）与工作台 Figma 设计门禁（AW-005 / AW-025 / AW-029~031 / AW-034）

1. **Tauri 2 + Three.js 二进制流传输与显存闭环（修补 `HY4-CB-P1-02 / P1-03 / P1-04`）**：
   - 严禁通过 Tauri JSON `invoke` 传输 3D 网格、纹理或音频字节。
   - 核心通过 `register_asynchronous_uri_scheme_protocol("assetweft", ...)` 暴露 `assetweft://blob/<sha256>`（单次 IPC 内存拷贝，显式支持 `GET / HEAD / OPTIONS` CORS 预检头与 `206 Partial Content` Range 响应，并在 `tauri.conf.json` CSP 放行 `assetweft:`）。
   - 全局复用单一 `WebGLRenderer` 上下文；切换资产时调用统一 `disposeScene(root)` 递归释放所有 Geometry、Material、嵌套贴图槽（`map/normalMap/roughnessMap/metalnessMap/aoMap/emissiveMap/envMap`）、`ImageBitmap.close()` 及 `KTX2/Draco` Worker 池，并结合 LRU 缓存限制多版本同屏对比时的 `WebContent` 进程内存峰值。
2. **三引擎坐标/手性/PBR 转换数学契约（`EngineTransformContract` & `ExportManifest v2`，修补 `HY4-CB-P0-02 / P0-03 / P0-04`）**：
   - **坐标系、手性与蒙皮矩阵转换契约**：
     - 源基准（glTF 2.0）：右手系（RH）、`+Y Up`、单位 `1.0 = 1 米`。
     - 目标引擎换基矩阵 $C$：Unity（左手系 LH、`+Y Up`、米，$\det(C) < 0$）；Unreal（左手系 LH、`+Z Up`、厘米 $\times 100$，$\det(C) < 0$）；Godot（右手系 RH、`+Y Up`、米）。
     - 凡涉及手性翻转（$\det(C) < 0$）的转换，必须同步执行：① 顶点与法线乘换基矩阵；② **反转三角形顶点索引绕序 `(i0, i2, i1)`**；③ **翻转 MikkTSpace 切线手性分量 `tangent.w = -tangent.w`**（切线由 AssetWeft M3 唯一权威生成，禁止引擎重算导致法线贴图裂痕）；④ 蒙皮逆绑定矩阵按相似变换重算 $\text{IBM}' = C \cdot \text{IBM} \cdot C^{-1}$；⑤ Unreal $\times 100$ 厘米缩放统一施加于节点平移、IBM 平移列、动画位移轨道、Morph 位移增量与挂点偏移。
   - **PBR 通道逐引擎打包与色彩空间契约**：
     - 导出时按目标引擎生成对应贴图通道布局（Unreal/Godot 采用 `ORM: R=AO, G=Roughness, B=Metallic`；Unity URP/HDRP 自动重打包为 `MetallicSmoothness: R=Metallic, A=1.0-Roughness` + 独立 AO），并在 `ExportManifest v2` 中强制声明每张贴图的 `color_space`（法线/ORM/MetallicSmoothness 强制 `sRGB = false` 线性空间，BaseColor/Emissive 为 `sRGB = true`）。
   - **`ExportManifest v2` 完整结构化字段**：
     - 除文件列表、语义哈希、引擎版本与 Artifact 级 `license_chain` 外，强制包含：`event_tracks`（动作帧事件）、`attach_points`（Socket 局部变换与父骨骼 ID）、`root_motion`（提取模式与单位）、`skeleton_mapping`（源骨骼 ID $\leftrightarrow$ Unity Humanoid / UE Mannequin 映射）、`material_channel_map`、`morph_target_curves`（供 UE 插件直接还原表情曲线）。
3. **Unreal 业界最顶尖资产交付设计：OpenUSD + Interchange glTF + Remote Control 实时一键直达**：
   - **格式层突破（解决 glTF 2.0 规格限制每顶点仅 4 骨骼影响的痛点）**：
     - **主推 OpenUSD (`.usdc` / `.usda`)**：Pixar 开源、Linux 基金会主导（Apache-2.0 许可证，100% 完全开源合法无专有 SDK 风险）。Unreal Engine 5.4/5.5/5.6 拥有官方 **Interchange OpenUSD** 核心支持。**USD 原生突破 glTF 4 骨骼影响数限制（支持 8+ 骨骼精细蒙皮与骨骼动画）**，材质完美映射 `UsdPreviewSurface` 与 `MaterialX`，无需任何复杂格式转换！
     - **成熟兼顾 glTF 2.0**：对中低多边形资产输出自包含 `.glb` + `ExportManifest v2`，UE 插件在内存中自动补齐表情动画。
     - **外包兜底纯 Rust FBX 写出器**：针对甲方强制交 `.fbx` 的外包场景，提供无外部依赖的纯 Rust 静态网格与基础骨骼 FBX 二进制写出器，彻底杜绝 Autodesk 专有闭源 SDK 污染。
   - **交互层突破（类似 Quixel Bridge / Blender to Unreal 的 Remote Control 体验）**：
     - 用户不再需要手动打开文件夹、把文件拖进 Unreal 窗口、再面对几十个繁琐导入选项！
     - AssetWeft 桌面端通过探测 Unreal 官方 **Remote Control API**（本地 HTTP `30010` / WebSocket `30020`）或 **Python Remote Execution**（UDP 广播）：
     - 当检测到电脑正在运行 Unreal Editor 时，视口右上角点亮「Unreal 已连接 🟢」，点击 **「一键直推到 Unreal」**，Unreal 后台自动无感完成资产导入、生成材质实例并自动保存在 `Content/AssetWeft/<Project>/` 目录，甚至自动把模型放置在当前编辑器关卡视口正中央！
4. **M6 桌面工作台视觉设计门禁（Mobbin $\to$ Eagle $\to$ Figma $\to$ Owner 审核）**：
   - 在编写任何桌面工作台前端页面代码（基线 `AW-034` / `AW-035`）前，强制执行三步视觉门禁（遵循 `[细则·视觉]`）：
     1. **Mobbin 采集并入库 Eagle**：按工作台核心视图（资产网格库、3D/2D/音频主视口、DAG 节点编排器、A/B 增量对比视图、三引擎导出面板、预算设置）在 Mobbin 采集高水准参考图，全部存入本地 Eagle 素材库并 `GET /api/item/list` 读回校验，生成 `references/MOBBIN-EAGLE-MANIFEST.md`；
     2. **Figma 模块化设计**：在 Figma 中构建 Variables 令牌、基础组件与 Auto Layout 完整工作台页面；
     3. **Owner 审核门禁**：导出 Figma 页面完整预览提交 Owner 审核，**Owner 审核通过后方可启动前端实现**。

---

## 4. 任务卡 DAG 依赖修正与 CI 汇总门禁（第 10 / 12 章）

### 4.1 任务卡依赖图（DAG）四项修正（严格对齐基线 `AW-001..040` 编号，修补 `HY4-CB-P1-10 / P2-01`）

1. **AW-016（增量修改契约与组件锁定提交）**：依赖从 `010, 014, 015, 017` 增补为 `010, 014, 015, 017, 022`（需要 M3 的 `AW-022` 提供基于 `DeterminismPolicy` 的 Mesh/Skeleton/Skin 组件级规范化语义哈希计算器）。
2. **AW-033（CLI / MCP stdio / 公开接口契约）**：将 `AW-016` 从前置硬阻塞改为两阶段交付——`AW-033a`（依赖 `008, 013, 014, 017`）在 W2 提前打通 CLI/MCP 的资产查询、任务提交与验证接口，支撑自动化测试；`AW-033b` 在 `AW-016` 完成后合入增量提交命令。
3. **AW-022（Mesh/Skin/Skeleton 基础编辑）与 AW-005（视口原型）解耦（修补 `HY4-CB-P1-10`）**：`AW-022` 属于纯计算层 `crates/processing`，仅依赖 `003, 017`（即 `contracts` 中的数据与语义哈希契约），**绝不逆向依赖 UI 层的 `AW-005`**，严格保持 `domain -> contracts -> processing -> application -> apps` 单向依赖拓扑。
4. **AW-029（Unity）/ AW-030（Unreal）/ AW-031（Godot）三引擎交付**：基础骨架/动画/Sprite/音频交付依赖 `017, 022, 023, 024`；面部 Morph 与挂点交付子项显式依赖基线 `AW-025`（面部轨道、挂点、服装配件、蒙皮迁移）。

### 4.2 GitHub Actions 防 Pending 死锁汇总门禁

- **禁止**在 Required Check 对应的 Workflow 顶层使用 `paths:` 过滤。
- 统一由 `.github/workflows/ci-gate.yml` 响应 `pull_request` 与 `merge_group`：
  1. `changes` Job 计算变更路径掩码（`rust`, `web`, `native`, `contracts`；未知新路径默认全跑 fail-safe）；
  2. `c1-fast`（必跑：格式、编译、`sqlx prepare --check`、21 表归属 lint、`cargo tree` 单向分层断言、`cargo-deny` 许可证 allowlist 阻断 GPL/AGPL/闭源 SDK 混入、契约测试）；
  3. `c2-integration`（按需 Job 级 `if:`：Linux PG18 容器迁移、CAS 6 边界崩溃注入 `G-CAS-1`、预算并发锁测试）；
  4. `c3-desktop-native`（按需 Job 级 `if:`：Windows x86_64 与 macOS ARM64 跨平台浮点确定性断言 `G-DET-1`、托管 PG18 启停恢复、Tauri `assetweft://` 视口冒烟 `G-VP-1..3`、无隐藏 Python/Node 运行时安装包审计 `G-R03-1`）；
  5. `gate-verdict`（`if: always()` 汇总 Job）：检查所有应跑 Job 均为 `success`、未触发 Job 为合法 `skipped`，一旦存在 `failure` 或 `cancelled` 立即 `exit 1`。分支保护规则仅将 `gate-verdict` 设为唯一必过状态检查。
