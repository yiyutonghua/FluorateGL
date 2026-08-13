# FluorateGL 一致性验证体系（gl-conformance）

桌面 GL 3.3 → GLES 翻译层的离线一致性验证，对应 MobileGL 的验证体系
（CTS + piglit + apitrace + selfcheck）。CI 入口：`.github/workflows/Conformance.yml`
（与功能测试 Test.yml 相互独立）。

## 加载机制（核心原则）

**所有宿主一律 dlopen 加载 `libfluorategl.so`**（不链接、不 LD_PRELOAD）：

| 工具 | 目录 | 加载方式 | 状态 |
|---|---|---|---|
| selfcheck | `selfcheck/` | 纯 dlsym（不链接系统 GL/EGL） | P0-1 ✅ |
| piglit | `piglit/` | waffle + `WAFFLE_EGL_LIBRARY/GL_LIBRARY` 指定 | P0-2 |
| CTS (deqp) | `cts/` | eglw + 库路径指定 | P0-3 |
| apitrace trace_replay | `trace_replay/` | LD_LIBRARY_PATH 指定目录 | P0-4 |

库名隔离铁律（见 `common/env.sh`）：从不 LD_PRELOAD 自己、从不以 libEGL* 命名自己。

## 环境组

所有工具统一 source `common/env.sh`：

- `FLUORATEGL_BACKEND=llvmpipe`（目标库内部后端；CI 无 GPU）
- `EGL_PLATFORM=surfaceless` + `MESA_LOADER_DRIVER_OVERRIDE=llvmpipe`
- `LIBGL_ALWAYS_SOFTWARE=1`

## 目录结构

```
tools/gl-conformance/
├── README.md            本文件
├── common/env.sh        统一环境组 + 库名隔离规则
├── selfcheck/           selfcheck.c（dlopen 自检，GL 3.3 core 上下文）
├── piglit/              待接入（P0-2）
├── cts/                 待接入（P0-3）
└── trace_replay/        待接入（P0-4）
```

## CI job 映射（.github/workflows/Conformance.yml）

| Job | 内容 | 基线 |
|---|---|---|
| `selfcheck` | dlopen 自检：EGL 初始化 + GL 3.3 core 上下文 + 版本伪装校验 | 无（自身断言） |
| `piglit-build` → `piglit-run` | waffle+piglit（打 patch）构建 → 精选清单运行 | `piglit/baseline/`（占位） |
| `cts-build` → `cts-run` | glcts（DEQP_TARGET=fluorategl-desktop）构建 → GL33 精选运行 | `cts/baseline/`（占位） |
| `trace-cases` / `trace-fixtures` / `build-retrace` / `retrace` | case 矩阵生成、fixture 水合、runner 构建、逐 case 回放 | `trace_replay/baseline/`（占位） |

基线说明：各工具的 baseline 目录记录**预期结果集**（如 piglit 的 pass/fail 清单），
CI 与基线对比判定回归；baseline 由各 P0-x 任务落地后首次稳定运行产出并提交。
首轮（无 baseline）只记录结果不失败，回归检测从第二轮起生效。
