# Checklist

## 第一阶段：扩展一致性（P0）

- [x] `glClearTexImage` / `glClearTexSubImage` 已实现为 no-op stub，`eglGetProcAddress` 查询不再返回 null
- [x] `glQueryCounter` 已实现为 no-op stub
- [x] `glGetQueryObjecti64v` / `glGetQueryObjectui64v` 已实现为返回 0 的 stub
- [x] `glShaderStorageBlockBinding` 已实现为 no-op stub
- [x] `glBlendEquationiARB` 等 5 个 ARB 后缀别名已导出，转发到 core 版本
- [x] 所有 `FAKE_EXTENSIONS` 中声明的扩展，其对应函数通过 `eglGetProcAddress` 返回有效指针

## 第二阶段：GL 3.3 Core 完整性（P1）

- [x] `glClampColor` 已实现为 no-op stub
- [x] `glProvokingVertex` 已实现为 no-op stub
- [x] `glBeginConditionalRender` / `glEndConditionalRender` 已实现为 no-op stub
- [x] `glGetFragDataIndex` 已实现为返回 -1 的 stub
- [x] `glObjectLabel` / `glObjectLabelKHR` 已实现为 no-op stub
- [x] `glProgramParameteri` 已实现为 no-op stub
- [x] `glGetActiveUniformName` 已实现为转发 `glGetActiveUniform` + 提取 name
- [x] `glTexImage2DMultisample` / `glTexImage3DMultisample` 已实现为 no-op stub
- [x] `glFramebufferTexture1D` 已实现为 no-op stub
- [x] `glFramebufferTexture3D` 已实现为转发 `glFramebufferTextureLayer`
- [x] `glPointParameteri` / `glPointParameteriv` 已实现为转发 `glPointParameterf`

## 第三阶段：VertexAttrib 类型转换（P2）

- [x] `glVertexAttrib{1,2,3,4}{s,d}` + `sv`/`dv`（16 个）已实现，转 float 调用 `*f` 版本
- [x] `glVertexAttrib4{iv,bv,ubv,usv,uiv}`（5 个）已实现，转 float
- [x] `glVertexAttrib4N{bv,sv,iv,ubv,usv,uiv}` + `glVertexAttrib4Nub`（7 个）已实现，normalized 转 float
- [x] `glVertexAttribI4{bv,sv,ubv,usv}`（4 个）已实现，转 `I4i`/`I4ui`
- [x] `glGetVertexAttribdv` 已实现，转 `glGetVertexAttribfv`

## 第四阶段：GLSL 翻译改进

- [x] `Cargo.toml` 已添加 `lru`、`sha2` crate 依赖
- [x] shader 翻译缓存模块已创建，支持 SHA256 key + LRU + 内存上限
- [x] 翻译入口已集成缓存查询，命中时跳过 shaderc + spirv-cross
- [x] 缓存命中/未命中有 debug 日志
- [x] `isamplerBuffer` → `isampler2D` 转换 polyfill 已实现
- [x] `textureQueryLod` polyfill 已实现
- [x] `gles_compile.rs` 中关于 MobileGlues 的注释错误已修正

## 第五阶段：验证

- [x] `cargo check` 编译通过
- [x] `cargo test` 现有测试不回归（79 passed, 0 failed）
- [ ] 重新生成的诊断日志中 null 函数数量显著减少（预期从 83 减少到 ≤18，剩余为 double 精度系列）
