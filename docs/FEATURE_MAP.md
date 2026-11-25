# AssetWeft Feature Map

现行代码真源映射（基线 `origin/main@e70a4a6`，Rust Workspace：`assetweft-core` + `assetweft-cli`）。每项严格记录当前已实现入口、命令/路径、源码位置与正常判据；未实现能力一律视为 `NOT_RUN`。

## 1. 本地库初始化与一致性备份恢复（SQLite Schema v6）
- **入口**：`assetweft library {init,backup,restore}`；核心 API `Application::{open,open_read_only,initialize_library,backup_library,restore_library}`
- **路径或子命令**：`library init --db <PATH>`（执行 `001..006` 迁移） · `library backup --db <PATH> --out <DIR>`（既有常规库文件） · `library restore --from <DIR> --dest <DIR>`（format 1/schema 6）；备份与恢复的目标目录须为新路径，使用核心备份限额
- **源码位置**：[crates/assetweft-cli/src/main.rs](../crates/assetweft-cli/src/main.rs) · [crates/assetweft-core/src/application.rs](../crates/assetweft-core/src/application.rs) · [crates/assetweft-core/src/library.rs](../crates/assetweft-core/src/library.rs) · [crates/assetweft-core/src/sqlite.rs](../crates/assetweft-core/src/sqlite.rs) · [crates/assetweft-core/src/catalog/backup_sqlite.rs](../crates/assetweft-core/src/catalog/backup_sqlite.rs)
- **正常判据**：stdout 单行 JSON `status:"PASS"`（`exit 0`）、`errors:[]`；重复 init 保数据；备份返回 `manifest` 和 `out_dir`，恢复返回 `manifest` 和 `dest_dir`，版本、对象可用性和反馈回读与源库一致；已有目录或损坏对象返回 `INVALID_INPUT/2`，未初始化库返回 `INPUT_INCOMPLETE/3`

## 2. Assurance：有界 GLB 结构检查与角色校验（含 `EXT_texture_webp`）
- **入口**：`assetweft inspect` / `assetweft validate`；核心 `assurance::checks::{inspect_geometry_*,validate_*,inspect_rig_*}`
- **路径或子命令**：`inspect <FILE.glb>` · `validate <FILE.glb> [--role static-mesh|rigged-character|animated-character]`（全局 `--max-bytes 1..=16777216`）
- **源码位置**：[crates/assetweft-cli/src/assurance.rs](../crates/assetweft-cli/src/assurance.rs) · [crates/assetweft-core/src/assurance/gltf_checks.rs](../crates/assetweft-core/src/assurance/gltf_checks.rs) · [crates/assetweft-core/src/assurance/checks.rs](../crates/assetweft-core/src/assurance/checks.rs) · [crates/assetweft-core/src/assurance/CONTRACT.md](../crates/assetweft-core/src/assurance/CONTRACT.md)
- **正常判据**：stdout 单行 JSON `status:"PASS"`（`exit 0`）；支持校验 `EXT_texture_webp` 声明与 WebP/PNG/JPEG 图像引用（不解码图像像素、不解析外部 URI），未执行检查保留于 `not_run`

