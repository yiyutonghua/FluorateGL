# 实现 OpenGL 3.3 Core 完整性与 GLSL 处理改进 Spec

## Why

FluorateGL 报告 GL 3.3.0，但诊断日志显示 93 个函数通过 `eglGetProcAddress` 返回 null，其中 83 个真正未实现。部分函数（如 `glClearTexImage`、`glShaderStorageBlockBinding`、`glQueryCounter`）对应已声明的扩展（`GL_ARB_clear_texture`、`GL_ARB_shader_storage_buffer_object`、`GL_ARB_timer_query`），这种"声明扩展但不提供函数"的不一致会导致 LWJGL capabilities 字段为 null，调用时抛 "No context is current" 错误，是 Sodium 方块不渲染和 OptiFine 红屏的根因类型。

同时，FluorateGL 的 GLSL 翻译管线缺少 shader 缓存机制，每次翻译都走完整 SPIR-V 管线（shaderc + spirv-cross），性能开销大。参考 MobileGlues 的实现，可引入 SHA256 + LRU 缓存显著降低开销。

## What Changes

### 第一部分：修复扩展不一致（P0，9 个函数）

- 实现 6 个已声明扩展但未实现的函数 no-op stub：
  - `glClearTexImage` / `glClearTexSubImage`（`GL_ARB_clear_texture`）
  - `glQueryCounter` / `glGetQueryObjecti64v` / `glGetQueryObjectui64v`（`GL_ARB_timer_query`）
  - `glShaderStorageBlockBinding`（`GL_ARB_shader_storage_buffer_object`）
- 导出 5 个 ARB 后缀别名（core 版本已实现）：
  - `glBlendEquationiARB` / `glBlendEquationSeparateiARB` / `glBlendFunciARB` / `glBlendFuncSeparateiARB` / `glVertexAttribDivisorARB`

### 第二部分：GL 3.3 Core no-op stub（P1，13 个函数）

- 实现以下 GL 3.3 core 函数的 no-op 或简单转发 stub：
  - `glClampColor`（no-op，GLES 总是 clamp）
  - `glProvokingVertex`（no-op，GLES 固定 LAST_PERVERTEX）
  - `glBeginConditionalRender` / `glEndConditionalRender`（no-op）
  - `glGetFragDataIndex`（返回 -1，dual-source 不支持）
  - `glObjectLabel` / `glObjectLabelKHR`（no-op，调试标签）
  - `glProgramParameteri`（no-op，GL 4.1 程序参数）
  - `glGetActiveUniformName`（转发 `glGetActiveUniform` + 提取 name）
  - `glTexImage2DMultisample` / `glTexImage3DMultisample`（no-op 或转发 `glTexStorage2DMultisample`）
  - `glFramebufferTexture1D`（no-op，1D 不支持）
  - `glFramebufferTexture3D`（转发 `glFramebufferTextureLayer`）
  - `glPointParameteri` / `glPointParameteriv`（转发 `glPointParameterf`）

### 第三部分：VertexAttrib 类型转换 stub（P2，32 个函数）

- 批量实现 `glVertexAttrib*` 的 short/double/int/normalized 变体：
  - `glVertexAttrib{1,2,3,4}{s,d}` + `sv`/`dv`（16 个，转 float 调用 `*f` 版本）
  - `glVertexAttrib4{iv,bv,ubv,usv,uiv}`（5 个，转 float）
  - `glVertexAttrib4N{bv,sv,iv,ubv,usv,uiv}` + `glVertexAttrib4Nub`（7 个，normalized 转 float）
  - `glVertexAttribI4{bv,sv,ubv,usv}`（4 个，转 `I4i`/`I4ui`）
  - `glGetVertexAttribdv`（1 个，转 `glGetVertexAttribfv`）

### 第四部分：GLSL 翻译改进

- 引入 shader 翻译缓存（SHA256 key + LRU + 内存上限）
- 补充 `isamplerBuffer` → `isampler2D` 转换 polyfill（参考 MobileGlues）
- 补充 `textureQueryLod` polyfill（参考 MobileGlues）
- 修正 `gles_compile.rs` 中关于 MobileGlues 的注释错误

## Impact

