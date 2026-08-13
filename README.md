# FluorateGL

桌面 OpenGL → OpenGL ES 翻译层,尝试让Minecraft:Java Edition在 Android GLES 3.1+ 设备上运行。

> 注意:这是纯 vibe coding 作品,目前无法使用(

## 适用场景

- **目标要求**:Android GLES 3.1+
- **伪装版本**:对外报告 OpenGL 3.3 / GLSL 3.30,内部翻译为 GLES 3.1+

## 构建

```bash
# Android aarch64(需配置 NDK)
cargo build --target aarch64-linux-android --release

# 主机 x86_64 开发构建(测试用)
cargo build --target x86_64-unknown-linux-gnu
```

## 测试

```bash
# 全量测试(cargo + C 差分 + glslang suite)
bash tests/run.sh

# 仅 Rust 单元测试
cargo test --target x86_64-unknown-linux-gnu
```

## 配置

通过环境变量配置:

| 变量 | 值 | 说明 |
|------|-----|------|
| `FLUORATEGL_BACKEND` | `system` / `angle`（暂不可使用） / `llvmpipe` | GLES 后端(默认 `system`) |
| `FLUORATEGL_LOG` | `error` / `warn` / `info` / `debug` / `trace` | 日志级别(默认 `info`) |
| `FLUORATEGL_SKIP_BACKEND` | `1` / `true` | 跳过 EGL/GLES 库加载(纯 CPU 场景) |
| `FLUORATEGL_FAIL_FAST` | `1` | 后端加载失败直接 abort(默认降级 stub) |
| `FLUORATEGL_FORCE_TQL_POLYFILL` | `1` | 强制启用 textureQueryLod polyfill(Mesa 声明扩展但功能未实现时) |
