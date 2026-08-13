# FluorateGL 待办清单（TODO）

本目录记录暂不实现/搁置的待办事项。每个主题一个文件，含背景、现状与后续评估方向。

## 当前待办总览

| 主题 | 状态 | 文件 |
|---|---|---|
| 固定管线函数（~715） | 待办（暂不实现） | [fixed_pipeline.md](fixed_pipeline.md) |
| ANGLE 后端问题（真机） | 搁置中（m00228 暂放） | [angle.md](angle.md) |
| 旧厂商扩展（~712） | 待评估（无签名源） | [legacy_extensions.md](legacy_extensions.md) |

## 各主题要点

- **固定管线（715）**：glBegin/glMatrixMode/glColor3f 等桌面 1.x-2.1 函数；北极星=GL 3.3 core 无固定管线、GLES 无对应；完整清单见 fixed_pipeline.md
- **ANGLE（真机）**：491 行日志 3 秒崩（UBO 对齐除零、glGetString null、26 个 GLES 函数 missing）；五层链路候选①EGL context 与函数表不匹配②库名（已排除）③版本不配对；盲区=loader 无 dlerror/dladdr、egl 导出无调用日志；恢复步骤见 angle.md
- **旧扩展（712）**：MG 有我们无、无签名源的旧厂商扩展（glAlphaFragmentOp1ATI 等）；LWJGL 基本不查询；需补则第三轮签名源（Khronos gl.xml 或 MG 手写定义解析）

## 约定

- 新待办：每主题一个 md 文件，头部注明状态与记录日期
- 已完成事项移出本目录（或标注完成 commit）