- Affected code:
  - `src/gl/program.rs`（FragDataLocation、ProgramParameteri、ObjectLabel、ShaderStorageBlockBinding、GetActiveUniformName、GetFragDataIndex）
  - `src/gl/render_state.rs`（ARB 别名导出）
  - `src/gl/vertex_array.rs`（VertexAttribDivisorARB 别名、VertexAttrib 类型转换系列）
  - `src/gl/texture.rs`（ClearTexImage、TexImage*Multisample、FramebufferTexture1D/3D）
  - `src/gl/query.rs`（QueryCounter、GetQueryObjecti64v/ui64v）
  - `src/gl/framebuffer.rs`（已实现 glDrawBuffer，无需改动）
  - `src/gl/exports.rs`（glObjectLabel/KHR stub）
  - `src/gl/pixel.rs`（ClampColor、PointParameteri/iv）
  - `src/shader_translator/`（缓存机制、polyfill）
  - `src/state/mod.rs`（可能需要扩展缓存状态）
  - `Cargo.toml`（新增 `lru`、`sha2` crate）

## ADDED Requirements

### Requirement: 扩展一致性保证

系统 SHALL 确保所有在 `FAKE_EXTENSIONS` 中声明的扩展，其对应的 GL 函数 MUST 通过 `eglGetProcAddress` 返回有效指针（非 null）。

#### Scenario: 已声明扩展的函数查询

- **WHEN** LWJGL/MC 通过 `eglGetProcAddress` 查询已声明扩展对应的函数（如 `glClearTexImage`）
- **THEN** 系统返回有效的函数指针（stub 或真实实现），而非 null
- **AND** 调用该函数不会抛出 "No context is current" 错误

### Requirement: GL 3.3 Core 函数完整性

系统 SHALL 为所有 OpenGL 3.3 Core 规范定义的函数提供导出入口，对于 GLES 3.2 不支持的特性，SHALL 通过 no-op stub、类型转换或 CPU 模拟提供语义等价或安全降级的实现。

#### Scenario: VertexAttrib 类型转换

- **WHEN** 应用调用 `glVertexAttrib3s(index, x, y, z)`（short 版本）
- **THEN** 系统将 short 参数转换为 float，调用 GLES 的 `glVertexAttrib3f`
- **AND** 不产生 GL 错误

### Requirement: Shader 翻译缓存

系统 SHALL 缓存 GLSL 翻译结果，避免对相同源码重复执行 shaderc + spirv-cross 翻译流程。

#### Scenario: 缓存命中

- **WHEN** 应用提交与已缓存条目相同的 shader 源码（SHA256 匹配）
- **THEN** 系统直接返回缓存的 GLSL ES 源码
- **AND** 跳过 shaderc 编译和 spirv-cross 转译
- **AND** 输出 debug 日志记录缓存命中

#### Scenario: 缓存未命中

- **WHEN** 应用提交未缓存的 shader 源码
- **THEN** 系统执行完整翻译流程
- **AND** 将结果存入缓存（key = SHA256(source + stage + gles_version)）
- **AND** 缓存达到上限时按 LRU 策略淘汰

### Requirement: GLSL 特性 Polyfill

系统 SHALL 为 GLES 3.2 不支持但 MC/光影 mod 常用的 GLSL 特性提供软件 polyfill。

#### Scenario: isamplerBuffer 转换

- **WHEN** shader 源码包含 `isamplerBuffer` 声明
- **THEN** 系统将其转换为 `isampler2D` + 注入坐标映射辅助函数
- **AND** `texelFetch` 调用自动适配为 2D 坐标

## MODIFIED Requirements

### Requirement: eglGetProcAddress 诊断

现有 `warn_missing_gl_function` 诊断机制保留，但随着函数逐步实现，null 告警数量应减少。重新生成日志后，null 函数列表应用于验证实现完整性。

## REMOVED Requirements

### Requirement: double 精度 uniform 支持

**Reason**: GLES 3.2 不支持 double 精度，MC/Sodium/OptiFine 不使用 double 精度 shader uniform。18 个 `glProgramUniform*d/dv/Matrix*dv` 和 `glVertexAttribLFormat` 保持 null 不影响功能。
**Migration**: 若未来有 mod 需要 double 精度，可实现为转 float 降级 + 告警。
