# AssetWeft Feature Map

现行代码真源映射（基线 `origin/main@e70a4a6`，Rust Workspace：`assetweft-core` + `assetweft-cli`）。每项严格记录当前已实现入口、命令/路径、源码位置与正常判据；未实现能力一律视为 `NOT_RUN`。

## 1. 本地库初始化与一致性备份恢复（SQLite Schema v6）
- **入口**：`assetweft library init`；核心 API `Application::{open,open_read_only,initialize_library,backup_library,restore_library}`
- **路径或子命令**：`library init --db <PATH>`（执行 `001..006` 迁移；其余库命令仅接受既有常规文件，不隐式建库）
- **源码位置**：[crates/assetweft-cli/src/main.rs](../crates/assetweft-cli/src/main.rs) · [crates/assetweft-core/src/application.rs](../crates/assetweft-core/src/application.rs) · [crates/assetweft-core/src/library.rs](../crates/assetweft-core/src/library.rs) · [crates/assetweft-core/src/sqlite.rs](../crates/assetweft-core/src/sqlite.rs) · [crates/assetweft-core/src/catalog/backup_sqlite.rs](../crates/assetweft-core/src/catalog/backup_sqlite.rs)
- **正常判据**：stdout 单行 JSON `status:"PASS"`（`exit 0`）、stderr 0 error；重复 init 幂等保数据，未初始化库被查询返回 `INPUT_INCOMPLETE`（`exit 3`）

## 2. Assurance：有界 GLB 结构检查与角色校验（含 `EXT_texture_webp`）
- **入口**：`assetweft inspect` / `assetweft validate`；核心 `assurance::checks::{inspect_geometry_*,validate_*,inspect_rig_*}`
- **路径或子命令**：`inspect <FILE.glb>` · `validate <FILE.glb> [--role static-mesh|rigged-character|animated-character]`（全局 `--max-bytes 1..=16777216`）
- **源码位置**：[crates/assetweft-cli/src/assurance.rs](../crates/assetweft-cli/src/assurance.rs) · [crates/assetweft-core/src/assurance/gltf_checks.rs](../crates/assetweft-core/src/assurance/gltf_checks.rs) · [crates/assetweft-core/src/assurance/checks.rs](../crates/assetweft-core/src/assurance/checks.rs) · [crates/assetweft-core/src/assurance/CONTRACT.md](../crates/assetweft-core/src/assurance/CONTRACT.md)
- **正常判据**：stdout 单行 JSON `status:"PASS"`（`exit 0`）；支持校验 `EXT_texture_webp` 声明与 WebP/PNG/JPEG 图像引用（不解码图像像素、不解析外部 URI），未执行检查保留于 `not_run`

## 3. Assurance & Motion：Rigged Simple 骨骼弯曲、出招叠加与足部滑步检测
- **入口**：`assetweft motion-stance` / `motion-strike` / `rig-bend`；核心 `motion::*` / `assurance::rig_blender`
- **路径或子命令**：`motion-stance <JSON> --role grounded-humanoid|hovering` · `motion-strike <GLB> [--with-idle] --out <GLB>` · `rig-bend <GLB> --blender <BIN> --out <DIR>`
- **源码位置**：[crates/assetweft-cli/src/assurance.rs](../crates/assetweft-cli/src/assurance.rs) · [crates/assetweft-core/src/motion/stance.rs](../crates/assetweft-core/src/motion/stance.rs) · [crates/assetweft-core/src/motion.rs](../crates/assetweft-core/src/motion.rs) · [crates/assetweft-core/src/assurance/rig_blender.rs](../crates/assetweft-core/src/assurance/rig_blender.rs)
- **正常判据**：单行 JSON `status:"PASS"`（`exit 0`）；`motion-stance` 水平滑步 ≤0.01 m 为 `PASS/0`、`hovering` 为 `INAPPLICABLE/5`、缺证据为 `NOT_RUN/3`

## 4. Catalog：元数据、精确组件引用、CAS 对象、导出、预览与评价（7 表写主）
- **入口**：`assetweft catalog <action>`；核心 `catalog::*`（表：`aw_assets`, `aw_asset_versions`, `aw_component_references`, `aw_objects`, `aw_managed_versions`, `aw_candidate_current`, `aw_feedback`）
- **路径或子命令**：`catalog {commit-object,register-version,add-reference,version,search,file-availability,references,current,preview,materialize-export,evict-export,feedback-add,feedback-latest} --db <DB> ...`
- **源码位置**：[crates/assetweft-cli/src/catalog.rs](../crates/assetweft-cli/src/catalog.rs) · [crates/assetweft-core/src/catalog/sqlite_metadata.rs](../crates/assetweft-core/src/catalog/sqlite_metadata.rs) · [crates/assetweft-core/src/catalog/objects_sqlite.rs](../crates/assetweft-core/src/catalog/objects_sqlite.rs) · [crates/assetweft-core/src/catalog/exports.rs](../crates/assetweft-core/src/catalog/exports.rs) · [crates/assetweft-core/src/catalog/CONTRACT.md](../crates/assetweft-core/src/catalog/CONTRACT.md)
- **正常判据**：stdout 单行 JSON `status:"PASS"`（`exit 0`）；CAS 发布前校验字节长度与 SHA-256，PNG/WAV/GLB 预览与导出不覆写既有文件，评价与候选接受解耦

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

## 10. AW-034:A Agent Skill 与 CLI 操作配方
- **入口**：[Skill](../skills/assetweft-cli/SKILL.md) · [离线评估](../skills/assetweft-cli/eval_recipe.sh)
- **正常判据**：构建后运行评估脚本，缺预算、额度不足、缺上传授权均拒绝且调用次数为零；成功路径核对单行 JSON、历史回执重放、SQLite 工作流及 CAS 字节。远端 GPU、付费 API、完整渲染与可选 MCP 适配器为 `NOT_RUN`。
