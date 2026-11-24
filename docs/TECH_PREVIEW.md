# AssetWeft 内部技术预览（AW-036:A）

## 交付范围

基线为 main 合入 AW-012:A 后的 609d8e3d，涵盖 AW-004:B、AW-012:A、AW-032:B、AW-033:B、AW-034:A、AW-035:A。

- 本地嵌入式 SQLite 和 CAS 对象库，共用 Rust 核心 assetweft-core。
- JSON CLI assetweft-cli 提供 library {init,backup,restore}（含 library backup 和 library restore）、inspect、validate、plan、motion-*、rig-bend、rigged-simple-components、workflow、budget、catalog、candidate、byo-gpu。
- 本地 Desktop Workbench（assetweft-workbench）提供素材目录和预览、生成与预算、增量候选。命令和正常判据见 [能力地图](FEATURE_MAP.md)。

预览运行时的架构边界为零 PostgreSQL 服务端依赖（sqlx/libpq）、零常驻本地守护进程、零必需 LLM 运行时、零必需 Python 运行时。Python 用于可选的 BYO Hugging Face/Kaggle 和 Blender 脚本，以及开发验证工具。

## 双平台与运行时边界

| 项目 | 已有证据与剩余边界 |
|---|---|
| #99 | Windows / Linux 原生 Workbench 视口实机截图在本地为 NOT_RUN。PR #160 的 E4 实机截图仅覆盖 macOS Workbench。GitHub windows-latest、macos-latest CI 覆盖构建和测试。#99 的独立视口实验仍待同提交原生截图对照。 |
| AW-031 / Godot | 验证主机缺少 Godot 4.7.2 二进制，ENGINE_GODOT=NOT_RUN。Blender 5.1.2 的离线验证覆盖 RiggedSimple 动作播放，并以 trellis2.glb 验证静态网格准入与角色拒绝。 |
| 付费云 GPU | 真实调用为 NOT_RUN。覆盖限于离线 mock、回执契约和 BYO Kaggle 校验器。 |
| 预览后续 | AW-032:C、AW-029 Unity 和 AW-030 Unreal 包与插件均为 NOT_STARTED。#142 的 8 项跨平台及许可不变量和 #28 的货币精度边界继续跟踪。 |

## 复验

- [tools/verify.sh](../tools/verify.sh) 的 --full 入口运行 C1，含预览依赖、源码启动入口和披露检查。
- [tools/c1-stdout-contract.sh](../tools/c1-stdout-contract.sh) 核对 C1 的单行 JSON 和退出码契约。
- [examples/rigged-simple/verify_character_loop.sh](../examples/rigged-simple/verify_character_loop.sh) 复跑离线角色闭环，按传入的引擎和外部资产执行对应检查。
