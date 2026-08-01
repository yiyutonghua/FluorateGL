# 优化 GL 实现方式与 GLSL 转译 Spec

## Why

FluorateGL 当前存在两类核心问题：
1. **GL 实现方式**：约 286 个函数手写 `backend::with_gles_dispatch(|dispatch| unsafe { ... })` 样板，`is_stub` 函数被复制 7 份，首次告警模式重复 21 次，program.rs 中 25+ 个函数静默 return 掩盖 bug，glGetError 直接透传导致 MC 可能误降级。
2. **GLSL 转译**：使用 Vulkan target 导致约 750 行 workaround 代码（UBO 往返、location 注入等），MobileGlues 用 OpenGL client 完全不需要这些。同时缺少 ResourceLimits 配置（循环限制默认 false 会拒绝带 while/do-while 的 mod shader）。

## What Changes

### 第一部分：GL 实现方式优化（P0-P1）

- 提取 `is_stub` 为 `GlesDispatch` 方法，消除 7 份重复
- 定义 `warn_once!` 声明宏，消除 21 个 static + 21 个 fn 样板
- 用声明宏生成纯转发函数（Uniform/VertexAttrib 系列），消除 100+ 样板函数
- 统一 ID 映射失败处理：消除 program.rs 中 25+ 个静默 return，改为首次告警 + 安全默认值
- 实现 glGetError 吞错策略（可配置，避免 MC 因 GLES 噪声误降级）

### 第二部分：GLSL 转译优化（P0-P1）

- **P0：添加 ResourceLimits 配置**（复刻 MobileGlues InitResources，启用循环限制）
- **P1：切换到 OpenGL target**（shaderc 支持 `TargetEnv::OpenGL`），删除约 750 行 workaround 代码
  - 删除 `convert_uniforms_to_ubo`、`unwrap_generated_ubo`、`strip_ubo_instance_name`
  - 删除 `inject_missing_locations`、`inject_missing_bindings`、`strip_varying_locations`、`strip_uniform_locations`
  - 删除 `rename_vulkan_builtin_variables`
  - 启用 `set_auto_map_locations(true)` + `set_auto_map_bindings(true)`
  - 版本改为 `#version 150 compatibility`（对齐 MobileGlues，支持 legacy 语法）
- 补充 GL_ARB_derivative_control 强制处理

### 第三部分：性能优化（P2）

- shader 缓存磁盘持久化（参考 MobileGlues cache.cpp 的 load/save）
- multidraw func_ptr 惰性缓存（首次后零分支）
- indirect buffer 复用 + 指数增长
- 热路径 debug 日志用 `log_enabled!` 包裹

## Impact

- Affected code:
  - `src/backend/dispatch.rs`（is_stub 方法、统一 stub 函数）
  - `src/gl/mod.rs` 或新建 `src/gl/macros.rs`（warn_once! / forward! 宏）
  - `src/gl/program.rs`（消除静默 return、Uniform 宏生成）
  - `src/gl/vertex_array.rs`（VertexAttrib 宏生成）
  - `src/gl/buffer.rs`、`texture.rs`、`render_state.rs` 等（is_stub 统一）
  - `src/gl/getter.rs`（glGetError 吞错）
  - `src/shader_translator/spirv_compile.rs`（OpenGL target + ResourceLimits）
  - `src/shader_translator/preprocess.rs`（删除 workaround、简化版本处理）
  - `src/shader_translator/postprocess.rs`（删除 UBO 相关后处理）
  - `src/shader_translator/cache.rs`（磁盘持久化）
  - `src/gl/multi_draw.rs`（func_ptr 缓存、buffer 复用）
  - `src/config.rs`（glGetError 吞错开关）
  - 测试文件（删除 UBO workaround 相关测试，新增 OpenGL target 测试）

## ADDED Requirements

### Requirement: GL 函数样板消除

系统 SHALL 通过声明宏和方法提取消除重复样板代码，同时保持功能不变。

#### Scenario: 纯转发函数生成

- **WHEN** 需要实现纯转发 GL 函数（如 `glUniform1f`）
- **THEN** 使用 `forward!` 宏一行声明，自动生成 `#[unsafe(no_mangle)]` + dispatch 调用
- **AND** 生成的代码与手写版本功能完全一致

#### Scenario: 首次告警统一

- **WHEN** ID 映射失败需要告警
- **THEN** 使用 `warn_once!` 宏，自动管理 AtomicBool + swap + log::warn
- **AND** 同一类型告警只输出一次

### Requirement: 错误处理一致性

系统 SHALL 统一所有 GL 函数的 ID 映射失败处理策略，消除静默 return。

#### Scenario: Program ID 映射失败

- **WHEN** program.rs 中任何函数遇到 program ID 未在 IdMap 中找到
- **THEN** 输出首次告警日志
- **AND** 写操作跳过执行，查询操作写入安全默认值（0 或 GL_FALSE）

### Requirement: glGetError 吞错

系统 SHALL 提供可配置的 glGetError 吞错策略，避免 GLES 噪声错误导致 MC 误降级。

#### Scenario: 吞错模式启用

- **WHEN** 配置启用 glGetError 吞错（默认启用）
- **THEN** glGetError 永远返回 GL_NO_ERROR
- **AND** debug 模式下记录被吞掉的错误到日志

### Requirement: ResourceLimits 配置

系统 SHALL 显式配置 glslang 的 ResourceLimits，启用循环限制和合理的资源上限。

#### Scenario: 带 while 循环的 shader

- **WHEN** shader 源码包含 `while` 或 `do-while` 循环
- **THEN** glslang 接受编译（循环限制为 true）
- **AND** 不因资源限制拒绝合法 shader

### Requirement: OpenGL Target 转译

系统 SHALL 使用 OpenGL client target 编译 GLSL→SPIR-V，避免 Vulkan target 的 UBO 往返复杂性。

#### Scenario: Standalone uniform 编译

- **WHEN** shader 包含 `uniform mat4 ModelViewMat;`（standalone uniform）
- **THEN** glslang 接受编译（无需 UBO 包装）
- **AND** spirv-cross 输出保持 standalone uniform
- **AND** MC 的 glGetUniformLocation 可按名查询

#### Scenario: 自动 location 映射

- **WHEN** shader 的 in/out 变量缺少 `layout(location=N)`
- **THEN** glslang 自动分配 location
- **AND** 无需 preprocess 手动注入

## MODIFIED Requirements

### Requirement: GLSL 预处理

预处理流程简化为：移除 #line、移除 MC 版本注释、版本规范化（150 compatibility）、undef VULKAN、特性 polyfill（textureQueryLod/isamplerBuffer）。删除 UBO 转换、location/binding 注入、内置变量重命名。

## REMOVED Requirements

### Requirement: Vulkan Target UBO 往返处理

**Reason**: 切换到 OpenGL target 后，standalone uniform 可直接编译，无需 UBO 包装→拆解往返。
**Migration**: 删除 `convert_uniforms_to_ubo`、`unwrap_generated_ubo`、`strip_ubo_instance_name`、`inject_missing_locations`、`inject_missing_bindings`、`strip_varying_locations`、`strip_uniform_locations`、`rename_vulkan_builtin_variables`。

### Requirement: Core Profile 强制

**Reason**: 切换到 compatibility profile 支持legacy 语法（gl_FragColor、varying、attribute），提升老 shader 兼容性。
**Migration**: `force_glsl_version` 改为升级到 `#version 150 compatibility`。
