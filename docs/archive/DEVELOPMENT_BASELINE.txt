# 游织 AssetWeft

## 开发架构、权威依据与任务卡执行基线

| 文档属性 | 内容 |
|---|---|
| 文档编号 | AW-DEV-001 |
| 版本 | 1.0 — 开发基线 |
| 资料核查基准日期 | 2026 年 11 月 11 日 |
| 建议仓库位置 | `docs/DEVELOPMENT_BASELINE.md` |
| 产品名称 | AssetWeft／游织，工作名，商标、域名及应用标识尚未完成核验 |
| 当前性质 | 工程设计与任务拆分基线，不是代码或模型运行验收报告 |

**本文用于开始开发，而不是继续扩大功能范围。** 它确定模块边界、数据契约、技术选择、开发顺序、CI 门禁和任务验收方式。

其中，**架构决策是本项目的设计；官方能力是外部证据；实际可用性必须由运行结果证明。** 下面的任务编号是文档内编号，不代表已经在 GitHub 创建了 Issue，也不代表代码或模型测试已经完成。

产品继续使用 **AssetWeft／游织** 作为工作名；正式商标、域名和应用标识尚未完成核验。

---

## 目录

1. 项目目标与边界
2. “唯一权威性”的工程落实
3. 同类项目审查后的采用决定
4. 架构基线：六个业务模块，一套应用服务
5. 开发语言与桌面技术
6. 必须先实现的数据与行为契约
7. 生成能力的执行分工
8. PostgreSQL、资产文件与未来云同步
9. 多模型、MCP 与 Skills 的治理
10. GitHub CI 与 PR 门禁
11. 任务卡的统一规则
12. 模块任务卡
13. 开发波次与任务分配
14. PR 的完成定义
15. 验收基准：先证明可用，再证明通用
16. 开工状态与执行规则

---

# 一、项目目标与边界

## 1.1 产品目标

用户通过**参考图、提示词、已有资产及增量修改要求**，生成、编辑、管理并导出游戏资产。

产品必须回答的核心问题是：

> 用户得到的资产，能否保持要求的外观和风格，具有需要的结构、动作、特效与音频，并进入实际游戏工程？

因此，产品的核心不是聊天界面，也不是模型数量，而是：

**生成效果、可编辑性、资产一致性、成本可控性与游戏交付能力。**

## 1.2 已确认需求

| 需求编号 | 已确认要求 | 开发约束 |
|---|---|---|
| R01 | Windows、macOS 桌面客户端 | 两个平台均需要原生安装、启动、升级和恢复验证 |
| R02 | 开源、本地优先 | 本地核心功能不依赖平台登录 |
| R03 | Rust 核心，不使用 Python 作为产品必需运行时 | 外部模型服务可以使用不同语言，但不能成为隐藏的客户端依赖 |
| R04 | 自动携带并管理 PostgreSQL | 用户不需要安装 Docker、手动建库或配置数据库 |
| R05 | 本地素材库、版本历史、满意度记录 | 大文件与数据库元数据分离 |
| R06 | 基准素材驱动、增量修改 | 默认不修改用户锁定的模型、材质、骨架等组件 |
| R07 | 角色、Boss、骨架、蒙皮、动作、服装、多角色交互、面部动画 | 分别定义能力与验收，不承诺任意角色一次生成成功 |
| R08 | 2D、像素、Sprite、UI、卡牌、建筑、植被、VFX、音频 | 使用不同生产路径，共享资产与工作流基础设施 |
| R09 | Unity、Unreal、Godot | 三个适配器分别验证，不能用一个 GLB 宣称全面兼容 |
| R10 | 用户自带 API、可选自托管或本地模型 | 无 GPU 用户不必部署模型 |
| R11 | CLI、MCP、Skills、API/SDK | 共用应用服务，不建立重复业务实现 |
| R12 | GitHub CI、按任务卡与 PR 开发 | 未取得运行证据不能关闭相关验收任务 |
| R13 | 后续云同步、创作者市场、积分 | V1 不提前实现复杂云平台和交易账本 |

**不在产品范围内：** 完整游戏逻辑、伤害计算、任务系统、网络同步、游戏经济、完整关卡设计，以及任意商业游戏的资源替换工具。

“完全自定义”指开放资产、绑定、动画、交互表现和导出参数；**不等于 V1 重做 Blender 的全部建模、雕刻与材质功能。**

---

# 二、“唯一权威性”的工程落实

不能将“唯一权威”理解为所有结论只看一个网站。应当分成两种权威。

## 2.1 外部技术事实：由对应技术所有者负责

