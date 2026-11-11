# AssetWeft（游织）v1.0 系统架构与可执行工程契约

| 文档属性 | 内容 |
|---|---|
| 文档编号 | `AW-DOC-ARCH-001` |
| 唯一维护位置 | `docs/ARCHITECTURE.md` |
| 上位基线 | `docs/DEVELOPMENT_BASELINE.md` (`AW-DEV-001`, SHA-256: `b15d51b3048a0d95424b514e59cc6331a620f302905277b5801e78ae86cd7fe1`) |
| 多模型审计输入 | `AUDIT-DSH` (`cdd04bad...`) + `AUDIT-HY4` (`65e71dc2...`) + 主控一手权威文献实测 |
| 核查日期 | 2026-11-11 |
| 状态 | V1 架构基线锁定（对应任务卡 `AW-001`） |

---

## 1. 模块划分與单向依赖拓扑

### 1.1 六大业务模块边界

| 模块 | 核心职责 | 公开接口边界 | 唯一写入的数据表（见第 2 章） | 严禁越界行为 |
|---|---|---|---|---|
| **M1 资产与素材库** | 导入、CAS 对象管理、版本 DAG、依赖追踪、用户反馈、一致性备份与恢复 | `AssetLibraryService` | `libraries`, `projects`, `assets`, `asset_versions`, `blobs`, `blob_intents`, `blob_sources`, `asset_dependencies`, `reference_sets`, `feedback` | 严禁调用任何外部 AI 模型或计费 API |
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

## 2. 数据库核心表归属矩阵（20 张表完整契约）

针对基线第 8.3 节 11 张表无法完整承载 §6.1–6.3 及 AW-015/016/017 承诺的问题，V1 正式确立 **20 张表**的唯一写入属主：

| 序号 | 表名 | 唯一写入模块 | 核心职责与关键不变量 |
|---|---|---|---|
| 1 | `libraries` | **M1** | 本地资产库根目录登记；卸载应用不删除库目录 |
| 2 | `projects` | **M1** | 承载 `ProjectProfile`（风格、单位米、右手坐标系、目标引擎、多边形/贴图预算） |
| 3 | `assets` | **M1** | 逻辑资产主表；含 `head_version_id`（用于增量提交 CAS 乐观锁） |
| 4 | `asset_versions` | **M1** | 不可变版本记录；含 `version_no`、`parent_version_id`、`component_manifest`（组件级语义哈希 JSONB） |
| 5 | `blobs` | **M1** | 内容寻址物理文件表（主键 `content_hash` SHA-256）；行存在则 `objects/` 文件必存在 |
| 6 | `blob_intents` | **M1** | CAS 写前意图与心跳表（`TMP_WRITTEN -> HASHED -> RENAMED -> COMMITTED`），支撑崩溃恢复与 GC 互斥 |
| 7 | `blob_sources` | **M1** | 记录同一 `content_hash` 下不同来源的 `origin_kind`、`provider_id`、`license_ref`、`terms_version`（去重不合并许可） |
| 8 | `asset_dependencies` | **M1** | 资产版本间的引用有向无环图（Mesh -> Material -> Texture 等），阻止被引用对象被误删 |
| 9 | `reference_sets` | **M1** | 承载 `ReferenceSet`（参考图/参考音频版本、用途、优先级、允许变化范围） |
| 10 | `feedback` | **M1** | 用户评价（`UNRATED / ACCEPTED / NEEDS_CHANGES / REJECTED`），强制绑定具体 `asset_version_id` |
| 11 | `recipe_versions` | **M2** | 不可变生产配方定义（步骤、参数 Schema、执行器要求、校验规则） |
| 12 | `generation_runs` | **M2** | 生产运行实例；绑定 `GenerationSpec`、`base_version_id`、`budget_account_id` 与总状态 |
| 13 | `run_steps` | **M2** | DAG 节点执行状态（6 公开状态 + 内部子状态 + 独立 `billing_state` 轴）；单写者租约保护 |
| 14 | `budget_accounts` | **M2** | 预算账户（全局/项目/Provider 级）：`limit_minor`, `spent_minor`, `reserved_minor`, `overdraft_minor` |
| 15 | `budget_reservations` | **M2** | 每次远端调用前的原子预扣费记录（`HELD / SETTLED / RELEASED / DEBT`） |
| 16 | `provider_profiles` | **M4** | 外部服务配置、能力声明（含是否支持幂等键/按客户端 ID 查任务）及 OS 凭据库引用（严禁存明文密钥） |
| 17 | `price_catalog_versions` | **M4** | 版本化计费单价快照；预留时钉住 `price_version`，保障结算可复现 |
| 18 | `remote_jobs` | **M4** | 远端异步任务写前意图表（`INTENT -> SUBMITTED -> POLLING -> DONE / LOST`），防崩溃后重复提交计费 |
| 19 | `validation_reports` | **M5** | 承载 `ValidationReport`（检查项、工具版本、`NOT_RUN / PASS / FAIL / WAIVED`、证据指针） |
| 20 | `export_manifests` | **M5** | 承载 `ExportManifest`（导出文件列表、语义哈希、引擎版本、能力限制、许可证汇总声明） |

