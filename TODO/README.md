# FluorateGL 待办清单（TODO）

本目录记录暂不实现/搁置的待办事项。每个主题一个文件，含背景、现状与后续评估方向。

## 当前待办总览

| 主题 | 状态 | 文件 | 说明 |
|---|---|---|---|
| 固定管线函数（~715） | 待办（暂不实现） | [fixed_pipeline.md](fixed_pipeline.md) | glBegin/glMatrixMode/glColor3f 等桌面 1.x-2.1 固定管线函数；北极星=GL 3.3 core 无固定管线，GLES 无对应；若需兼容旧应用再评估 |
| ANGLE 后端问题（真机） | 搁置中 | — | 真机 ANGLE 后端 GLES 函数表缺失（capabilities 检测 version=0，491 行日志待精读）；EGL 导出调用日志增强待办（loader dlerror/路径记录、egl 导出调用日志） |
| 旧厂商扩展（~712） | 待评估 | — | MG 有我们无、无签名源的旧扩展（glAlphaFragmentOp1ATI 等）；LWJGL 基本不查询，边际价值低；如需可补第三轮签名源（MG 手写定义解析） |
| 安全可声明扩展 | 待决策 | — | 查询/微调类 stub 无副作用，可考虑入 FAKE_EXTENSIONS：GL_ARB_internalformat_query2、GL_KHR_no_error、GL_ARB_texture_filter_anisotropic、GL_ARB_separate_shader_objects |
| 行为依赖扩展（不建议声明） | 已定（不声明） | — | buffer_storage（Adreno 事故教训）、compute_shader/image_load_store、DSA、draw_instanced 系、transform_feedback 系——stub 会导致应用走错误路径 |

## 约定

- 新待办：每主题一个 md 文件，头部注明状态与记录日期
- 已完成事项移出本目录（或标注完成 commit）
