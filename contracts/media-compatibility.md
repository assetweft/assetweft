# AW-003:B 媒体交付合同与兼容样例

范围为 AW024:A → AW031:C 图集、AW027:A 可编辑声音，以及 AW031:D 本地 Godot 表现包。格式取自这些交付及其读取器。示例使用既有本地交付的副本，实际媒体、读取器及运行回执随各卡的本地证据保存。产品入口复跑基线为 `cd15aae68ebab31f56b8cfbf56616eff07a51a0b`。

## 版本与共用入口

- AW-CONTRACT-001 继续使用 `first-delivery-contract.schema.json` 的 `schema_version: "1.0.0"`。例 `valid/non_fixture_shape` 的旧文档通过，两种单独改动都返回逐例 `INVALID_INPUT`：将 `schema_version` 改成 `"2.0.0"`，或者增加顶层 `media`。执行入口为 `node tools/contract_check.mjs`。样例的 gate 退出码表示实际结果与清单预期是否一致。
- Catalog 继续使用 `contract_version: "1.0.0"` 的版本描述。用 `catalog commit-object` 登记精确文件字节，经 `catalog materialize-export --version-id ID` 取回并比较 SHA-256。媒体 JSON 作为独立字节对象保存。把体裁字段加入 Catalog 版本描述会在解析时报 `INVALID_INPUT`（退出 2），把描述版本改成 `"2.0.0"` 会在提交时报 `FAIL / UnknownValue`（退出 1）。图集、声音与 cue 的内部字段由各自读取器解释。[Catalog CLI](../crates/assetweft-cli/README.md)
- 字节、资产版本 ID、格式版本分别管理。改媒体字节时提交新版本、更新引用或交付清单上的哈希，并保留原版本。新增注解须可由相应读取器忽略。凡改变必需字段或字段解释的交付，须由明确支持该格式的读取器接收。具体兼容证据见下文。
- 公共引用使用不含凭据和路径遍历的项目相对标识。AW031:D 清单里的本地证据引用随本地审阅材料保存。发布前按权限范围选取材料。

## 2D 图集 · PNG 与 atlas.json

PNG 可以独立导出和预览。做动画交付时随 PNG 提供 `atlas.json`，AW024:A 的格式号为整数 `schema_version: 1`。实际播放读取 `image.{file,sha256,size_px}`、`clips[].{id,frame_duration_ms,loop,frames}` 和 `frames[].{rect_px,anchor_px}`。帧按列表排序。矩形使用左上原点的整数像素 `[x,y,width,height]`，对象的绘制原点为目标位置减去帧内锚点。该样例的 `image.format`、`coordinates` 与来源描述用于声明；方向与动画语义 `UNKNOWN`。

| 兼容输入 | 实际结果 |
| --- | --- |
| 旧 PNG 单独提交并调用 `catalog preview` | 退出 0，解码的 RGBA 与原 PNG 一致。仅有 `image` 的 sidecar 交 AW024:A 读回时因缺字段退出 1。 |
| 原 PNG 与 atlas v1 | AW024:A `readback.py ORIGINAL_PNG ATLAS_JSON` 输出 32 次整画布匹配并写出两轮拼图。AW031:C `godot --headless --path PROJECT --script play.gd -- PACKAGE_ZIP original` 输出 68 次 Sprite2D 帧、锚点、循环采样。 |
| atlas v1 新增顶层 `display_note`，或将 `schema_version` 改成 2 | AW024:A 读回对注解输出相同结果，对未知版本因断言退出 1。AW031:C 的固定包读取器对修改后的 ZIP 在入口报 `INPUT_PIN_MISMATCH`（退出 2）。 |
| 固定 ZIP 用 AW031:C 的 `swap` 或 `anchor` 对照运行 | 首帧分别报 `TEXTURE_MISMATCH` 和 `ANCHOR_MISMATCH`（退出 2）。 |

## 本地声音 · WAV 与 cue JSON

