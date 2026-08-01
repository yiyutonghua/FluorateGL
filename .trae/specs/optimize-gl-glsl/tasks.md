# Tasks

## 第一阶段：GL 实现方式优化（P0，最高优先级）

- [ ] Task 1: 提取 is_stub 为 GlesDispatch 方法
  - [ ] SubTask 1.1: 在 `src/backend/dispatch.rs` 为 GlesDispatch 添加 `pub fn is_stub(&self, f: *const ()) -> bool` 方法
  - [ ] SubTask 1.2: 删除 `src/gl/` 下 7 个文件中的重复 `is_stub` 函数，改为调用 `dispatch.is_stub(...)`

- [ ] Task 2: 定义 warn_once! 声明宏
  - [ ] SubTask 2.1: 在 `src/gl/mod.rs` 或新建 `src/gl/macros.rs` 定义 `warn_once!` 宏（管理 AtomicBool + swap + log::warn）
  - [ ] SubTask 2.2: 将 21 个首次告警模式迁移为使用 `warn_once!` 宏
  - [ ] SubTask 2.3: 删除被替换的 21 个 static 变量和 21 个 warn 函数

- [ ] Task 3: 统一 ID 映射失败处理
  - [ ] SubTask 3.1: 在 program.rs 中为 25+ 个静默 return 的函数添加首次告警
  - [ ] SubTask 3.2: 查询类函数（glGetActiveUniform 等）在 ID 失败时写入安全默认值

## 第二阶段：声明宏消除样板（P1）

- [ ] Task 4: 定义 forward! 宏生成纯转发函数
  - [ ] SubTask 4.1: 定义 `forward!` 宏，支持一行声明生成 `#[unsafe(no_mangle)]` + dispatch 调用
  - [ ] SubTask 4.2: 将 vertex_array.rs 中 60+ 个 VertexAttrib 转发函数迁移为宏生成
  - [ ] SubTask 4.3: 将 program.rs 中 20+ 个 Uniform 转发函数迁移为宏生成
  - [ ] SubTask 4.4: 将其他模块的纯转发函数迁移为宏生成

- [ ] Task 5: 实现 glGetError 吞错策略
  - [ ] SubTask 5.1: 在 `src/config.rs` 添加 `swallow_gl_error: bool` 配置字段（默认 true）
  - [ ] SubTask 5.2: 在 `src/gl/getter.rs` 修改 glGetError，启用吞错时返回 GL_NO_ERROR + debug 日志

## 第三阶段：GLSL 转译优化（P0-P1）

- [ ] Task 6: 添加 ResourceLimits 配置（P0，独立于 target 切换）
  - [ ] SubTask 6.1: 在 `src/shader_translator/spirv_compile.rs` 添加 100+ 个 `set_limit` 调用，复刻 MobileGlues InitResources
  - [ ] SubTask 6.2: 重点启用循环限制（nonInductiveForLoops/whileLoops/doWhileLoops = true）

- [ ] Task 7: 切换到 OpenGL target（P1，核心架构优化）
  - [ ] SubTask 7.1: 修改 `spirv_compile.rs` 的 target_env 为 `TargetEnv::OpenGL` + `EnvVersion::OpenGL4_5`
  - [ ] SubTask 7.2: 启用 `set_auto_map_locations(true)`
  - [ ] SubTask 7.3: 修改 `preprocess.rs` 的 `force_glsl_version` 为升级到 `#version 150 compatibility`
  - [ ] SubTask 7.4: 删除 `convert_uniforms_to_ubo`、`inject_missing_locations`、`inject_missing_bindings`、`rename_vulkan_builtin_variables`
  - [ ] SubTask 7.5: 删除 `postprocess.rs` 中的 `unwrap_generated_ubo`、`strip_ubo_instance_name`、`strip_varying_locations`、`strip_uniform_locations`
  - [ ] SubTask 7.6: 添加 GL_ARB_derivative_control 强制处理（`#ifdef GL_ARB_derivative_control` → `#if 0`）
  - [ ] SubTask 7.7: 更新测试（删除 UBO workaround 测试，新增 OpenGL target 测试）

## 第四阶段：性能优化（P2）

- [ ] Task 8: shader 缓存磁盘持久化
  - [ ] SubTask 8.1: 在 `src/shader_translator/cache.rs` 添加 serialize/deserialize 方法
  - [ ] SubTask 8.2: 添加 load（启动时加载）和 save（put 后写入）逻辑
  - [ ] SubTask 8.3: 在 config.rs 添加缓存文件路径配置

- [ ] Task 9: multidraw 性能优化
  - [ ] SubTask 9.1: 在 `src/gl/multi_draw.rs` 实现 func_ptr 惰性缓存（OnceLock<fn>）
  - [ ] SubTask 9.2: 实现 indirect buffer 复用 + 指数增长

- [ ] Task 10: 热路径日志优化
  - [ ] SubTask 10.1: 为热路径 debug 日志添加 `log::log_enabled!(log::Level::Debug)` 包裹

## 第五阶段：验证

- [ ] Task 11: 编译与测试验证
  - [ ] SubTask 11.1: `cargo check` 编译通过
  - [ ] SubTask 11.2: `cargo test` 测试通过（更新后的测试）
  - [ ] SubTask 11.3: 手动验证 MC shader 翻译正确性

# Task Dependencies

- [Task 1] 和 [Task 2] 可并行（不同文件）
- [Task 3] 依赖 [Task 2]（使用 warn_once! 宏）
- [Task 4] 依赖 [Task 2]（宏基础设施）
- [Task 5] 可与 [Task 1]-[Task 4] 并行
- [Task 6] 独立于 GL 优化，可并行
- [Task 7] 依赖 [Task 6]（先确保 ResourceLimits 正确）
- [Task 7.7]（测试更新）依赖 [Task 7.1]-[Task 7.6] 完成
- [Task 8] 可与 [Task 7] 并行（不同模块）
- [Task 9] 可与 [Task 7] 并行
- [Task 10] 可与 [Task 7] 并行
- [Task 11] 依赖所有前置 Task 完成
