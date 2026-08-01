# Checklist

## 第一阶段：GL 实现方式优化（P0）

- [ ] `is_stub` 已提取为 `GlesDispatch::is_stub` 方法，7 个文件中的重复函数已删除
- [ ] `warn_once!` 宏已定义，21 个首次告警模式已迁移
- [ ] program.rs 中 25+ 个静默 return 已添加首次告警
- [ ] 查询类函数 ID 失败时写入安全默认值（0 或 GL_FALSE）

## 第二阶段：声明宏消除样板（P1）

- [ ] `forward!` 宏已定义，支持一行声明生成纯转发函数
- [ ] vertex_array.rs 中 60+ 个 VertexAttrib 转发函数已迁移为宏生成
- [ ] program.rs 中 20+ 个 Uniform 转发函数已迁移为宏生成
- [ ] `glGetError` 吞错策略已实现，可通过 config 配置
- [ ] 吞错模式启用时 glGetError 返回 GL_NO_ERROR + debug 日志

## 第三阶段：GLSL 转译优化（P0-P1）

- [ ] ResourceLimits 已配置，100+ 个 `set_limit` 调用复刻 MobileGlues InitResources
- [ ] 循环限制已启用（nonInductiveForLoops/whileLoops/doWhileLoops = true）
- [ ] target env 已切换为 `TargetEnv::OpenGL` + `EnvVersion::OpenGL4_5`
- [ ] `set_auto_map_locations(true)` 已启用
- [ ] `force_glsl_version` 已改为升级到 `#version 150 compatibility`
- [ ] `convert_uniforms_to_ubo` 已删除
- [ ] `inject_missing_locations` 已删除
- [ ] `inject_missing_bindings` 已删除
- [ ] `rename_vulkan_builtin_variables` 已删除
- [ ] `unwrap_generated_ubo` 已删除
- [ ] `strip_ubo_instance_name` 已删除
- [ ] `strip_varying_locations` 已删除
- [ ] `strip_uniform_locations` 已删除
- [ ] GL_ARB_derivative_control 强制处理已实现
- [ ] UBO workaround 相关测试已删除
- [ ] OpenGL target 新测试已添加

## 第四阶段：性能优化（P2）

- [ ] shader 缓存支持磁盘持久化（load/save）
- [ ] multidraw func_ptr 惰性缓存已实现
- [ ] indirect buffer 复用 + 指数增长已实现
- [ ] 热路径 debug 日志已用 `log_enabled!` 包裹

## 第五阶段：验证

- [ ] `cargo check` 编译通过
- [ ] `cargo test` 测试通过
- [ ] MC shader 翻译正确性已验证（standalone uniform 可查询、location 自动分配）
- [ ] 代码行数显著减少（预期 preprocess.rs 从 ~670 行降至 ~250 行，postprocess.rs 从 ~530 行降至 ~200 行）