## 13. AW-028:A 静态建筑和植被套件检查
- **入口**：`assetweft static-kit-inspect` · `assurance::checks::inspect_static_kit_file`
- **路径或子命令**：`static-kit-inspect <FILE.glb> --spec <SPEC.json>`，全局 `--max-bytes` 分别限制两个文件
- **源码位置**：[CLI](../crates/assetweft-cli/src/assurance.rs) · [静态套件领域](../crates/assetweft-core/src/assurance/static_kit_domain.rs) · [契约](../crates/assetweft-core/src/assurance/CONTRACT.md#aw-028a-static-kit-inspection)
- **正常判据**：单行 JSON `PASS/0`，闭合建筑满足比例与接缝合同，允许开放薄片的植被保留边界计数。报告记录优化前后计数及差值，视觉质量为 `NOT_RUN`

## AW-025:A 面部表情与刚性配件
- **入口**：`assetweft face-attachment-inspect` · `assurance::checks::inspect_face_attachment_file`
- **路径或子命令**：`face-attachment-inspect <FILE.glb> --spec <SPEC.json>`，全局 `--max-bytes` 分别限制两个文件
- **源码位置**：[CLI](../crates/assetweft-cli/src/assurance.rs) · [核心契约](../crates/assetweft-core/src/assurance/CONTRACT.md#aw-025a-face-and-attachment-inspection)
- **正常判据**：单行 JSON `PASS/0`。不需脸的角色回报 `INAPPLICABLE`，且仍检验刚性配件；有脸角色检查真实权重和眼颌轨道。视觉引擎为 `NOT_RUN`。

## AW-026:A 独立 VFX 与翻页图集检查
- **入口**：`assetweft vfx-inspect` · `assurance::checks::inspect_vfx_file`
- **路径或子命令**：`vfx-inspect <SPEC.json> [--flipbook <FILE.png>]`，全局 `--max-bytes` 分别限制两个文件
- **源码位置**：[CLI](../crates/assetweft-cli/src/assurance.rs) · [VFX 领域](../crates/assetweft-core/src/assurance/vfx_domain.rs) · [契约](../crates/assetweft-core/src/assurance/CONTRACT.md#aw-026a-independent-vfx-inspection)
- **正常判据**：`PASS/0`，报告时序、参数、编辑状态、可选图集和各自 SHA-256，`visual_engine_status=NOT_RUN`

## AW-027:A 本地 WAV 与循环边界检查
- **入口**：`assetweft audio-inspect` · `assurance::checks::inspect_audio_file`
- **路径或子命令**：`audio-inspect <FILE.wav> [--spec <SPEC.json>]`，全局 `--max-bytes` 分别限制两个文件
- **源码位置**：[CLI](../crates/assetweft-cli/src/assurance.rs) · [音频领域](../crates/assetweft-core/src/assurance/audio_domain.rs) · [契约](../crates/assetweft-core/src/assurance/CONTRACT.md#aw-027a-local-wav-inspection)
- **正常判据**：单行 JSON `PASS/0`，可解码文件满足所给声道、采样率、时长和循环约束。报告保留采样量测与原始文本，试听和 ASR 为 `NOT_RUN`

## 3. Assurance & Motion：Rigged Simple 骨骼弯曲、出招叠加与足部滑步检测
- **入口**：`assetweft motion-stance` / `motion-strike` / `rig-bend`；核心 `motion::*` / `assurance::rig_blender`
- **路径或子命令**：`motion-stance <JSON> --role grounded-humanoid|hovering` · `motion-strike <GLB> [--with-idle] --out <GLB>` · `rig-bend <GLB> --blender <BIN> --out <DIR>`
- **源码位置**：[crates/assetweft-cli/src/assurance.rs](../crates/assetweft-cli/src/assurance.rs) · [crates/assetweft-core/src/motion/stance.rs](../crates/assetweft-core/src/motion/stance.rs) · [crates/assetweft-core/src/motion.rs](../crates/assetweft-core/src/motion.rs) · [crates/assetweft-core/src/assurance/rig_blender.rs](../crates/assetweft-core/src/assurance/rig_blender.rs)
- **正常判据**：单行 JSON `status:"PASS"`（`exit 0`）。`motion-stance` 水平峰值位移按最近 1 nm 取整（`round_ties_even`）后 ≤0.01 m 为 `PASS/0`，回执保留未取整的 `max_slip_m` 和 `limit_m: 0.01`。超大世界坐标受 binary64 精度限制。`hovering` 为 `INAPPLICABLE/5`，缺证据为 `NOT_RUN/3`。

### AW-023:B IK 与重定向
- **入口**：`assetweft motion-retarget`，核心 `motion::retarget`。
- **路径或子命令**：`assetweft [--call-id ID] [--max-bytes N] motion-retarget SPEC.json`。
- **源码位置**：[CLI](../crates/assetweft-cli/src/assurance.rs) · [核心验证](../crates/assetweft-core/src/motion/retarget.rs)。
- **正常判据**：单行 JSON `PASS/0`，按顺序通过体型比例、IK 可达性、接触滑移、循环闭合。首个失败阶段为 `FAIL/1`，后续为 `NOT_RUN`。输入错误为 `INVALID_INPUT/2`，视觉引擎为 `NOT_RUN`。

## 4. Catalog：元数据、精确组件引用、CAS 对象、导出、预览与评价（7 表写主）
- **入口**：`assetweft catalog <action>`；核心 `catalog::*`（表：`aw_assets`, `aw_asset_versions`, `aw_component_references`, `aw_objects`, `aw_managed_versions`, `aw_candidate_current`, `aw_feedback`）
- **路径或子命令**：`catalog {commit-object,register-version,add-reference,version,search,file-availability,references,current,preview,materialize-export,evict-export,feedback-add,feedback-latest} --db <DB> ...` · `catalog feedback-get --db <DB> --asset-id <A> --version-id <V> --feedback-id <ID>`
- **源码位置**：[crates/assetweft-cli/src/catalog.rs](../crates/assetweft-cli/src/catalog.rs) · [crates/assetweft-core/src/catalog/sqlite_metadata.rs](../crates/assetweft-core/src/catalog/sqlite_metadata.rs) · [crates/assetweft-core/src/catalog/objects_sqlite.rs](../crates/assetweft-core/src/catalog/objects_sqlite.rs) · [crates/assetweft-core/src/catalog/exports.rs](../crates/assetweft-core/src/catalog/exports.rs) · [crates/assetweft-core/src/catalog/CONTRACT.md](../crates/assetweft-core/src/catalog/CONTRACT.md)
- **正常判据**：stdout 单行 JSON `status:"PASS"`（`exit 0`）；CAS 发布前校验字节长度与 SHA-256，PNG/WAV/GLB 预览与导出不覆写既有文件，评价与候选接受解耦。`feedback-get` 只读返回三 ID 精确匹配的记录或 `null`，可用于追加回执丢失后确认原 ID。

## 5. Production：增量规格校验、持久化工作流与候选接受（5 表写主）
- **入口**：`assetweft plan` / `workflow <action>` / `candidate <action>`；核心 `production::*`（表：`aw_workflow_runs`, `aw_workflow_steps`, `aw_workflow_dependencies`, `aw_candidates`, `aw_candidate_attempts`）
- **路径或子命令**：`plan --request <F> --trusted-context <F>` · `workflow {create,status,claim,finish,cancel}` · `candidate {initialize,propose,schedule-rigged-simple-motion,propose-rigged-simple-motion,attempt,show,receipt}`
- **源码位置**：[crates/assetweft-cli/src/production.rs](../crates/assetweft-cli/src/production.rs) · [crates/assetweft-core/src/production/specification.rs](../crates/assetweft-core/src/production/specification.rs) · [crates/assetweft-core/src/sqlite/acceptance.rs](../crates/assetweft-core/src/sqlite/acceptance.rs) · [crates/assetweft-core/src/production/CONTRACT.md](../crates/assetweft-core/src/production/CONTRACT.md)
- **正常判据**：`plan` 一致返回 `DECLARATION_CONSISTENT/0`（`execution:"NOT_RUN"`）；`workflow`/`candidate` 单行 JSON `PASS/0`，候选接受与 Catalog 头指针 CAS 在同一 `IMMEDIATE` 事务原子提交

## 6. Consumption：本地预算额度、调用意图与用量回执账本（3 表写主）
- **入口**：`assetweft budget <action>`；核心 `consumption::*`（表：`aw_budget_accounts`, `aw_budget_intents`, `aw_budget_receipts`）
- **路径或子命令**：`budget {approve,balance,reserve,intent,receipts,cancel} --db <DB> ...`
- **源码位置**：[crates/assetweft-cli/src/consumption.rs](../crates/assetweft-cli/src/consumption.rs) · [crates/assetweft-core/src/consumption/application.rs](../crates/assetweft-core/src/consumption/application.rs) · [crates/assetweft-core/src/consumption/sqlite.rs](../crates/assetweft-core/src/consumption/sqlite.rs) · [crates/assetweft-core/src/consumption/CONTRACT.md](../crates/assetweft-core/src/consumption/CONTRACT.md)
- **正常判据**：stdout 单行 JSON `status:"PASS"`（`exit 0`）；精确整数 `i64` 记账，未知报价保持 `null`，条件 `UPDATE` 原子防超支，取消不伪造远端退款或停机

## 7. Execution：BYO GPU 外部执行器与本地 Test Fixture 协议（0 表写主）
- **入口**：`assetweft byo-gpu <action>`；核心 `execution::{byo_gpu,application,runtime_application}`
- **路径或子命令**：`byo-gpu profiles` · `byo-gpu kaggle-validate --bundle <DIR>` · `byo-gpu run --db <DB> --budget-id <ID> --intent-id <ID> --profile hf-zerogpu|kaggle-cli --model <M> --out-dir <DIR> [--image <F>|--prompt <T>|--bundle <D>] [--adapter-bin <CMD>] [--allow-remote-upload]`
- **源码位置**：[crates/assetweft-cli/src/execution.rs](../crates/assetweft-cli/src/execution.rs) · [crates/assetweft-core/src/execution/byo_gpu/policy.rs](../crates/assetweft-core/src/execution/byo_gpu/policy.rs) · [crates/assetweft-core/src/execution/byo_gpu/process.rs](../crates/assetweft-core/src/execution/byo_gpu/process.rs) · [crates/assetweft-core/src/execution/CONTRACT.md](../crates/assetweft-core/src/execution/CONTRACT.md)
- **正常判据**：`profiles`/`kaggle-validate` 返回 `PASS/0`；`run` 经 Consumption 原子扣占 1 `Task`，上传本地文件须显式 `--allow-remote-upload`，成功返回 `SUCCEEDED` 与 `artifact.{file,sha256}`（`cash_cost:null`），已结算 intent 重放返回历史回执且 `invoked:false`

## 8. 合同校验与工程验证门禁（Contracts + C1 + Verification CLI）
- **入口**：`./tools/verify.sh` · `./tools/c1-stdout-contract.sh` · `node tools/contract_check.mjs`
- **路径或子命令**：`./tools/verify.sh --crate <pkg> [--test <name>]` · `./tools/verify.sh --contracts` · `./tools/verify.sh --full`
- **源码位置**：[tools/verify.sh](../tools/verify.sh) · [tools/c1-stdout-contract.sh](../tools/c1-stdout-contract.sh) · [tools/contract_check.mjs](../tools/contract_check.mjs) · [contracts/README.md](../contracts/README.md) · [contracts/media-compatibility.md](../contracts/media-compatibility.md)
- **正常判据**：退出码 `0`；`--full` 通过 fmt/clippy/test/build 且 C1 stdout 恰 1 行 `{"status":"PASS",...}`；`--contracts` 通过 5/5 invalid-manifest cases 单测与 14/14 fixture 期望匹配；失败仅输出压缩结论、关键失败行与完整日志路径

## 9. 引擎离线示例夹具（Godot Static / Rigged Simple）
- **入口**：本地 Godot / Blender 夹具验证脚本（非 CI 必跑，按需手工复跑）
- **路径或子命令**：见 [examples/godot-static/README.md](../examples/godot-static/README.md) 与 [examples/rigged-simple/README.md](../examples/rigged-simple/README.md)
- **源码位置**：[examples/godot-static/verify.gd](../examples/godot-static/verify.gd) · [examples/rigged-simple/godot_playback.gd](../examples/rigged-simple/godot_playback.gd) · [examples/rigged-simple/godot_binding_control.gd](../examples/rigged-simple/godot_binding_control.gd)
- **正常判据**：按夹具 README 退出码 `0` 且对照失败样例按预期报错；未执行的引擎导入、渲染与真实 F0 资产验收维持 `NOT_RUN`

## 12. AW-035:A 最小真实角色闭环

- **入口**：`assetweft rigged-simple-components <FILE> --out-dir <DIR>` 与 [verify_character_loop.sh](../examples/rigged-simple/verify_character_loop.sh)。
- **正常判据**：CLI 仅分解固定 Rigged Simple 原件或已验证的 idle+strike 衍生件；脚本经本地 CAS、workflow 和 candidate 接受一个 motion-only 修订。`EXT_texture_webp` 静态网格必须通过 static-mesh 而被 rigged-character 拒绝；未给 Blender/Godot 时明确为 `NOT_RUN`，本地 adapter 不代表真实或付费 GPU 执行。

## 10. AW-034:A/B Agent Skill 与 CLI 操作配方
- **入口**：[Skill](../skills/assetweft-cli/SKILL.md) · [离线评估](../skills/assetweft-cli/eval_recipe.sh) · [增量复用与恢复评估](../skills/assetweft-cli/eval_extended_recipe.sh)
- **正常判据**：构建后运行评估脚本，缺预算、额度不足、缺上传授权均拒绝且调用次数为零；成功路径核对单行 JSON、历史回执重放、SQLite 工作流及 CAS 字节。远端 GPU、付费 API、完整渲染与可选 MCP 适配器为 `NOT_RUN`。
- **扩展判据**：核验既有版本及 CAS 字节后直接验收，无生成调用；恢复库保留原版本与反馈并可追加反馈。未提交版本拒绝复用，损坏快照拒绝恢复且无目标清单。

## 11. AW-032:A/B 本地素材工作台
- **入口**：[构建与启动](../apps/workbench/README.md) · `assetweft catalog list --db <DB> [--limit <N>]`
- **正常判据**：无需配置数据库服务器或登录，能打开或初始化本地 SQLite，索引、受管文件和版本反馈与 Core 一致；GLB 只显示静态网格，支持 PNG/WAV 预览。高级编辑保持 `NOT_RUN`，云端同步与登录已禁用。
- **生成与预算**：Generation & Budgets 展示额度、用量与回执。未知金额保持 `null`，远端 UNKNOWN 禁盲重试，本地取消保留远端未决额度。BYO GPU 仅按显式操作执行，本地文件上传默认未授权，历史回执重放不调用执行器。
- **增量候选**：Incremental Candidates 展示候选及关联资产头，测试夹具采用 BYTE 锁和期望修订号。接受以 Core 事务为准，陈旧分支保留。生成、格式校验、版本反馈、结构接受各有独立结果。

## 14. AW-024:B UI、卡牌和地块模板检查
- **入口**：`assetweft card-template-check` · `assurance::checks::check_card_template_file`
- **路径或子命令**：`card-template-check <TEMPLATE.json> [--illustration <FILE.png>]`，全局 `--max-bytes` 分别限制两个文件
- **源码位置**：[CLI](../crates/assetweft-cli/src/assurance.rs) · [模板排版领域](../crates/assetweft-core/src/assurance/card_layout_domain.rs) · [契约](../crates/assetweft-core/src/assurance/CONTRACT.md#aw-024b-card-template-layout-check)
- **正常判据**：单行 JSON `PASS/0`，互换插画时布局摘要不变，PNG 摘要独立，视觉质量为 `NOT_RUN`。布局违规返回 `FAIL/1`，输入错误返回 `INVALID_INPUT/2`