---

## 3. 五大关键工程机制设计

### 3.1 托管 PostgreSQL 18 生命周期、自动自愈与外接逃生阀（AW-004 / AW-008 / AW-012）

1. **初始化与编码隔离**：
   - 统一使用 `initdb -D <pgdata> --encoding=UTF8 --locale=C --no-locale --data-checksums --auth-local=scram-sha-256 --auth-host=scram-sha-256`。
   - 强制 `--locale=C --no-locale` 彻底消除 Windows/macOS GUI 进程缺失环境变量或中文用户名导致的 locale 初始化失败与跨版本索引排序漂移。
   - 启动前检测数据目录所在卷类型：**拒绝 SMB/NFS 网络盘及 OneDrive/iCloud/Dropbox 实时同步目录**（返回稳定错误码 `AW-FS-001`）；若用户主目录包含非 ASCII 字符导致 Windows 子进程路径兼容问题，自动映射至本地安全短路径（如 `%PROGRAMDATA%\AssetWeft\pgdata`）。
2. **通信寻址与认证（零固定端口冲突）**：
   - **macOS**：禁用 TCP（`listen_addresses = ''`），仅监听权限为 `0700` 的本地 Unix Domain Socket（若用户目录路径超过 `sun_path` 104 字节限制，自动映射至 `/tmp/.assetweft-<uid>/pg`）。
   - **Windows**：监听 `127.0.0.1` 动态空闲端口（`49152–65535` 探测后启动，并从 `postmaster.pid` 第 4 行回读确认实际端口写入 `runtime.json`），绝不抢占默认的 `5432` 端口。
   - **认证**：初始化时生成 32 字节随机高熵口令存入 OS 凭据库（macOS Keychain / Windows Credential Manager），严禁使用 `trust`。
3. **异常退出与 `postmaster.pid` 自动自愈状态机**：
   - 解析 `postmaster.pid`：比对数据目录 `realpath`、检查 PID 是否存活且进程启动时间/命令行匹配 `postgres`。
   - 若为断电/强退残留（`STALE_PIDFILE`），先备份原 `postmaster.pid` 到 `logs/` 再自动清理并拉起，无需用户手动干预；若为 Windows Defender / 杀毒软件瞬时锁文件（`ERROR_SHARING_VIOLATION`），执行 5 次指数退避（200ms ~ 3.2s），仍失败则报 `AW-DB-011` 引导用户排除目录。
   - **严禁自动执行 `initdb` 覆盖非空目录，严禁自动执行 `pg_resetwal`**。
4. **外接 PostgreSQL 18 逃生阀（External PG Escape Hatch）**：
   - 针对极端受限的企业 Windows 终端（组策略禁止桌面应用拉起 `postgres.exe` 子进程），`persistence` 层支持在设置中配置外部 `DATABASE_URL`（连接本机已有 PostgreSQL、Docker 容器或局域网 PostgreSQL 18 实例）。由于底层同为 PostgreSQL 18，**100% 复用同一套 SQL Schema 与 `sqlx` 查询，零双数据库维护成本**。
5. **`pg_upgrade` 双目录与熔断判据（Kill Criteria）**：
   - 升级仅使用 `--copy` 模式（禁用会就地破坏旧集群的 `--link`），通过 `CURRENT` 指针文件原子切换 `pg/18` 与 `pg/19`。
   - 命中任一 Kill Criteria（`--check` 失败、磁盘可用空间 < `2.2 × PGDATA`、20 张表升级前后 `count(*)` 不一致、主键索引 `amcheck` 失败）立即将 `CURRENT` 指回旧集群，保证零数据丢失。

