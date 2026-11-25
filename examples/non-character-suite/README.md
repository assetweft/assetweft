# AW-035:C 非角色与最终范围验证

在仓库根目录运行：

```sh
cargo build -p assetweft-cli
TMPDIR="$(realpath "${TMPDIR:-/tmp}")" ./examples/non-character-suite/verify_non_character_suite.sh
```

需要 Node.js、Python 3 和 SQLite CLI。可用 `ASSETWEFT_BIN` 指定已构建的 CLI。脚本不接收位置参数，正常退出码为 0，stdout 只输出一行 JSON。临时素材和本地书库随脚本退出清除。

| 基准 | 实际离线检查 |
| --- | --- |
| BM-01 | 静态 GLB 通过静态角色校验，并被骨骼角色校验拒绝 |
| BM-02 | Rigged Simple 角色校验、生成 idle + strike、分解派生动作组件 |
| BM-03 | 像素图集和锚点通过，循环越界被拒；UI、卡牌、地块换图时布局 SHA 相等、插图 SHA 不同；文本溢出被拒 |
| BM-04 | 封闭建筑网格与带透明遮罩的非封闭植被卡均通过，插口接缝超差被拒 |
| BM-05 | 使用同一 CLI 重跑表现包的战斗、音频与三个引擎契约检查 |
| BM-06 | 在规范化临时路径恢复 SQLite，校验数据库完整性、版本记录及托管对象原字节，拒绝已篡改快照 |

机械 GLB 用 `turret_base`、`turret_barrel` 和 `claw_l` 命名根、骨骼和端部，保留真实蒙皮与动画缓冲。另用机械名称运行 retarget 和双主体运动交互采样，验证这两个检查接受自定义名称。

V1 非目标为 `full_game_runtime_ai`、`commercial_asset_ripping`、`full_dcc_replacement`、`mandatory_cloud_account`。用户任务观察与实时引擎视口为 `NOT_RUN`，市场假设报告为 `SEPARATE_UNVALIDATED_NOT_RUN`。
