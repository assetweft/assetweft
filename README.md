# AssetWeft（游织）

**开源、本地优先的游戏资产生成、增量编辑与多引擎交付工作台**  
*Open-Source, Local-First Game Asset Generation, Incremental Editing & Multi-Engine Delivery Workbench*

---

## 核心定位

**AssetWeft（游织）** 致力于解决 AI 游戏资产进入真实工程的“最后一公里”问题：通过**参考图、提示词、已有资产及增量修改契约**，生成、编辑、验证并向 **Unity、Unreal Engine、Godot** 交付结构完整、组件可锁定、成本可控的游戏资产。

- **增量修改契约（Incremental Modification Contract）**：每次操作显式绑定 `base_version`、`locked_components` 与 `editable_components`。增加一个攻击动作或替换一套贴图时，严格保持已锁定的 Mesh、材质与骨骼语义哈希不变，拒绝“改一个动作重做整个角色”。
- **强边界反腐败层（Anti-Corruption Layer）**：外部模型输出必须经过 `Schema 检查 -> 业务语义检查 -> 权限与预算原子预留 -> M3 确定性处理与 M5 技术校验`，严禁模型响应直接成为数据库写入指令或覆盖锁定资产。
- **单一本地核心与托管持久化**：桌面 UI（Tauri 2 + Three.js）、CLI 与 `MCP stdio` 共享同一个按需启动的 Rust 核心守护进程（`core-daemon`），内置托管 PostgreSQL 18（存储版本图、运行记录与预算账本）与内容寻址文件库 CAS（存储大文件 Blob）。
- **工程诚实度与状态分离**：严格正交分离**执行状态**（`QUEUED / RUNNING / REMOTE_UNKNOWN / SUCCEEDED / FAILED / CANCELLED`）、**技术检查**（`NOT_RUN / PASS / FAIL / WAIVED`）与**用户评价**，并强制区分资产来源（`AI_GENERATED / PROCEDURAL / IMPORTED / MANUALLY_EDITED / TEST_FIXTURE`）。

---

## 仓库权威文档导航（W0 基线）

本仓库严格执行“一事一处、唯一权威”原则：

| 文档路径 | 说明 |
|---|---|
| [`docs/DEVELOPMENT_BASELINE.md`](./docs/DEVELOPMENT_BASELINE.md) | 游织 v1.0 开发架构、权威依据与 40 张任务卡执行基线（`AW-DEV-001`） |
| [`docs/PRODUCT.md`](./docs/PRODUCT.md) | 产品范围、已确认需求追踪矩阵（`R01–R13`）、V1 边界与六大验收基准样本 |
| [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) | 六大业务模块边界、Rust Workspace 分层、20 张核心表归属矩阵、PG18/CAS/IPC/预算/引擎交付可执行契约 |
| [`docs/AUDIT_AND_OPEN_QUESTIONS.md`](./docs/AUDIT_AND_OPEN_QUESTIONS.md) | 多模型联合架构审计报告摘要（8 项 P0 关键补齐）与待业主决策问题清单 |
| [`evidence/sources.yaml`](./evidence/sources.yaml) | 外部技术规范（`E01–E11`）与候选开源仓库精确 Commit SHA / SPDX 许可证锁定清单 |
| [`evidence/capabilities.yaml`](./evidence/capabilities.yaml) | 基础设施、生成模型与三引擎交付能力验证状态表（未实测项严格标记 `NOT_RUN`） |
| [`INTEGRITY.json`](./INTEGRITY.json) | 原始开发基线交付物完整性校验元数据 |

---

## 开发波次路线图

- **W0：建立基线**（`AW-001`、`AW-002`、`AW-003`、`AW-006`）— 锁定架构文档、外部权威版本、核心数据契约与许可证准入清单。
- **W1：验证三项核心技术风险**（`AW-004`、`AW-005`、`AW-007` 并行）— 验证双平台 PostgreSQL 18 托管原型、Tauri+Three.js 二进制视口性能原型、真实生成可行性实验。
- **W2：本地闭环**（`AW-008` ~ `AW-018`、`AW-022`、`AW-033a`）— 打通单核心进程 IPC、CAS 版本原子提交、DAG 编排、预算幂等锁与基础校验。
- **W3：专业生产能力**（`AW-019` ~ `AW-028`、`AW-033b`、`AW-034`）— 落地 3D 骨骼动作、2D 图集排版、VFX 参数化、本地音频处理与 MCP/Skills。
- **W4：工作台与三引擎交付**（`AW-029` ~ `AW-032`、`AW-035`）— 完成 Unity、Unreal Engine、Godot 导入验证与完整混沌 Boss 端到端集成验收。
- **W5：开源发行**（`AW-036`）— 双平台安装包签名、自动升级与恢复验收。