### 3.2 单一核心进程（`core-daemon`）多入口租约与 IPC 握手（AW-008 / AW-033）

1. **单实例互斥与寻址**：
   - 核心进程通过 `runtime_dir/core.lock`（`flock(LOCK_EX|LOCK_NB)` / Windows 命名互斥）保证全局唯一。
   - 桌面 UI、CLI、`MCP stdio` 均作为 IPC 客户端连接核心进程；**`MCP stdio` 是纯转发瘦代理（Proxy）**，且核心库启用 `#![deny(clippy::print_stdout)]`，确保 `stdout` 100% 留给 MCP JSON-RPC 帧。
2. **会话租约（Lease）与优雅退出谓词**：
   - 每个入口连接在握手中申请 `Lease`（默认 TTL 30s，每 10s 心跳续期）。
   - 核心进程仅在以下条件**同时满足**时才优雅停止 PG 并退出：
     `active_leases == 0 && active_runs == 0 (含 REMOTE_UNKNOWN 待对账任务) && maintenance_lock == none && idle_elapsed > idle_timeout (默认 300s)`。
   - 关闭桌面 UI 窗口仅释放 `Desktop` 租约，绝不中断正在后台执行的 CLI 或 MCP 生成任务。
3. **版本握手与平滑升级（Drain）**：
   - 握手消息 `Hello / Welcome` 强制校验 `(proto_min, proto_max)`、`core_build` 与 `db.schema_version`。
   - 客户端升级后发现旧核心版本不匹配时，发送 `DrainRequest`：旧核心拒绝新任务、等待在飞任务归零后退出，由新客户端拉起新核心。

### 3.3 CAS 内容寻址存储与三阶段无锁竞态 GC（AW-010 / AW-012）

1. **写前意图与原子落盘**：
   - 写入顺序固定为：`登记 blob_intents(TMP_WRITTEN)` -> `流式写 work/tmp/<uuid>.part 并同步计算 SHA-256` -> `fsync(file)` -> `rename 至 objects/aa/bb/<sha256>` -> `fsync(dir)` -> `在 PG 事务内获取 pg_advisory_xact_lock(hashtext(hash)) 并写入 blobs + blob_sources + asset_versions + 更新 intent 为 COMMITTED`。
2. **三阶段垃圾回收（Mark -> Grace -> Sweep）**：
   - **Phase A（标记）**：扫描 `objects/` 中无 `blobs` 引用、无活跃 `blob_intents` 且 `mtime` 早于窗口 `W`（默认 7 天）的对象，记入 `gc_tombstones` 墓碑表，不删文件。
   - **Phase B（宽限期 G = 24h）**：宽限期内若任何新写入命中该哈希，直接撤销墓碑并复用。
   - **Phase C（删除）**：在事务内获取与写入端相同的 `pg_advisory_xact_lock(hashtext(hash))`，二次确认 `blobs` 无引用且墓碑未撤销后，提交事务并 `unlink` 物理文件。彻底消除“GC 删掉正在提交的文件”的竞态。

### 3.4 预算原子预留、写前意图、组件语义哈希与姿态校准门禁（AW-014 / AW-015 / AW-016 / AW-019）

1. **按通道原币种/原积分直显与防超支原子预留 SQL**：
   - **直观计价模型（不搞自动跨币种汇率换算）**：各外部 AI 平台按其原生计价单位独立建账（美元通道显示 `USD $`、人民币通道显示 `CNY ¥`、积分制平台如 Tripo/Meshy 显示 `Credits 积分`、本地开源模型显示 `Free 0 成本`）。用户在界面中只需设定直观的**「单次任务最高限额」**与**「项目/日总限额」**。
   - **底层整数防精度丢失**：数据库内部统一采用整数微单位（`minor_units`，如 `1 元 = 1,000,000 微元` 或 `1 积分`）存储，彻底杜绝浮点数加减误差。
   - **防超支原子预留**：严禁“先 SELECT 余额再 INSERT 任务”。所有付费步骤调用前，必须执行单条带条件更新：
     `UPDATE budget_accounts SET reserved_minor = reserved_minor + :cost WHERE id = :acct AND reserved_minor + spent_minor + overdraft_minor + :cost <= limit_minor`。影响行数为 0 则立即拒绝并返回 `BUDGET_EXCEEDED`。
