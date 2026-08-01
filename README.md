# FluorateGL

桌面 OpenGL → OpenGL ES 翻译层,让依赖桌面 GL 的应用在 Android GLES 3.1+ 设备上运行。

> 注意:这是纯 vibe coding 作品

## 适用场景

- **目标设备**:Android GLES 3.1+(骁龙 Adreno 等 GPU)
- **启动器**:FCL / ZL2 等启动器
- **伪装版本**:对外报告 OpenGL 3.2 / GLSL 1.50,内部翻译为 GLES 3.1

## 构建

```bash
# Android aarch64(需配置 NDK)
cargo build --target aarch64-linux-android --release
```

## 配置

通过环境变量配置:

| 变量 | 值 | 说明 |
|------|-----|------|
| `FLUORATEGL_BACKEND` | `system` / `angle` / `llvmpipe` | GLES 后端(默认 `system`) |
| `FLUORATEGL_LOG` | `error` / `warn` / `info` / `debug` / `trace` | 日志级别(默认 `info`) |
| `FLUORATEGL_SKIP_BACKEND` | `1` / `true` | 跳过 EGL/GLES 库加载(纯 CPU 场景) |
