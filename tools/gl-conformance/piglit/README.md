# piglit against FluorateGL（本地版）

在桌面 Linux 上跑 [piglit](https://gitlab.freedesktop.org/mesa/piglit) 的
desktop-GL 测试，以 **FluorateGL 作为 OpenGL 实现**（`libfluorategl.so`），
底层走 Mesa llvmpipe 软件渲染（CI 无 GPU）。

本目录是 MobileGL `tools/piglit-android/` 的本地改写：去掉 Android 专属
（adb/推送/imagereader），保留串行执行 + timeout + `PIGLIT:` 结果解析 +
results.json/summary.txt 产出。patch 只改了注释文字，**代码逻辑与 MobileGL
版完全一致**。

## 工作原理

```
piglit test binary (x86_64, glibc)
  └─ waffle surfaceless_egl（打了 waffle-fluorategl.patch）
       ├─ WAFFLE_EGL_LIBRARY=<libfluorategl.so 绝对路径> → dlopen 它作为 EGL 实现
       ├─ WAFFLE_GL_LIBRARY=<libfluorategl.so 绝对路径> → waffle_dl_sym 在此解析 gl*
       ├─ WAFFLE_FORCE_GL_CONTEXT_VERSION=33core
       │    把低版本 compat 上下文请求升级到 GL 3.3 core（只升不降），
       │    否则 piglit 的 supports_gl_compat_version=10 测试会拿到裸后端的
       │    版本串而自我跳过
       └─ PIGLIT_PLATFORM=surfaceless_egl（无窗口平台）
  └─ libfluorategl.so（FLUORATEGL_BACKEND=llvmpipe）
       └─ 内部 dlopen 系统 libEGL.so.1/libGLESv2.so.2（Mesa llvmpipe）
```

关键规则（与 `tools/gl-conformance/common/env.sh` 一致）：从不 LD_PRELOAD
libfluorategl.so，从不以 libEGL*/libGL* 命名它——FluorateGL 内部用裸 soname
dlopen 系统 EGL/GLES，同名会导致递归加载自己。

## patch 说明

| patch | 作用 |
|---|---|
| `patches/waffle-fluorategl.patch` | ① `WAFFLE_EGL_LIBRARY`/`WAFFLE_GL_LIBRARY` 环境变量让 waffle 的 `dlopen`/`waffle_dl_sym` 指向任意 EGL/GL 库（默认仍是系统 libEGL.so/libGL.so.1）；② `WAFFLE_FORCE_GL_CONTEXT_VERSION=33core` 上下文版本升级（只升不降）；③ meson 构建修复（wayland 关闭时不再清掉 surfaceless_egl 找到的 dep_egl）；④ `#ifdef __ANDROID__` 保护的 imagereader 窗口模式（桌面编译自动跳过） |
| `patches/piglit-fluorategl.patch` | **piglit-dispatch-init.c 修复（核心，必须保留）**：`piglit_dispatch_default_init` 在 waffle 框架构造期间执行（`gl_fw` 为 NULL），原代码走 fallback 分支，GL 函数经**系统** libEGL 的 `eglGetProcAddress` 绑定——静默绕过 waffle dlopen 的目标库，所有测试实际跑在裸驱动上。修复后 waffle 构建一律使用 waffle resolver。另含 CMake 调整（EGL 支持与 EGL 测试解耦、Android 系统名）与 `PIGLIT_DEBUG_VERSION_STRING` 调试打印 |

## 一次性构建（host：Linux，无 GPU 也可）

```sh
WORK=path/to/workdir && cd $WORK
git clone --depth 1 https://gitlab.freedesktop.org/mesa/piglit.git
git clone --depth 1 https://gitlab.freedesktop.org/mesa/waffle.git
git -C waffle apply $FLUORATEGL/tools/gl-conformance/piglit/patches/waffle-fluorategl.patch
git -C piglit apply $FLUORATEGL/tools/gl-conformance/piglit/patches/piglit-fluorategl.patch
python3 -m venv venv && ./venv/bin/pip install mako numpy packaging
```

构建 waffle（meson；桌面 Linux 只开 surfaceless_egl 平台）：

```sh
cd $WORK/waffle
meson setup build -Dbuildtype=release -Dsurfaceless_egl=enabled \
  -Dglx=disabled -Dx11_egl=disabled -Dgbm=disabled -Dwayland=disabled \
  -Dbuild-tests=false -Dbuild-examples=false -Dprefix=$WORK/prefix
ninja -C build && meson install -C build
```

构建 piglit（`PKG_CONFIG_LIBDIR` 指到 waffle 安装目录）：

```sh
cd $WORK/piglit && export PKG_CONFIG_LIBDIR=$WORK/prefix/lib/pkgconfig
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DPIGLIT_USE_WAFFLE=ON -DPIGLIT_BUILD_GL_TESTS=ON \
  -DPIGLIT_BUILD_GLES1_TESTS=OFF -DPIGLIT_BUILD_GLES2_TESTS=OFF \
  -DPIGLIT_BUILD_GLES3_TESTS=OFF -DPIGLIT_BUILD_EGL_TESTS=OFF \
  -DPIGLIT_BUILD_GLX_TESTS=OFF -DPIGLIT_BUILD_WGL_TESTS=OFF \
  -DPIGLIT_BUILD_CL_TESTS=OFF -DPIGLIT_BUILD_VK_TESTS=OFF \
  -DPIGLIT_BUILD_DMA_BUF_TESTS=OFF -DPIGLIT_USE_GBM=OFF \
  -DPIGLIT_USE_WAYLAND=OFF -DPIGLIT_USE_X11=OFF \
  -DPYTHON_EXECUTABLE=$WORK/venv/bin/python
ninja -C build
```

## 枚举用例清单

```sh
PIGLIT_ROOT=$WORK/piglit PIGLIT_BUILD_DIR=$WORK/piglit/build \
  PYTHON=$WORK/venv/bin/python \
  $FLUORATEGL/tools/gl-conformance/piglit/gen_lists.sh          # gl33-full.list（~15k）
$FLUORATEGL/tools/gl-conformance/piglit/gen_lists.sh --ci       # gl33-ci.list（精选）
```

组名用 `@` 分隔（`spec@!opengl 3.3@minmax`）；full 覆盖 GL 1.x–3.3 版本组 +
GLSL 1.10–3.30 组 + ARB 扩展组，ci 聚焦 GL 3.3 / GLSL 3.30 最高版本组并可按
基线覆盖人工裁剪。

## 运行

```sh
source $FLUORATEGL/tools/gl-conformance/common/env.sh   # 统一环境组（runner 内部也会设）
python3 $FLUORATEGL/tools/gl-conformance/piglit/run_piglit_local.py \
  --piglit-root $WORK/piglit --list gl33-ci.list \
  --library $FLUORATEGL/target/release/libfluorategl.so \
  --waffle-dir $WORK/prefix/lib \
  --wflinfo $WORK/prefix/bin/wflinfo \
  --out results-ci
```

参数：

| 参数 | 必填 | 说明 |
|---|---|---|
| `--piglit-root` | 是 | piglit 源码 checkout（内含构建目录） |
| `--build-dir` | 否 | 构建目录名（默认 `build`） |
| `--list` | 是 | `piglit print-cmd` 产出的用例清单（name ::: cmd） |
| `--library` | 是 | `libfluorategl.so` 路径（waffle dlopen 目标；经 `WAFFLE_EGL_LIBRARY`/`WAFFLE_GL_LIBRARY` 注入） |
| `--waffle-dir` | 否 | 含 `libwaffle-1.so` 的目录（进 LD_LIBRARY_PATH）；waffle 装系统路径则省略 |
| `--extra-lib-dir` | 否 | 附加 LD_LIBRARY_PATH 目录（可重复） |
| `--out` | 是 | 结果目录（写 results.json / summary.txt / raw.log） |
| `--timeout` | 否 | 每测试超时秒数（默认 60） |
| `--chunk` | 否 | 每个 chunk 脚本的测试数（默认 200） |
| `--wflinfo` | 否 | wflinfo 二进制；给出则跑全套前先做栈自检（waffle→FluorateGL→llvmpipe 出 3.3 core 上下文） |

runner 内部注入的环境：`WAFFLE_EGL_LIBRARY`/`WAFFLE_GL_LIBRARY`（绝对路径）、
`WAFFLE_FORCE_GL_CONTEXT_VERSION=33core`、`PIGLIT_PLATFORM=surfaceless_egl`、
`PIGLIT_SOURCE_DIR`、`LD_LIBRARY_PATH`（waffle/lib 目录），以及统一环境组
`FLUORATEGL_BACKEND=llvmpipe EGL_PLATFORM=surfaceless
MESA_LOADER_DRIVER_OVERRIDE=llvmpipe LIBGL_ALWAYS_SOFTWARE=1`。

退出码语义：`PIGLIT:` 结果行优先；无结果行且非零退出 = `crash`；GNU timeout
退出码（124/137/142）= `timeout`。chunk 脚本缺标记 = `missing`（本地几乎不会出现）。

## 结果对比与基线

```sh
python3 compare_results.py results.json ../piglit/baseline/results.json -o diff.md
```

`compare_results.py` 输出两轮结果的总数表 + 分类 diff（两侧都坏 / 仅新坏 /
仅新好 / 一侧 skip）。baseline 建议：首次跑出稳定结果后，把 `results.json`、
`summary.txt`、用例清单一起提交到 `piglit/baseline/`，CI 与新跑结果对比
判定回归（bad 集合必须是基线 bad 集合的子集，方向见整合阶段 Conformance.yml）。

## 已知注意事项

- 只有 LLVMpipe 可用时，部分测试较慢——CI 精选清单控制时长，`--timeout`
  按需放宽。
- MSAA winsys 配置不匹配时相关测试失败（llvmpipe 配置面差异），FBO 内部
  MSAA 不受影响。
- 强制 3.3 core 上下文下，真正依赖 compat-profile 特性的测试会失败——
  对 core-only 实现这是诚实的失败。
- 差分类结果（渲染像素对比）在 llvmpipe 上以 pass/fail 结果行计，不做
  像素级 diff（那是 trace_replay 的事，见 P0-4）。
