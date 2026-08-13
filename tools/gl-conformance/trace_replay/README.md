# FluorateGL trace replay（apitrace retrace 一致性验证）

本目录构建一个 Linux 命令行回放 runner（基于 [apitrace](https://github.com/apitrace/apitrace)），
用于验证桌面 GL 3.3 trace 经 libfluorategl.so 翻译到 GLES 3.1 后的渲染结果。

从 MobileGL 的 `tools/trace_replay/` 抄写并简化：**单 backend**（DirectGLES = 桌面 GL →
GLES 翻译，无 Vulkan 路径），fixture 通过 MobileGL 的 mirror 跨仓库下载。

## 目录结构

```
trace_replay/
├── CMakeLists.txt                 内嵌构建 apitrace 库 + 生成 glproc + 链接定制 runner
├── apitrace_glproc_fluorategl.cpp 拦截 glReadPixels/glGetError 等（dlopen libfluorategl.so）
├── apitrace_glws_egl.cpp          定制 glws（EGL dlopen 建上下文，pbuffer）
├── apitrace_fbo_dump.{cpp,hpp}    帧缓冲附件 dump 钩子（调试用）
├── trace_replay_core.{cpp,hpp}    runner 核心：retrace + SSIM 对比 + result.json
├── trace_replay_cli.cpp           命令行入口（--fluorategl-library 等）
├── apitrace_exit.hpp              apitrace exit() 拦截
├── run_trace_case.cmake           ctest 单 case 执行脚本
├── trace_cases.json               39 个 case 元数据（target-call/宽高/crop/SSIM 阈值）
├── trace_cases.py                 case 清单生成器（names / cmake / fixture-files）
├── run_retrace_local.py           本地 runner（单 backend，含 fixture 水合 + 结果汇总）
├── scripts/                       fixture 获取与缓存（mirror → Actions cache 两级）
└── fixtures/                      Git LFS 指针（~90 个文本指针，不含内容）
```

## 前置依赖

- apitrace 源码树（MobileGL fork 或上游均可）：`git clone https://github.com/MobileGL-Dev/apitrace.git 3rdparty/apitrace`（CMake 变量 `APITRACE_ROOT` 可覆盖）
- 构建 libfluorategl.so（cargo，x86_64 host）
- CMake ≥ 3.22.1、Ninja、Python3、C++17 编译器

## 构建与运行

```sh
# 构建 libfluorategl（host）
cargo build --target x86_64-unknown-linux-gnu

# 构建 runner（source 目录为 tools/gl-conformance/trace_replay/）
cmake -S tools/gl-conformance/trace_replay -B build-test -G Ninja \
  -DAPITRACE_ROOT=$PWD/3rdparty/apitrace \
  -DFLUORATEGL_TRACE_REPLAY_LIBRARY=$PWD/target/x86_64-unknown-linux-gnu/debug/libfluorategl.so
cmake --build build-test --target fluorategl_trace_replay

# 水合 fixture（mirror 下载；仓库只跟踪 LFS 指针）
bash tools/gl-conformance/trace_replay/scripts/fetch-trace-fixture-lfs.sh OpenRA

# ctest 跑全部 case
ctest --test-dir build-test -V -R 'FluorateGLTraceReplay\.'

# 本地 runner（全部 case + 结果汇总）
python3 tools/gl-conformance/trace_replay/run_retrace_local.py --all
```

环境组（与 common/env.sh 一致，缺一不可）：

```
FLUORATEGL_BACKEND=llvmpipe EGL_PLATFORM=surfaceless
MESA_LOADER_DRIVER_OVERRIDE=llvmpipe LIBGL_ALWAYS_SOFTWARE=1
```

## Fixture 获取机制

- 本仓库只提交 Git LFS **指针**（几十字节文本，`fixtures/*.tgz/png`），不提交 ~0.76 GiB 内容
- `fetch-trace-fixture-lfs.sh <case>`：先试 MobileGL mirror（git.hit.moe / repo.miawa.cn，
  可被 `FLUORATEGL_TRACE_FIXTURE_MIRROR_BASE(S)` 覆盖；兼容读取旧 `MOBILEGL_*` 变量名），
  mirror 全挂时回退 MobileGL 的 GitHub LFS media 端点，下载后按指针 oid 做 SHA-256 校验
- OpenRA 例外：fixture 在 MobileGL 仓库是普通 git 文件（非 LFS），直接从 GitHub raw 下载
- `trace-fixture-cache.sh <key|verify|reset> <case>`：CI 中派生内容寻址缓存 key
  （基于指针 oid，key 前缀 `fluorategl-fixture-v1-`），Actions cache 恢复后校验，失败则 reset 回指针

## CI 映射（供 .github/workflows/Conformance.yml 使用）

| Job | 内容 |
|---|---|
| trace-cases | `trace_cases.py --ci --format names` 生成 case 矩阵（38 个 ci=true） |
| trace-fixtures | 水合全部 CI fixture（mirror → LFS，oid 校验），打包传 artifact |
| build-retrace | 构建 apitrace runner + release lib（传 artifact，矩阵不重复构建） |
| retrace | 每 case 一个矩阵 job（fail-fast=false, max-parallel=4）：下载 runner+fixture，ctest 跑 `FluorateGLTraceReplay.<case>.DirectGLES`，结果 always() 上传 |