2. **防重复付费写前意图（Write-Ahead Intent）**：
   - 发起外部 HTTP POST 前，**必须先在数据库提交 `remote_jobs(state='INTENT', idempotency_key, request_hash)`**。
   - 若在收到响应前进程崩溃，重启后该步骤状态为 `REMOTE_UNKNOWN`（其预算预留 `HELD` 保持冻结、不释放为 0），优先通过 Provider 的 `idempotency_key` 或任务列表接口对账，严禁盲目重发第二次付费请求。
3. **组件级语义哈希与增量 CAS 提交**：
   - 区分**物理文件哈希（`blob.content_hash`）**与**逻辑组件语义哈希（`component_manifest`）**：GLB 重新打包会改变文件字节哈希，但只要 `component_manifest.mesh`（规范化顶点/索引/法线/UV 缓冲哈希）、`skeleton`、`materials` 未变，即证明满足 AW-016“只改动作时 Mesh/材质/Rig 哈希不变”的验收判据。
   - 提交新版本采用乐观锁：`UPDATE assets SET head_version_id = :new WHERE id = :id AND head_version_id = :base_version`。
4. **刚体挂点（Socket）与骨骼拓扑变更的校准对比测试门禁（A/B Calibration Gate）**：
   - 针对增量修改中涉及 `Sockets`（武器挂点/特效锚点）微调或 `Rig`（骨骼拓扑重绑定 + `AW-019` 动作重定向）的场景，强制设立自动化**前后同帧姿态校准对比测试（Pose & Socket Drift Calibration）**：
     1. 锁定组件（`Mesh / Material`）规范化语义哈希必须 **100% 恒等（0 差异）**；
     2. 对挂点与重定向动画采样关键帧（0% / 25% / 50% / 75% / 100%），对比修改前后世界空间变换矩阵：**挂点位置偏移（Position Drift）$\le 0.5\text{ mm}$、旋转误差（Angular Error）$\le 0.1^\circ$、脚底接触点滑动（Foot Skate）不劣化**，并输出量化校准对比报告写入 `validation_reports`，未达标直接拦截提交。

### 3.5 视口二进制协议、三引擎无感交付与工作台 Figma 设计门禁（AW-005 / AW-025 / AW-029~031）

1. **Tauri 2 + Three.js 二进制流传输**：
   - 严禁通过 Tauri JSON `invoke` 传输 3D 网格、纹理或音频字节。
   - 核心通过 `register_asynchronous_uri_scheme_protocol("assetweft", ...)` 暴露 `assetweft://blob/<sha256>`，支持 HTTP Range 与 `ArrayBuffer` 零拷贝直传 Three.js `GLTFLoader`；切换资产时强制执行 Three.js 显存回收（`geometry.dispose()`, `material.dispose()`, `texture.dispose()`）。
2. **三引擎交付与“零闭源依赖 + 无感三层Unreal管线”设计**：
   - **主仓库开源许可证**：统一采用 **`Apache-2.0 OR MIT` 双许可**（Owner 已拍板）。
   - **Unity**：基于 `com.unity.cloud.gltfast` (Apache-2.0) + AssetWeft Unity Editor C# 插件读取 `ExportManifest`。
   - **Unreal Engine（三层无感开源交付架构）**：用户在工作台只需点击「一键发送到 Unreal」，无需关心底层是 glTF 还是 FBX：
     - **第 1 层（默认主通道，零外部依赖）**：AssetWeft 输出标准 `glTF 2.0 (.glb)` + `ExportManifest.json`。Unreal C++ 插件调用引擎自带 **Interchange 原生管线**导入骨骼网格、材质与动画；若检测到旧版 UE 5.5（存在原生 glTF 表情曲线遗漏 Bug），由我们的 **Unreal C++ 插件直接读取 `ExportManifest.json` 在引擎内存里自动补齐 Morph Target 动画曲线**，从而在 UE 5.5 / 5.6+ 下均无需依赖闭源 Autodesk FBX SDK！
     - **第 2 层（纯 Rust 开源 FBX 写出器）**：针对必须交付 `.fbx` 物理文件的传统外包流水线，使用纯 Rust 开源二进制写出器生成静态网格与基础骨骼 `.fbx`，100% 规避 Autodesk 闭源 SDK 传染。
     - **第 3 层（本机 Blender CLI 无感桥接）**：针对含特殊自定义约束的复杂绑定 FBX，若检测到用户本机安装了 Blender，自动后台调用 `blender --background` 完成工业级 FBX 2020.2 转换。
   - **Godot**：基于 Godot 4.x 原生 glTF 2.0 导入器 + GDScript `EditorImportPlugin`。
   - **重型 AI 模型与 GPL-3.0 进程隔离**：桌面客户端保持纯 Rust 零 Python 依赖；`UniRig`、`TRELLIS.2`、`ACE-Step-1.5` 通过标准 Docker / 远端推理 Endpoint 按需接入；`piper1-gpl` (GPL-3.0) 严格作为可选外部独立进程调用，默认内置 TTS 优先采用 Apache-2.0 的 `Kokoro-82M`。
