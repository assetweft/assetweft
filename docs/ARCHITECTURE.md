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
3. **版本握手与平滑升级（Drain 防死锁协议）**：
   - 握手消息 `Hello / Welcome` 采用独立于业务消息的轻量版本协商帧，校验 `(proto_min, proto_max)`、`core_build` 与 `db.schema_version`。
   - 客户端升级后发现旧核心版本不匹配时，在不申请业务 `Lease` 的前提下直接发送 `DrainRequest`：旧核心将 `draining = true`、拒绝新任务、忽略或撤销空闲计时器，并在 `active_runs == 0` 时立即优雅停止 PG 并退出，彻底消除“新客户端持 Lease 导致旧核心永不退出”的死锁（修补 `HY4-CB-5.2`）。

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

### 3.4 预算原子预留、跨平台浮点确定性契约与姿态校准门禁（AW-014 / AW-015 / AW-016 / AW-022 / AW-023）

1. **按通道原币种/原积分直显与防超支原子预留 SQL**：
   - **直观计价模型（不搞自动跨币种汇率换算）**：各外部 AI 平台按其原生计价单位独立建账（美元通道显示 `USD $`、人民币通道显示 `CNY ¥`、积分制平台如 Tripo/Meshy 显示 `Credits 积分`、本地开源模型显示 `Free 0 成本`）。用户在界面中只需设定直观的**「单次任务最高限额」**与**「项目/日总限额」**。
   - **底层整数防精度丢失**：数据库内部统一采用整数微单位（`minor_units`，如 `1 元 = 1,000,000 微元` 或 `1 积分`）存储，彻底杜绝浮点数加减误差。
   - **防超支原子预留**：严禁“先 SELECT 余额再 INSERT 任务”。所有付费步骤调用前，必须执行单条带条件更新：
     `UPDATE budget_accounts SET reserved_minor = reserved_minor + :cost WHERE id = :acct AND reserved_minor + spent_minor + overdraft_minor + :cost <= limit_minor`。影响行数为 0 则立即拒绝并返回 `BUDGET_EXCEEDED`。
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

### 3.5 视口二进制协议、三引擎转换数学契约与工作台 Figma 设计门禁（AW-005 / AW-025 / AW-029~031 / AW-034）

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
3. **三引擎交付与“零闭源依赖 + 无感三层Unreal管线”设计**：
   - **主仓库开源许可证**：统一采用 **`Apache-2.0 OR MIT` 双许可**（Owner 已拍板）。
   - **Unity**：基于 `com.unity.cloud.gltfast` (Apache-2.0，同时固定稳定版兜底与离线校验和) + AssetWeft Unity Editor C# 插件读取 `ExportManifest v2`。
   - **Unreal Engine（三层无感开源交付架构）**：用户在工作台只需点击「一键发送到 Unreal」，无需关心底层是 glTF 还是 FBX：
     - **第 1 层（默认主通道，零外部依赖）**：AssetWeft 输出标准 `glTF 2.0 (.glb)` + `ExportManifest v2`。Unreal C++ 插件调用引擎自带 **Interchange 原生管线**导入骨骼网格、材质与动画；若检测到旧版 UE 5.5（存在原生 glTF 表情曲线遗漏 Bug），由我们的 **Unreal C++ 插件直接读取 `ExportManifest.morph_target_curves` 在引擎内存里自动补齐 Morph Target 动画曲线**，从而在 UE 5.5 / 5.6+ 下均无需依赖闭源 Autodesk FBX SDK！
     - **第 2 层（纯 Rust 开源 FBX 写出器）**：针对必须交付 `.fbx` 物理文件的传统外包流水线，使用纯 Rust 开源二进制写出器生成静态网格与基础骨骼 `.fbx`，100% 规避 Autodesk 闭源 SDK 传染。
     - **第 3 层（本机 Blender CLI 无感桥接）**：针对含特殊自定义约束的复杂绑定 FBX，若检测到用户本机安装了 Blender，自动后台调用 `blender --background`（受外部进程沙箱隔离，仅挂载临时输入输出目录）完成工业级 FBX 2020.2 转换。
   - **Godot**：基于 Godot 4.x 原生 glTF 2.0 导入器 + GDScript `EditorImportPlugin`。
   - **重型 AI 模型与 GPL-3.0 进程沙箱隔离（`ExternalProcessSandboxPolicy`）**：桌面客户端保持纯 Rust 零 Python 依赖（通过安装包审计 Gate `G-R03-1` 强制校验）；`UniRig`、`TRELLIS.2`、`ACE-Step-1.5` 通过标准 Docker / 远端推理 Endpoint 按需接入；`piper1-gpl` (GPL-3.0) 严格作为可选外部独立进程调用（绑定 `127.0.0.1` 临时端口 + 每次会话随机 Token 鉴权 + 仅传显式输入字节，严禁直访 CAS 目录或凭据库）。
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
