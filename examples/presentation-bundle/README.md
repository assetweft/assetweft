# AW-035:B 离线表现包

在仓库根目录运行：

```sh
cargo build -p assetweft-cli
TMPDIR="$(realpath "${TMPDIR:-/tmp}")" ./examples/presentation-bundle/verify_presentation_bundle.sh
```

需要 Node.js 和 Python 3。可用 `ASSETWEFT_BIN` 指定已构建的 CLI。脚本不接收位置参数，正常退出码为 0，stdout 只输出一行 JSON。临时 GLB、PNG、WAV 和 JSON 随脚本退出清除。

- 五种战斗动作采样经 retarget 四阶段检查，双角色固定时钟对照检查接触与分离。
- Q6 在 `BYTE` 锁模式下测量挂点世界空间漂移。0.5 mm 须通过，0.6 mm 须以超限原因返回 `FAIL/1`。另对两个 GLB 内相同挂点网格的 primitive、accessor、buffer view 和引用字节计算 SHA-256，摘要须相等。
- 分别检查两类布料绑定、透明 flipbook、动作阶段绑定、Voice/SFX 溯源，以及 Unity、Unreal、Godot 的离线转换与纹理通道契约。取消后残留、GPL 进程内链接和数据纹理 sRGB 各须触发指定失败。
- Unity、Unreal、Godot 实时编辑器渲染、独立 TA 审核、可选 BGM 生成和 ASR/TTS 播放分别为 `NOT_RUN`。声音为合成 PCM 夹具，动作采样为固定时序，脸部表达在此刚性挂件对照中为 `INAPPLICABLE`。
