# 旧厂商扩展待办（legacy extensions，~712 个）

## 状态：待评估（记录于 2026-08-08）

## 是什么

MobileGlues 源码导出、但我们未导出的**旧厂商/旧扩展函数**（约 712 个），
如 `glAlphaFragmentOp1ATI`、`glActiveStencilFaceEXT`、`glAsyncMarkerSGIX` 等。
它们：
- 不在 glcorearb.h（Khronos GL core + 主流扩展头）签名集中
- 不在 MG gles.h GL_FUNC_TYPEDEF（GLES 函数表）中
- 不在 MG STUB/NATIVE 宏（gl_stub.cpp/gl_native.cpp）的解析结果中
- 多为 MG 各模块手写定义的旧扩展（GL 1.x-2.x 时代 / ATI-NV-EXT 厂商扩展）

## 与 fixed_pipeline.md 的区分

| | fixed_pipeline（715） | legacy_extensions（~712） |
|---|---|---|
| 性质 | 桌面 core 1.x-2.1 固定管线 | 旧厂商扩展（非 core） |
| 是否导出 | 有意不导出（北极星 core 无） | 未导出（无签名源，非主动决策） |
| 签名源 | 有（glcorearb.h 等） | 无（需第三轮签名源） |

## 为什么暂不处理

- **LWJGL 基本不查询**：这些是 GL 1.x-2.x 时代扩展，LWJGL 3 的 capabilities 加载
  针对 core + 常见 ARB/EXT 名，旧厂商扩展被查询概率极低
- **无签名源**：生成器（tools/gen_stub_exports.py）无法为它们生成签名正确的
  stub——无签名 stub 的 extern "C" 参数个数不匹配是 ABI 风险
- **边际价值低**：即使补齐，也只是「存在不崩溃」的 stub，无实际功能

## 如需补（第三轮签名源方案）

1. 解析 **Khronos OpenGL Registry 的 gl.xml**（官方注册表，含全部函数签名）
2. 或解析 MG 各模块手写定义（`gl/getter.cpp`/`buffer.cpp`/`texture.cpp` 等的
   `void glXxx(...)` 行首定义）——生成器已具备 parse_args 逻辑，补一个
   行首定义提取器即可
3. 生成方式与阶段 2/3 相同（stub_fn! 批量 + symbols 登记）

## 数量参考

- 上轮全量对比：MG 2741 vs 我们 1392 → 剩余 1424 = 固定管线 715 + 本项 ~712
- 精确清单可从生成器第三轮签名源提取后补充到本文件