3. **M6 桌面工作台视觉设计门禁（Mobbin $\to$ Eagle $\to$ Figma $\to$ Owner 审核）**：
   - 在编写任何 `AW-025` 工作台前端页面代码前，强制执行三步视觉门禁（遵循 `[细则·视觉]`）：
     1. **Mobbin 采集并入库 Eagle**：按工作台核心视图（资产网格库、3D/2D/音频主视口、DAG 节点编排器、A/B 增量对比视图、三引擎导出面板、预算设置）在 Mobbin 采集高水准参考图，全部存入本地 Eagle 素材库并 `GET /api/item/list` 读回校验，生成 `references/MOBBIN-EAGLE-MANIFEST.md`；
     2. **Figma 模块化设计**：在 Figma 中构建 Variables 令牌、基础组件与 Auto Layout 完整工作台页面；
     3. **Owner 审核门禁**：导出 Figma 页面完整预览提交 Owner 审核，**Owner 审核通过后方可启动前端实现**。

---

## 4. 任务卡 DAG 依赖修正与 CI 汇总门禁（第 10 / 12 章）

### 4.1 任务卡依赖图（DAG）四项修正

1. **AW-016（增量提交）**：依赖从 `010, 014, 015, 017` 增补为 `010, 014, 015, 017, 022`（需要 M3 的 `AW-022` 提供 Mesh/Skeleton/Skin 组件级语义哈希计算器）。
2. **AW-033（CLI / MCP stdio / 公开接口契约）**：将 `AW-016` 从前置硬阻塞改为两阶段交付——`AW-033a`（依赖 `008, 013, 014, 017`）在 W2 提前打通 CLI/MCP 的资产查询、任务提交与验证接口，支撑自动化测试；`AW-033b` 在 `AW-016` 完成后合入增量提交命令。
3. **AW-022（Mesh/Skin/Skeleton 基础编辑）**：依赖从 `003, 017` 增补为 `003, 005, 017`（承接 `AW-005` 视口二进制传输与渲染契约）。
4. **AW-029 / AW-030 / AW-031（三引擎交付）**：基础骨架/动画/Sprite/音频交付依赖 `017, 022, 023, 024`；面部 Morph 与挂点交付子项显式依赖 `AW-025`。

### 4.2 GitHub Actions 防 Pending 死锁汇总门禁

- **禁止**在 Required Check 对应的 Workflow 顶层使用 `paths:` 过滤。
- 统一由 `.github/workflows/ci-gate.yml` 响应 `pull_request` 与 `merge_group`：
  1. `changes` Job 计算变更路径掩码（`rust`, `web`, `native`, `contracts`）；
  2. `c1-fast`（必跑：格式、编译、`sqlx prepare --check`、表归属 lint、契约测试）；
  3. `c2-integration`（按需 Job 级 `if:`：Linux PG18 容器迁移、CAS 崩溃注入、预算并发锁测试）；
  4. `c3-desktop-native`（按需 Job 级 `if:`：Windows / macOS 托管 PG18 启停恢复与 Tauri 视口冒烟）；
  5. `gate-verdict`（`if: always()` 汇总 Job）：检查所有應跑 Job 均为 `success`、未触发 Job 为合法 `skipped`，一旦存在 `failure` 或 `cancelled` 立即 `exit 1`。分支保护规则仅将 `gate-verdict` 设为唯一必过状态检查。
