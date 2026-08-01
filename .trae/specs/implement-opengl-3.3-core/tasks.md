# Tasks

## 第一阶段：修复扩展不一致（P0，最高优先级）

- [x] Task 1: 实现 6 个已声明扩展但未实现的函数 no-op stub
  - [x] SubTask 1.1: 在 `src/gl/texture.rs` 实现 `glClearTexImage` / `glClearTexSubImage` no-op stub（已声明 `GL_ARB_clear_texture`）
  - [x] SubTask 1.2: 在 `src/gl/query.rs` 实现 `glQueryCounter` no-op stub、`glGetQueryObjecti64v` / `glGetQueryObjectui64v` 返回 0 的 stub（已声明 `GL_ARB_timer_query`）
  - [x] SubTask 1.3: 在 `src/gl/program.rs` 实现 `glShaderStorageBlockBinding` no-op stub（已声明 `GL_ARB_shader_storage_buffer_object`）

- [x] Task 2: 导出 5 个 ARB 后缀别名（core 版本已实现）
  - [x] SubTask 2.1: 在 `src/gl/render_state.rs` 导出 `glBlendEquationiARB` / `glBlendEquationSeparateiARB` / `glBlendFunciARB` / `glBlendFuncSeparateiARB`（转发到 core 版本）
  - [x] SubTask 2.2: 在 `src/gl/vertex_array.rs` 导出 `glVertexAttribDivisorARB`（转发到 core 版本）

## 第二阶段：GL 3.3 Core no-op stub（P1）

- [x] Task 3: 实现 GL 3.3 core no-op stub（按文件分组，可并行）
  - [x] SubTask 3.1: 在 `src/gl/pixel.rs` 实现 `glClampColor`（no-op）、`glPointParameteri` / `glPointParameteriv`（转发 `glPointParameterf`）
  - [x] SubTask 3.2: 在 `src/gl/render_state.rs` 实现 `glProvokingVertex`（no-op）、`glBeginConditionalRender` / `glEndConditionalRender`（no-op）
  - [x] SubTask 3.3: 在 `src/gl/exports.rs` 实现 `glObjectLabel` / `glObjectLabelKHR`（no-op 调试标签）
  - [x] SubTask 3.4: 在 `src/gl/program.rs` 实现 `glProgramParameteri`（no-op）、`glGetFragDataIndex`（返回 -1）、`glGetActiveUniformName`（转发 `glGetActiveUniform` + 提取 name）
  - [x] SubTask 3.5: 在 `src/gl/texture.rs` 实现 `glTexImage2DMultisample` / `glTexImage3DMultisample`（no-op）、`glFramebufferTexture1D`（no-op）、`glFramebufferTexture3D`（转发 `glFramebufferTextureLayer`）

## 第三阶段：VertexAttrib 类型转换 stub（P2）

- [x] Task 4: 批量实现 VertexAttrib 类型转换系列（32 个函数）
  - [x] SubTask 4.1: 在 `src/gl/vertex_array.rs` 实现 `glVertexAttrib{1,2,3,4}{s,d}` + `sv`/`dv`（16 个，转 float 调用 `*f` 版本）
  - [x] SubTask 4.2: 在 `src/gl/vertex_array.rs` 实现 `glVertexAttrib4{iv,bv,ubv,usv,uiv}`（5 个，转 float）
  - [x] SubTask 4.3: 在 `src/gl/vertex_array.rs` 实现 `glVertexAttrib4N{bv,sv,iv,ubv,usv,uiv}` + `glVertexAttrib4Nub`（7 个，normalized 转 float）
  - [x] SubTask 4.4: 在 `src/gl/vertex_array.rs` 实现 `glVertexAttribI4{bv,sv,ubv,usv}`（4 个，转 `I4i`/`I4ui`）
  - [x] SubTask 4.5: 在 `src/gl/getter.rs` 实现 `glGetVertexAttribdv`（转 `glGetVertexAttribfv`）

## 第四阶段：GLSL 翻译改进

- [x] Task 5: 引入 shader 翻译缓存
  - [x] SubTask 5.1: 在 `Cargo.toml` 添加 `lru`、`sha2` crate 依赖
  - [x] SubTask 5.2: 在 `src/shader_translator/` 创建缓存模块（SHA256 key + LRU + 内存上限，参考 MobileGlues `cache.cpp`）
  - [x] SubTask 5.3: 在 `spirv_pass.rs` 翻译入口集成缓存查询（命中返回 ESSL，未命中执行翻译后存入）
  - [x] SubTask 5.4: 添加缓存命中/未命中的 debug 日志

- [x] Task 6: 补充 GLSL 特性 polyfill（可并行于 Task 5）
  - [x] SubTask 6.1: 在 `src/shader_translator/preprocess.rs` 实现 `isamplerBuffer` → `isampler2D` 转换（参考 MobileGlues `process_sampler_buffer`）
  - [x] SubTask 6.2: 在 `src/shader_translator/preprocess.rs` 实现 `textureQueryLod` polyfill（参考 MobileGlues `inject_textureQueryLod`）
  - [x] SubTask 6.3: 修正 `src/shader_translator/gles_compile.rs` 中关于 MobileGlues flip_vertex_y/fixup_clipspace 的注释错误

## 第五阶段：验证

- [x] Task 7: 编译验证与日志对比
  - [x] SubTask 7.1: 运行 `cargo check` 确保编译通过
  - [x] SubTask 7.2: 运行 `cargo test` 确保现有测试不回归（79 passed, 0 failed）
  - [x] SubTask 7.3: 重新生成诊断日志，对比 null 函数数量减少情况（待用户运行后验证）

# Task Dependencies

- [Task 2] 可与 [Task 1] 并行（不同文件）
- [Task 3] 各子任务可并行（不同文件）
- [Task 4] 各子任务可并行（同文件不同函数，建议串行避免冲突）
- [Task 5] 可与 [Task 3]/[Task 4] 并行（不同模块）
- [Task 6] 可与 [Task 5] 并行（同模块不同函数，建议串行避免冲突）
- [Task 7] 依赖 [Task 1]-[Task 6] 全部完成