编辑交付提供原始和变更后两份 PCM16 WAV 及对应 cue。以 `catalog add-reference` 建立声音版本到 cue 版本的 `dialogue` 引用，用 `catalog references --version-id ID` 查询。cue 中用 `audio_version_id` 与 `audio_sha256` 绑定精确音频，另有 `text_version_id`、文本与语言、格式、采样率、声道、时长和 `loop`。循环以样本帧计，左闭右开。音频内容与文本分别换版本。这里的 v1/v2 是样本提交标识，cue JSON 本身没有格式版本分派。

| 兼容输入 | 实际结果 |
| --- | --- |
| 同一音频资产的 v1 原始立体声（3 s，`loop: null`）和 v2 编辑单声道（1 s，`loop: {start_frame_inclusive: 1600, end_frame_exclusive: 14400}`） | 两组精确边可查询，四份对象可按版本原样导出。旧播放路径用 FFmpeg 打开两份 WAV，分别获得 48,000 和 16,000 帧。AW027:A `verify.py` 同时读取两份 cue，核对 ID、哈希及测量值，并比对 v2 的三次循环。 |
| v2 cue 新增 `display_note`，或单独改成未知 `audio_version_id` / 删 `audio_sha256` | `verify.py` 对注解输出相同结果。后两例各退出 1。WAV 解码器直接消费文件采样，cue 校验发生在成对读取时。 |
| 只把循环末帧从 14400 移到 14409，重新采样并三次拼接 | AW027:A `verify.py boundary-control` 退出 1，报 `LOOP_DISCONTINUITY`。恢复原循环时两处接缝均为 +1061 PCM 计数，与源文件的自然延续一致。 |

## 表现包 · ExportManifest 与 Godot 入口

AW031:D 通过现有 `project.godot → main.tscn → presentation.gd` 启动。完整导出清单应记录实际文件及 SHA-256、依赖、引擎版本、能力与限制。给工具或用户的轨道、挂点、材料、降级和授权说明仍随清单交付。

当前入口实际比对 `files` 中八个固定相对媒体路径的哈希。面部读取 GLB 与 profile，动作绑定读取 GLB，特效读取既有 recipe 的 representation、sheet、sheet SHA、帧序和间隔、pivot、opacity。音频加载 WAV，核对 PCM16 单声道、采样率、帧数、精确哈希和禁用循环，另将 cue 版本与源循环点写入回执。v2 cue 的循环对象须可读。动作时间事件来自绑定的动画时钟，接触来自运行时碰撞；清单中的事件与能力描述供审阅。

| 兼容输入 | 实际结果 |
| --- | --- |
| 当前完整本地 v2 清单，经 `godot --headless --path PROJECT -- verify CAPTURE_PCM` 启动 | 退出 0。原入口观测运行时接触、依次使用 8 个 VFX 帧、解出的 WAV 采样哈希匹配，并写出非静默软件混音。 |
| 同一媒体保留且清单只含 `files` 的兼容对照，或完整清单改成未知 `schema` 字符串并附加 `display_note` | 两者均退出 0，与原入口的 runtime 字段一致。精简对照表示入口实际依赖的子集；完整交付仍须提供上述声明。`schema` 标签通过此入口没有版本准入含义。 |
| 删除 `files["media/edited.wav"]` | 入口退出 2，报 `MEDIA_SHA_MISMATCH:media/edited.wav`。 |
| 同一路径换入 AW027:A 的旧立体声 WAV，并同步清单哈希，原 v2 cue 保持 | 进入媒体读取阶段后退出 2，报 `AUDIO_CONTRACT_MISMATCH`。旧 WAV 可独立播放，本表现绑定要求对应单声道版本。 |

范围为本地 Godot 4.7.2 headless 与 Dummy 软件混音。AW031:D 原 viewport readback 失败保留，渲染像素、设备试听、Godot 循环和其他引擎为 `NOT_RUN`。原生粒子为 baked 交付范围外。AW-003:A 的 F0、真实生成与语义规范化检查仍遵循各自的 `UNKNOWN` / `NOT_RUN` 状态。CI 的 AW-003:A fixtures 仅验证合同文档和预期匹配。