| 证据编号 | 核对对象 | 权威来源与本轮结论 |
|---|---|---|
| **E01** | Rust 依赖锁定 | Cargo 官方区分依赖声明与精确锁定信息；应用仓库应提交 `Cargo.lock`。见 [Cargo.toml vs Cargo.lock](https://doc.rust-lang.org/nightly/cargo/guide/cargo-toml-vs-cargo-lock.html)。 |
| **E02** | 桌面进程、外部二进制、更新 | Tauri 官方提供进程模型、Sidecar 打包与签名更新机制；这不自动证明完整 PostgreSQL 分发包已经可用。见 [进程模型](https://v2.tauri.app/concept/process-model/)、[Sidecar](https://v2.tauri.app/develop/sidecar/)、[Updater](https://v2.tauri.app/plugin/updater/)。 |
| **E03** | PostgreSQL 生命周期 | 官方提供 `initdb`、`pg_ctl`、`pg_upgrade`；初始化认证、升级与恢复需要明确设计。见 [initdb](https://www.postgresql.org/docs/current/app-initdb.html)、[pg_ctl](https://www.postgresql.org/docs/current/app-pg-ctl.html)、[升级说明](https://www.postgresql.org/docs/current/pgupgrade.html)。 |
| **E04** | 3D 数据与校验 | Khronos glTF 规范及官方 Validator；文件规范合格不等于动作自然或游戏表现合格。见 [glTF 2.0](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)、[glTF Validator](https://github.com/KhronosGroup/glTF-Validator)。 |
| **E05** | MCP | 官方协议与官方 Rust SDK；协议版本必须与宿主兼容，不跟随 `latest` 自动升级。见 [协议版本页面](https://modelcontextprotocol.io/specification/2026-07-28)、[Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)。 |
| **E06** | Agent Skills | 官方规范规定 `SKILL.md`、元数据及可选脚本、参考资料等结构。见 [Agent Skills Specification](https://agentskills.io/specification)。 |
| **E07** | Unity 交付 | Unity 官方 glTFast 提供 Editor/Runtime 导入导出；具体资产能力仍需测试。见 [Unity glTFast](https://github.com/Unity-Technologies/com.unity.cloud.gltfast/blob/main/Packages/com.unity.cloud.gltfast/Documentation~/index.md)。 |
| **E08** | Unreal 交付 | 官方骨骼模型与动画导入流程；FBX 管线明确采用 FBX 2020.2。见 [FBX Skeletal Mesh Pipeline](https://dev.epicgames.com/documentation/unreal-engine/fbx-skeletal-mesh-pipeline-in-unreal-engine)。 |
| **E09** | Godot 交付 | 官方支持 glTF/GLB、骨骼与动画相关导入；采用稳定版本文档，不以开发版文档冻结产品能力。见 [Available 3D formats](https://docs.godotengine.org/en/stable/tutorials/assets_pipeline/importing_3d_scenes/available_formats.html)。 |
| **E10** | GitHub 门禁 | Required Checks、路径跳过、`merge_group`、权限与可信执行边界。见 [Required status checks](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks)、[安全使用 pull_request_target](https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target)。 |
| **E11** | 真实生成能力 | 原始论文、作者仓库、模型卡与实际 API 文档，分别证明方法、实现和接口。例：[UniRig 原始论文](https://arxiv.org/abs/2504.12451)、[UniRig 官方仓库](https://github.com/VAST-AI-Research/UniRig)、[TRELLIS.2 官方仓库](https://github.com/microsoft/TRELLIS.2)。 |

对于许可证，必须分别核对：

**代码、模型权重、数据、声音资源、依赖和服务条款。**

不能依据仓库首页的 MIT 标识，推断全部模型权重、声音和输出都拥有相同许可。

## 2.2 项目内部：每种信息只有一个维护源

| 信息 | 唯一维护位置 |
|---|---|
| 产品范围与需求编号 | `docs/PRODUCT.md` |
| 模块与依赖规则 | `docs/ARCHITECTURE.md` |
| 对外数据契约 | `contracts/` |
| 模型与引擎能力状态 | `evidence/capabilities.yaml` |
| 外部来源与版本 | `evidence/sources.yaml` |
| 开发任务 | GitHub Issues；本文件中的编号保持稳定 |
| PR 验收证据 | 对应 PR 与 CI 产物 |
| 重大技术取舍 | `docs/adr/` |

**禁止同时手工维护几张互相矛盾的“支持能力表”。**

本轮查阅的分支页面不全部具有固定提交地址。因此，**AW-001 必须将实际采用的依赖、文档和源码固定到版本或提交；未锁定前不进入实现基线。** 不补写不存在的 Commit SHA。

---

# 三、同类项目审查后的采用决定

本轮进一步核对了部分实际文件，而不是只记录项目名称。

| 项目 | 核查到的实现 | 采用决定 |
|---|---|---|
| **Asset Tap** | Rust Workspace；CLI、桌面及 MCP；MCP 文档强调复用 CLI 的 JSON 行为 | 借鉴共享核心和统一输出契约，不复制其全部 UI 技术栈 |
| **AssetForge** | 分阶段资产流水线；仓库同时包含占位/Stub 后端 | 借鉴可跳过阶段；明确隔离编排测试与真实生成测试 |
| **Blender Agent Skills** | 专业 Skill 路由，通过 Blender `bpy`、CLI/MCP 执行 | 借鉴技能组织，不让客户端因此强制依赖 Python |
| **TRELLIS.2、UniRig** | 原始论文与公开实现分别覆盖 3D 生成、骨架及蒙皮方向 | 作为执行器候选，不让其内部格式决定产品数据模型 |

对应原始来源：

- [Asset Tap — Cargo.toml](https://github.com/nightandwknd/asset-tap/blob/main/Cargo.toml)
- [Asset Tap — MCP.md](https://github.com/nightandwknd/asset-tap/blob/main/docs/MCP.md)
- [AssetForge](https://github.com/InFaNsO/AssetForge)
- [Blender Agent Skills — blender-core/SKILL.md](https://github.com/EL4CTEO/blender-skills/blob/main/skills/blender-core/SKILL.md)
- [TRELLIS.2](https://github.com/microsoft/TRELLIS.2)
- [UniRig](https://github.com/VAST-AI-Research/UniRig)

这些判断来自实际仓库说明、`Cargo.toml`、`MCP.md`、`SKILL.md` 及作者论文。**尚未克隆运行这些项目，因此没有将它们标记为本项目复现通过。**

**最终策略：借鉴已有工程结构，但自行建立资产契约、生成记录、质量验证和游戏交付。**

---

# 四、架构基线：六个业务模块，一套应用服务

## 4.1 模块划分

| 模块 | 职责 | 拥有的数据 | 不负责 |
|---|---|---|---|
| **M1 资产与素材库** | 导入、索引、版本、依赖、历史、反馈 | Asset、AssetVersion、Blob、Reference、Feedback | 模型推理 |
| **M2 生产编排** | 规格、配方、任务、预算、恢复、增量提交 | GenerationSpec、Recipe、Run、Step | 具体模型协议 |
| **M3 确定性资产处理** | 几何、绑定、动画、图像、音频、参数化特效 | 处理参数与结果 | 账户和付费服务管理 |
| **M4 生成连接器** | LLM、图片、3D、动作、音频服务接入 | ProviderProfile、Capability、RemoteJob | 最终质量判定 |
| **M5 验证与交付** | 技术检查、报告、目标引擎转换 | ValidationReport、ExportManifest | 完整游戏逻辑 |
| **M6 工作台与开放入口** | 桌面、CLI、MCP、SDK、Skills | 交互状态与调用映射 | 绕过应用服务直接写库 |

PostgreSQL、文件系统、操作系统凭据库是基础设施，不是新增业务平台。

## 4.2 运行结构

```text
桌面 UI ─────┐
CLI ─────────┼── 本地应用服务
MCP stdio ───┘         │
                       ├── M1 资产与版本
                       ├── M2 生产编排
                       ├── M3 确定性处理
                       ├── M4 生成连接器
                       └── M5 验证与交付
                               │
              ┌────────────────┼────────────────┐
              ▼                ▼                ▼
       PostgreSQL          本地文件库       外部模型服务
```

**本地核心由一个按需启动的进程统一管理。**

桌面、CLI、MCP 不分别启动数据库，不分别维护任务状态。程序退出时按任务策略处理后台任务；不默认注册开机运行的系统服务。

## 4.3 源码依赖约束

```text
domain：不依赖 UI、数据库、网络 SDK
processing：依赖 domain
application：依赖 domain、processing 和必要端口
adapters：实现数据库、文件、Provider、MCP 等端口
delivery：实现目标引擎交付端口
apps：负责装配上述实现
```

禁止：

- 动画算法依赖 PostgreSQL 表结构。
- 资产格式依赖某个 AI 响应对象。
- MCP 工具直接执行 SQL。
- 模型响应直接成为数据库写入指令。
- UI 代码自行调用付费生成接口。

只在明确会替换的边界建立接口，不把每个内部函数包装成抽象层。

---

# 五、开发语言与桌面技术

| 部分 | 基线选择 | 状态 |
|---|---|---|
| 核心与本地服务 | Rust | 确定 |
| UI | TypeScript，React 作为默认候选 | 框架在初始 UI PR 固定 |
| 桌面外壳 | Tauri 2 | 首选，受 AW-004/AW-005 原型约束 |
| 首个 3D 视口 | Three.js | 首选原型方案 |
| 本地元数据库 | PostgreSQL 18 系列 | 固定受支持补丁版本，不自动追逐大版本 |
| 大文件 | 内容寻址文件库 | 确定 |
| Unity | C# | 确定 |
| Unreal | C++ | 确定 |
| Godot | GDScript 优先 | 必要时再引入原生扩展 |
| 必要原生 SDK | C/C++ 小范围封装 | 按任务批准 |
| Go | V1 不引入 | 当前无明确收益 |
| Python | 不作为产品与项目自定义工具的必需运行时 | 外部服务内部实现例外 |

Tauri 使用系统 WebView，Windows 与 macOS 存在平台差异；Three.js 有官方 glTF 加载能力，但不是现成的专业绑定编辑器。**是否足够由原型决定，不能先宣传“高性能专业编辑器”。**

官方依据：[Tauri 进程模型](https://v2.tauri.app/concept/process-model/)、[Three.js GLTFLoader](https://threejs.org/docs/pages/GLTFLoader.html)。

Tauri 的 Sidecar 功能只能解决外部程序启动与打包的一部分。PostgreSQL 还需要共享库、运行资源、认证、数据目录和升级处理，不能只复制三个可执行文件。

官方依据：[Tauri Sidecar](https://v2.tauri.app/develop/sidecar/)。

---

# 六、必须先实现的数据与行为契约

## 6.1 核心对象

| 对象 | 必须包含 |
|---|---|
| `ProjectProfile` | 风格、单位、坐标、目标平台、引擎、资产预算 |
| `ReferenceSet` | 文件版本、用途、优先级、用户允许的变化 |
| `AssetVersion` | 资产 ID、版本 ID、组件引用、父版本、来源 |
| `GenerationSpec` | 操作目标、基础版本、允许修改范围、输出要求 |
| `RecipeVersion` | 步骤、依赖、参数、执行器要求、校验规则 |
| `GenerationRun` | 实际输入、模型版本、任务号、费用、输出、错误 |
| `ActionPresentation` | Motion、VFX、SFX、语音和事件引用 |
| `ValidationReport` | 检查项、工具版本、结果、未测试项 |
| `ExportManifest` | 文件、依赖、引擎版本、能力与限制 |

GLTF 是公共交换格式之一，不是完整工程数据库。骨架、动画等可映射到规范数据，生产历史、用户反馈与动作事件由项目契约另行保存。

官方依据：[Khronos glTF 2.0](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)。

## 6.2 增量修改契约

用户增加一次攻击，不允许自动重做角色。

每次操作必须携带：

```text
base_version
reference_versions
editable_components
locked_components
output_contract
budget_policy
```

提交流程：

**读取基础版本 → 生成候选 → 验证锁定组件 → 检查版本冲突 → 提交新版本。**

若当前骨架不能支持动作，返回明确的不支持原因，或者提出扩大修改范围的方案。不得静默修改骨架。

## 6.3 状态分离

```text
执行状态：QUEUED / RUNNING / REMOTE_UNKNOWN / SUCCEEDED / FAILED / CANCELLED

技术检查：NOT_RUN / PASS / FAIL / WAIVED

用户评价：UNRATED / ACCEPTED / NEEDS_CHANGES / REJECTED
```

`WAIVED` 只能用于允许人工放行的检查，并记录原因。文件不可解析、引用缺失等阻断错误不能靠“用户满意”掩盖。

## 6.4 空间、时间与媒体规范

建议内部统一：

- 几何使用米和明确的右手坐标约定；引擎适配器负责转换。
- 动画关键帧使用秒，不将所有资产强制为同一帧率。
- 骨骼 ID 与显示名称分离。
- Skin 保留实际权重信息；每顶点影响数由目标平台配置决定。
- 音频保存采样率、声道、时长和循环点。
- 像素资产保存画布、锚点、色板、方向、帧序列。
- VFX 区分引擎原生描述与烘焙序列帧。

**“输出契约满足”与“美术质量优秀”分别评价。**

---

# 七、生成能力的执行分工

## 7.1 本地与远程

| 任务 | 默认路径 | 说明 |
|---|---|---|
| 参考素材、索引、版本 | 本地 | 无需模型 |
| 需求整理与台词 | 用户 Agent 或 LLM API | 已由外部 Agent 生成合格规格时，不重复调用 LLM |
| 图片转 3D | 远程 API／用户自托管 | 不要求普通用户拥有 GPU |
| 骨架预测 | 专用服务或模型 | 骨架编辑仍在本地 |
| 触手、法环、简单循环 | 本地程序化 | 不默认调用视频模型 |
| 人形动作 | 动作库、重定向、动作 API | 按能力选择 |
| VFX | 本地参数化与图像素材 | 视频只作为可选参考或烘焙输入 |
| 配音 | 远程服务，或通过验证的本地插件 | 不强制随客户端安装大模型 |
| 音效、音乐 | 已核验的服务或用户部署模型 | 本地负责裁切、混音、循环、格式处理 |
| 检查和导出 | 本地 | 引擎验收使用实际引擎 |

GitHub 可以调度测试和用户授权的批量任务，但 GPU Larger Runner 等具有独立可用条件和计费方式，不能把它包装成免费模型服务器。

官方依据：[GitHub Larger Runners](https://docs.github.com/en/actions/concepts/runners/larger-runners)。

## 7.2 首批候选的采用限制

| 候选 | 本轮核对结果 | 进入开发的方式 |
|---|---|---|
| TRELLIS.2 | 作者论文与官方实现提供 3D 生成依据 | 远程执行器候选，不内嵌其训练架构 |
| UniRig | 骨架与蒙皮预测方向，有公开实现 | 分为骨架检查、修正、蒙皮三个可中断步骤 |
| Meshy Rigging API | 官方明确主要面向标准人形，不适用任意非人形 | 用于人形验证，不能用来承诺混沌 Boss |
| Kokoro | 官方权重页面标注 Apache-2.0 | TTS 候选；中文与本地运行仍需实测 |
| Piper 当前维护项目 | 当前仓库为 GPL-3.0 | 单独核查集成与声音模型许可 |
| ACE-Step 1.5 | 当前仓库标注 MIT | 音乐候选；固定版本、依赖与权重后验收 |

原始来源：

- [TRELLIS.2](https://github.com/microsoft/TRELLIS.2)
- [UniRig](https://github.com/VAST-AI-Research/UniRig)
- [UniRig 论文](https://arxiv.org/abs/2504.12451)
- [Meshy 官方 API 文档](https://docs.meshy.ai/)
- [Kokoro-82M 官方模型卡](https://huggingface.co/hexgrad/Kokoro-82M)
- [Piper 当前维护仓库](https://github.com/OHF-Voice/piper1-gpl)
- [ACE-Step 1.5 官方仓库](https://github.com/ace-step/ACE-Step-1.5)

依据来自原作者仓库、模型卡、论文及服务接口文档，不采用第三方宣传作为准入依据。

**缺少商用、再分发或输出条件证据的候选，可以保留为研究项，但不能进入默认发行配置。**

---

# 八、PostgreSQL、资产文件与未来云同步

## 8.1 本地数据库管理

客户端负责初始化、启动、健康检查、迁移、备份和停止。

必须满足：

- 当前用户权限运行。
- 不默认监听外部网络。
- 不使用无条件 `trust` 作为认证方案。
- 数据目录独立于安装目录。
- 卸载不自动删除用户素材。
- 多个入口共用同一个核心进程和数据库管理器。

PostgreSQL 官方明确说明初始化用户与认证要求；`pg_ctl` 提供启动和停止能力。

官方依据：[initdb](https://www.postgresql.org/docs/current/app-initdb.html)、[pg_ctl](https://www.postgresql.org/docs/current/app-pg-ctl.html)。

## 8.2 文件提交

```text
写入 work 临时目录
        ↓
文件完整性检查
        ↓
计算内容哈希
        ↓
原子落入 objects
        ↓
数据库事务登记引用
        ↓
提交资产版本
```

数据库事务与文件系统操作不是同一个事务。异常可能留下孤儿文件，因此采用延迟回收，不在异常路径立即删除可能仍被引用的内容。

**同一文件哈希可以去重，但来源和许可记录不能因此合并。**

## 8.3 最小表集合

`libraries`、`projects`、`assets`、`asset_versions`、`blobs`、`asset_dependencies`、`generation_runs`、`run_steps`、`feedback`、`provider_profiles`、`recipe_versions`。

模型密钥不明文存入这些表。数据库只保存凭据引用。

## 8.4 升级与备份

升级流程：

**预检查 → 一致性备份 → 新目录升级 → 验证 → 切换 → 保留恢复路径。**

PostgreSQL 官方区分小版本更新与大版本升级；不能把应用二进制回滚等同于数据库回滚。

官方依据：[PostgreSQL 升级文档](https://www.postgresql.org/docs/current/pgupgrade.html)。

## 8.5 云端演进

未来同步的是资产版本、哈希、元数据和文件，不是 PostgreSQL 数据目录。

云端冲突保留分支版本；登录不自动上传；删除和缓存清理必须区分。V1 只保留稳定 ID、版本关系与可导出的库清单，**不提前实现分布式复制或 CRDT 平台。**

---

# 九、多模型、MCP 与 Skills 的治理

## 9.1 不按品牌分配架构角色

GPT、Claude、DeepSeek、Gemini、Kimi、MiniMax、Hy4 可以作为规划或开发工具，但都不能自行宣称：

- API 存在。
- 许可证允许商用。
- 测试已经通过。
- 生成资产已经可以进入游戏。

ZCode 的官方定位是编程工作台与 Agent 执行环境；应登记为 **Agent 宿主**，不是与基础模型混在一个字段。Hy4 也要与 HY-Motion 等专用模型分别登记。

官方来源：[ZCode](https://github.com/zai-org/ZCode)。具体模型 ID、接口版本与能力由 AW-006、AW-019 分别固定和实测，未核实项不补写能力结论。

## 9.2 协议适配必须独立核对

OpenAI、Claude 的结构化输出、DeepSeek 的工具调用、Gemini 的协议状态以及 Kimi、MiniMax 的多轮消息要求存在差异。不能把“兼容接口”理解为所有字段和行为完全相同。

参考官方来源：

- [OpenAI Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
- [Claude 官方开发文档](https://platform.claude.com/docs/)
- [DeepSeek 官方 API 文档](https://api-docs.deepseek.com/)
- [Gemini API 官方文档](https://ai.google.dev/gemini-api/docs)
- [Kimi/Moonshot 官方文档](https://platform.moonshot.cn/docs)
- [MiniMax 官方文档](https://platform.minimax.io/docs)

统一治理方式是：

**模型输出 → Schema 检查 → 业务语义检查 → 权限与预算检查 → 确定性执行。**

协议要求保留的专用字段按文档处理，不把模型思考内容当作事实证据，也不默认写进公共日志。

## 9.3 MCP 与 Skills

首批 MCP 工具保持少量：

```text
project.inspect
asset.search
asset.inspect
generation.plan
generation.estimate
generation.run
job.get
job.cancel
asset.validate
asset.export
```

长任务先采用 `job_id + 查询` 的应用契约；协议级长任务能力在宿主和 SDK 测试通过后再启用。

Skills 只定义任务方法、输入要求和验收步骤，不拥有绕过预算、直接写数据库或执行任意命令的权力。官方 Skills 规范支持指令和附属资源结构，MCP 则负责工具交互。

官方依据：[Agent Skills](https://agentskills.io/specification)、[MCP](https://modelcontextprotocol.io/specification/2026-07-28)。

---

# 十、GitHub CI 与 PR 门禁

## 10.1 五类 CI

| 编号 | 工作流 | 触发与用途 |
|---|---|---|
| **C1** | `pr-fast` | 每个 PR：格式、编译、类型和核心契约测试 |
| **C2** | `integration` | 相关变更：数据库、文件提交、任务恢复、模拟 Provider |
| **C3** | `desktop-native` | 相关变更与发布前：Windows/macOS 安装、PG、恢复 |
| **C4** | `asset-real` | 人工批准或发布任务：真实生成、实际引擎、视觉结果 |
| **C5** | `release` | 受保护发布：锁版、签名、安装包、升级与恢复 |

### 必须保留的细节

**始终触发总入口，再按变更选择内部 Job。**

GitHub 官方说明，整个 Workflow 因路径条件被跳过时，Required Check 可能保持 Pending；依赖失败导致的 Job 跳过也需要明确处理。因此最后使用一个汇总门禁，核对“应运行的检查是否真正成功”。采用 Merge Queue 时增加 `merge_group`。

官方依据：[Troubleshooting required status checks](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks)。

PostgreSQL Service Container 的 Linux 测试只能证明 SQL/迁移，不证明 Windows/macOS 原生分发可用。两类证据分别保存。

官方依据：[Creating PostgreSQL service containers](https://docs.github.com/en/actions/tutorials/use-containerized-services/create-postgresql-service-containers)。

真实模型生成不在每个 PR 全量执行。涉及生成算法、模型版本或格式转换的变更，必须触发相应专项验证，不能只跑模拟后端。

## 10.2 安全边界

外部 PR 不接触发布密钥、付费 API 或有特权的自托管 Runner。不使用特权工作流执行不可信 PR 代码。Actions 固定到可核验提交，或者使用官方支持且已验证的依赖锁定机制。

官方依据：[Securely using pull_request_target](https://docs.github.com/en/actions/reference/security/securely-using-pull_request_target)。

## 10.3 防止过度测试与过度治理

| 必须做 | 按风险触发 | 默认不做 |
|---|---|---|
| 文件损坏、版本覆盖、重复收费测试 | 模型、GPU、性能、引擎全量验证 | 每个函数都写测试 |
| 凭据保护、路径与资源边界 | 复杂解析器隔离与专项模糊测试 | 所有素材上传安全扫描 |
| 迁移和恢复测试 | 新增数据库/格式版本审查 | 微服务、Kubernetes、事件总线 |
| 合同与错误语义测试 | 新依赖与许可证复核 | 为未来平台预建企业权限体系 |

**测试预算由测量结果决定，不以覆盖率或测试数量作为绩效。**

---

# 十一、任务卡的统一规则

以下每张任务卡继承统一要求：

**输入完整、依赖完成、范围明确，才能进入开发；存在真实产物、测试和审查记录，才能关闭。**

## 11.1 任务卡模板

```yaml
id: AW-010
title: 内容寻址文件库与资产提交
module: M1
status: TODO
owner_role: core-engineering
reviewer_role: independent-reviewer

depends_on:
  - AW-009

authority:
  - E03
  - E04

scope:
  - 流式计算文件哈希
  - 临时文件校验与原子落盘
  - 数据库引用提交
  - 异常恢复与延迟回收

out_of_scope:
  - 云端同步
  - 感知去重
  - 分布式事务

deliverables:
  - crates/adapters/src/storage/
  - tests/integration/asset_commit/

acceptance:
  - 数据库提交失败不会形成有效资产版本
  - 重复内容不会重复占用对象存储
  - 回收不会删除仍被引用的文件

ci:
  - C1
  - C2

evidence_required:
  - commit
  - workflow_run
  - test_report
```

这是任务格式示例，不是已经执行的记录。

## 11.2 任务状态

`TODO → READY → IN_PROGRESS → REVIEW → DONE`

另外允许：

- `BLOCKED`：缺少依赖、授权、算力或外部条件。
- `NEEDS_DECISION`：需要范围或架构决策。

**BLOCKED 不能通过伪造替代结果变成 DONE。**

负责人以角色分配，实际 GitHub 用户在建卡时绑定；不得虚构人员名单。

---

# 十二、模块任务卡

以下共 **40 张任务卡**：基础与 V1 开发 36 张、可选接入 2 张、后续云端 2 张。逻辑任务可以包含多个小 PR，不要求把大模块塞进单个 PR。

## 12.1 基础与关键风险验证

**责任：架构负责人、核心工程、技术美术。**

**依据：E01–E11。**

**禁止事项：先建设完整平台，再验证生成链路。**

| 卡号 | 依赖 | 交付任务 | 验收条件 / CI |
|---|---|---|---|
| **AW-001** | 无 | 建立 `PRODUCT`、`ARCHITECTURE`、`sources.yaml`；固定依赖、文档和引擎候选版本 | 每个事实有来源和状态；未知项不填假值；需求能追踪到任务卡。C1 |
| **AW-002** | 001 | Rust Workspace、锁文件、PR 模板、Issue 模板、最小 CI | 干净环境可构建；故意失败测试能阻止合并；不创建几十个空 crate。C1 |
| **AW-003** | 001 | `contracts/`：资产、版本、任务、表现包、报告 Schema | 正例、缺字段、非法引用均有预期结果；TS/Rust 契约一致。C1 |
| **AW-004** | 002 | Windows/macOS PostgreSQL 托管原型 | 普通用户启动、中文路径、重复启动、端口占用、异常退出恢复有实际日志。C3 |
| **AW-005** | 002、003 | Tauri＋Three.js 视口原型 | 加载真实 Mesh、Skin、Morph、动画；记录性能与内存；形成保留或替换决定。C3 |
| **AW-006** | 001 | 首批模型、API 与许可证准入清单 | 代码、权重、依赖、输出条件分开；选定可实际调用的首批执行器。C1＋人工审查 |
| **AW-007** | 003、006 | 真实生成可行性实验：人形基线与简化混沌 | 保存真实输入、原始产物、费用、失败记录；不得用 Stub 证明模型质量。C4 |

**AW-004、005、007 是三项独立的技术风险。** 任一不通过，应局部调整方案，不继续无条件扩大开发。

## 12.2 M1：资产库、持久化和恢复

**责任：核心工程。**

**依据：E03、E04。**

**范围边界：只做本地资产库，不做云同步。**

| 卡号 | 依赖 | 交付任务 | 验收条件 / CI |
|---|---|---|---|
| **AW-008** | 003、004 | 本地核心进程、单实例、IPC、PG 生命周期 | 桌面和 CLI 共享同一实例；断开 UI 不损坏运行任务；没有多个数据库互相争用。C1/C3 |
| **AW-009** | 008 | 数据表、迁移、Repository 实现 | 空库初始化、连续升级、失败恢复可验证；业务核心不直接持有 SQL。C2 |
| **AW-010** | 009 | 内容寻址存储、临时区、版本原子提交 | 同内容去重；DB/文件异常可恢复；引用仍有效时不误删。C2 |
| **AW-011** | 010 | 索引、标签、搜索、生成历史、版本比较、满意度 | 评价绑定具体版本；删除预览不删除原始资产；不满意原因可检索。C1/C2 |
| **AW-012** | 009、010 | 一致性备份、恢复、升级保护 | 在新目录恢复完整资产库；数据库引用与文件一致；升级失败保留恢复路径。C2/C3 |

## 12.3 M2：规格、工作流与成本

**责任：核心工程、生成集成工程。**

**依据：项目契约、E11 对应的实际接口。**

**范围边界：本地持久任务，不建分布式调度系统。**

| 卡号 | 依赖 | 交付任务 | 验收条件 / CI |
|---|---|---|---|
| **AW-013** | 003 | GenerationSpec、Recipe、引用优先级、属性锁定 | 同一任务可由 UI 或外部 Agent 提交；非法修改范围被拒绝。C1 |
| **AW-014** | 009、010、013 | DAG 执行、任务状态、取消、恢复、有限重试 | 依赖失败不执行下游；重启恢复状态；取消不丢失已产生的资产。C2 |
| **AW-015** | 014 | 预算、实际用量、幂等和远端未知状态 | 超时不盲目重复付费提交；未知费用不是零；并发任务不会绕过预算。C2 |
| **AW-016** | 010、014、015、017 | 候选版本、差异检查、增量提交 | 只改动作时 Mesh/材质/Rig 哈希不变；基础版本变化时报告冲突。C2 |

预算控制的是**后续允许提交的任务**，不能保证取消已经被服务商接受的任务一定停止计费。

## 12.4 M5 基础：统一质量报告

**责任：质量工程、技术美术。**

| 卡号 | 依赖 | 交付任务 | 验收条件 / CI |
|---|---|---|---|
| **AW-017** | 003、010 | 验证器接口、错误等级、报告及证据结构 | `NOT_RUN` 与 `FAIL` 分开；Mock、程序化生成、AI 生成来源明确；报告可追踪输入版本。C1/C2 |

这是公共验证基础，不是一套万能 AI 评分系统。

## 12.5 M4：模型与执行器连接

**责任：生成集成工程。**

**依据：E11 和各 Provider 的官方接口。**

**范围边界：不自动注册付费账户，不默认购买算力。**

| 卡号 | 依赖 | 交付任务 | 验收条件 / CI |
|---|---|---|---|
| **AW-018** | 006、014、015 | Provider 能力描述、凭据引用、提交/查询/取消/下载协议 | 密钥不出现在项目与日志；返回内容经过类型和大小检查；不支持的能力明确报错。C2 |
| **AW-019** | 013、018 | LLM 协议适配与跨模型任务用例 | GPT、Claude、DeepSeek、Gemini、Kimi、MiniMax、Hy4 分别登记测试状态；不以“兼容”代替测试。C2/C4 |
| **AW-020** | 017、018 | 图像、3D、绑定、动作首批真实连接器 | 下载并校验真实结果；异形任务不会路由到仅支持人体的绑定接口。C2/C4 |
| **AW-021** | 017、018 | TTS、音效、音乐连接器 | 台词与语音版本对应；采样率/时长可读取；输出条款及模型版本可追踪。C2/C4 |

没有对应凭据的 Provider 不强行跑 CI；其状态保持未实测，不能出现在“已验证支持”的列表中。

## 12.6 M3：3D、动作和媒体生产

**责任：图形工程、动画工程、技术美术。**

**依据：E04、E11 和具体处理库的官方文档。**

| 卡号 | 依赖 | 交付任务 | 验收条件 / CI |
|---|---|---|---|
| **AW-022** | 003、017 | Mesh/Skin/Skeleton 数据、绑定姿态、基础编辑 | 骨架无环、引用有效、权重合法；预设弯曲姿态能实际驱动网格。C1/C4 |
| **AW-023** | 022 | 动画曲线、FK/IK、重定向、触手程序化动作、基础接触约束 | 待机/移动/攻击可播放；循环与恢复姿态可检查；两角色共享时间轴有样例。C1/C4 |
| **AW-024** | 003、017 | 2D/像素、Sprite Sheet、UI/卡牌确定性排版 | 帧、锚点、透明度、文字不溢出；图像模型不负责最终文字准确性。C1/C4 |
| **AW-025** | 022、023 | 面部轨道、挂点、服装配件、蒙皮迁移基础能力 | Morph/眼睛/下颌可播放；刚性与蒙皮配件分开；不支持的服装结构可报告。C4 |
| **AW-026** | 023、024 | VFX 参数化、Flipbook、ActionPresentation 事件 | 蓄力/释放/恢复同步；取消动作时停止拖尾与声音；实际命中不伪装成固定时间事件。C1/C4 |
| **AW-027** | 003、017 | 本地音频处理：裁切、混音、格式、循环点 | 无非法样本、文件可解码；处理前后可试听；不统一强制所有音频同一响度。C1/C4 |
| **AW-028** | 022 | 静态建筑、模块化接口、植被变体、LOD/碰撞基础处理 | 建筑接缝按规格验证；植被允许合法薄片；优化前后差异与资源预算有报告。C4 |

这些任务不要求 V1 拥有通用雕刻器、全自动四边形重拓扑或影视级布料求解。需要专业外部工具时，记录显式依赖和交换方式，不能隐藏到“自动生成成功”背后。

## 12.7 M5：三个引擎分别交付

**责任：引擎集成工程。**

**依据：E07、E08、E09。**

| 卡号 | 依赖 | 交付任务 | 验收条件 / CI |
|---|---|---|---|
| **AW-029** | 017、022、023、024 | Unity 基础插件、资源导入与示例场景 | 静态、骨骼、动画、Sprite、音频进入实际工程；事件可观察。C4 |
| **AW-030** | 017、022、023、024 | Unreal 导入路线原型及正式桥接 | 先验证 Skeletal Mesh、动画、Morph 路径；不能默认 GLB 覆盖所有需求。C4 |
| **AW-031** | 017、022、023、024 | Godot 导入插件与资源映射 | 模型、动画库、Sprite、音频与事件在指定版本播放。C4 |

三张卡均须记录具体引擎版本、操作系统、格式与限制。后续 VFX、面部、服装能力由同一兼容矩阵增加，不另建矛盾列表。

## 12.8 M6：桌面、MCP、Skills 和用户体验

**责任：桌面工程、开放接口工程。**

**依据：E02、E05、E06。**

| 卡号 | 依赖 | 交付任务 | 验收条件 / CI |
|---|---|---|---|
| **AW-032** | 005、008、011、013、015、018、022、023、024 | 素材库、生成向导、预览、编辑、任务与费用界面 | 新手可完成参考图到资产任务；专家可编辑参数；无凭据时给出能力状态，不假成功。C3/C4 |
| **AW-033** | 008、013、014、016、017 | CLI、MCP stdio、公开接口契约与首批 SDK | 同一请求经不同入口得到相同业务结果；长任务返回可追踪 ID。C2 |
| **AW-034** | 018、033 | 首批 Skills、Agent 配置向导、模板与许可提示 | 外部 Agent 能检查能力、规划、执行、验证；不能绕过预算或任意执行 Shell。C2/C4 |

所有复杂流程仍由应用服务执行。Skill 不重复实现资产管理，MCP 不另写一套任务数据库。

## 12.9 集成验收与发布

**责任：质量、发布工程和模块负责人共同承担。**

| 卡号 | 依赖 | 交付任务 | 验收条件 / CI |
|---|---|---|---|
| **AW-035** | 007、016、020–031、032–034 | 固定测试集与完整混沌 Boss 集成 | 真实产物、动作、特效、声音、增量修改及三个引擎记录齐全；人工修改步骤如实披露。C4 |
| **AW-036** | 012、029–035 | 开源发行、安装器、签名、升级与文档 | Windows/macOS 干净安装可用；不强制本地大模型；更新和恢复不丢资产。C3/C5 |

完整混沌验收至少包含：

**模型、骨架、蒙皮、待机、移动、普通攻击、技能表现、受击/死亡表现、音效、台词/配音、可选 BGM、事件、验证报告和引擎资源。**

自动生成与人工修正应分别记录。**允许明确标注的人机协作，不允许把人工完成部分说成自动完成。**

## 12.10 可选接入与未来版本

| 卡号 | 依赖 | 任务 | 边界与验收 |
|---|---|---|---|
| **AW-037** | 032 | 可选登录与自愿统计 | 不登录可使用；统计与素材上传分开；不默认采集提示词、路径和资产内容 |
| **AW-038** | 014、015、018、033 | 用户 GitHub 批量执行模板 | 只运行批准模板；素材不进入公开仓库；明确 Runner 与 Provider 费用；不承诺免费 GPU |
| **AW-039** | 036 | 云端同步设计与首个实现 | 同步资产版本与对象；离线冲突保留；不复制 PG 数据目录 |
| **AW-040** | 039 | 市场、许可与积分领域设计 | 私有库、商品、购买权益、奖励账本分开；法律与支付审查先于收费上线 |

AW-037、038 不阻断本地核心 Alpha。AW-039、040 不得提前扩展 V1 主干结构。

---

# 十三、开发波次与任务分配

不按“GPT 负责架构、Claude 负责代码”分配责任。**按模块和验收责任分配，模型只是工具。**

| 责任角色 | 负责范围 |
|---|---|
| 架构负责人 | 范围、契约、证据、ADR、重大取舍 |
| 核心工程 | Rust、PG、版本、文件、任务与预算 |
| 生成集成工程 | API、协议、许可证、真实生成 |
| 图形/技术美术 | 骨架、变形、动作、VFX、视觉验收 |
| 桌面工程 | 界面、视口、交互、安装体验 |
| 引擎工程 | Unity、Unreal、Godot |
| 质量/发布工程 | CI、报告、回归、安装与恢复 |

小团队允许一人兼任，但同一高风险 PR 不应仅由其编写 Agent 自行宣布通过。

## 推进波次

| 波次 | 工作 | 离开条件 |
|---|---|---|
| **W0：建立基线** | 001、002、003、006 | 版本、契约、证据和候选可追踪 |
| **W1：验证三项风险** | 004、005、007 并行 | PG 打包、视口、真实生成链路都有可判定结果 |
| **W2：本地闭环** | 008–018、022，交付原型提前介入 | 导入、保存、任务、生成、校验可运行 |
| **W3：专业能力** | 019–028、033、034 | 动作、视听、2D/静态资产形成实际路径 |
| **W4：用户与引擎** | 029–032、035 | 不同入口和三引擎通过指定能力验收 |
| **W5：发行** | 036；可选 037、038 | 双平台安装、升级、恢复达到门槛 |
| **W6：云端** | 039、040 | 单独立项，不反向拖累本地版本 |

波次描述允许工作交叠；实际启动以任务卡依赖完成情况为准。

每位实现者最多同时承担一张主要实施卡；同一契约变更串行，不同模块可并行。任务卡不是时间承诺，进度根据产物与验收推进。

---

# 十四、PR 的完成定义

一个 PR 只需要提交六类信息：

| 项目 | 要求 |
|---|---|
| 任务 | 对应一个主要任务卡 |
| 范围 | 做了什么、明确没有做什么 |
| 契约 | 是否改变公开格式、数据迁移或外部接口 |
| 权威依据 | 新增外部 API 或技术事实对应哪个来源 |
| 运行证据 | 实际命令、提交、CI Run、报告和产物 |
| 未完成项 | `NOT_RUN`、阻塞原因、剩余风险 |

普通 Bug 修复不要求新 ADR。以下情况才需要：

**新增数据库大版本、改变资产格式、替换桌面技术、引入新必需运行时、改变付费执行或上传边界。**

禁止为了让 PR 变绿而：

- 无说明删除或弱化断言。
- 将真实后端换成 Stub 后仍沿用原验收结论。
- 自动更新视觉基准图而不审查差异。
- 将未测试引擎标记为支持。
- 用生成海报、README 宣传或模型文字回复作为运行证据。

---

# 十五、验收基准：先证明可用，再证明通用

建议第一批固定六组样本，而不是只选最好看的输出。

| 基准 | 主要验证 |
|---|---|
| 人形 NPC | 标准模型、绑定、生活动作、台词配音 |
| 简化混沌 | 非人形骨骼链、蒙皮、程序化攻击 |
| 完整混沌 | 参考图一致性、技能、视听同步、增量修改 |
| 像素角色 | 多方向、帧一致性、锚点与图集 |
| 消消乐效果 | 特效时序、透明边缘、取消与音效 |
| 建筑/植被套件 | 模块化、材质、LOD、碰撞与性能 |

每个实验保存：

`输入哈希、提示词、规格、模型/执行器版本、参数、原始输出、处理后输出、费用、验证报告、人工评价、引擎结果`。

**来源字段必须区分：**

`AI_GENERATED`、`PROCEDURAL`、`IMPORTED`、`MANUALLY_EDITED`、`TEST_FIXTURE`。

程序化生成本身是有效生产方式，但不能冒充模型推理证据。用户满意也不能覆盖损坏的文件或错误引用。

---

# 十六、开工状态与执行规则

当前可以形成的开发状态是：

| 内容 | 状态 |
|---|---|
| 产品范围、模块、任务编号 | 已形成本文基线 |
| 官方资料与部分源码文件核对 | 已完成基线报告中的核对，见证据编号；不等于源码全面审计 |
| 全部采用版本、Commit、许可证清单锁定 | AW-001/AW-006 执行 |
| GitHub Issue、分支、PR 创建 | 尚未执行 |
| PostgreSQL/视口原型 | 尚未执行 |
| 真实模型和三个引擎测试 | 尚未执行 |

**首个应推进的任务是 AW-001，而不是直接让 Agent 编写完整客户端。**

完成 AW-001 后，AW-002、003、006 可以并行；随后立即开展 AW-004、005、007。这样能尽早检验数据库部署、视口和真实生成三条关键路径，不会先投入大量平台代码，再发现核心产物无法交付。

后续每次开发遵循同一条链：

> **领取任务卡 → 核对指定权威来源与版本 → 实现限定范围 → 执行对应 CI → 提交真实证据 → 审查 → 更新任务与能力状态。**

这份基线的核心约束是：

**不让模型名称决定架构，不让架构复杂度代替生成质量，不让文档声明代替运行证据，不让“防幻觉”演变为无边界的治理工程。**
