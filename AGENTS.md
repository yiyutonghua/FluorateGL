# AGENTS.md

桌面 OpenGL → OpenGL ES 翻译层（LD_PRELOAD 拦截 GL/EGL，纯 Rust），让 Minecraft Java 版跑在
Android GLES 3.1+ 上，对外伪装 GL 3.3 / GLSL 3.30 / EGL 1.4。产物 libfluorategl.so
（crate 名 fluorategl，edition 2024）。无 workspace / build.rs / rust-toolchain.toml。

## 构建与测试

- Android 发布（CI 权威）：`cargo ndk --target aarch64-linux-android -- build --release`（NDK r27c + cargo-ndk）
- 主机：`cargo build --target x86_64-unknown-linux-gnu`
- 全量测试：`bash tests/run.sh`；选项 `--skip-glslang | --only-cargo | --only-c | --only-diff`
- Rust 测试：`cargo test --target x86_64-unknown-linux-gnu`
- 单个 example：`cargo run --example glslang_suite --release`、`cargo build --example bench_opt`
- shaderc 以 build-from-source 构建：主机编译需 cmake + ninja + python3 + C++ 编译器

## 测试布局（非标准）

- 4 个 example 定义在 tests/glsl/ 下（test_preprocess / glslang_suite / translate_test / bench_opt），
  `cargo test` 不覆盖它们；集成测试是 tests/ 下的 7 个 .rs 文件（纯 Rust，不碰 EGL）
- 主机跑测试前置：apt 装 libegl-dev libgles2-mesa-dev libgles2 libegl1，并导出
  `LD_LIBRARY_PATH=<仓库根>`、`EGL_PLATFORM=surfaceless`、`MESA_LOADER_DRIVER_OVERRIDE=llvmpipe`
  （run.sh 会在仓库根自动建软链 libGLESv3.so -> libGLESv2.so.2：system 后端默认加载 libGLESv3.so）
- 差分测试另需 `LIBGL_ALWAYS_SOFTWARE=1`，结果用 `python3 tests/gl/diff_compare.py` 对比

## 依赖与生成代码

- submodule tests/glsl/glslang（KhronosGroup/glslang）：glslang_suite 依赖其 Test/ 目录，
  克隆后需 `git submodule update --init --recursive`（CI checkout 已设 recursive）
- src/gl/stub_exports.rs 与 src/symbols.rs 的生成段由 tools/gen_stub_exports.py 产出，勿手改；
  该脚本顶部硬编码仓库外的 MobileGlues 路径，运行前先确认存在

## 环境变量

- 完整环境变量表见 README；此处仅列 README 未收录的一条：
- `GLSLANG_DUMP_FAILURES=<dir>`：glslang_suite 失败时导出失败用例

## 关键机制

- 加载：src/init.rs 的 #[ctor] 只做配置+日志；EGL/GLES 的 dlopen 惰性到首次调用；
  加载失败用 OnceLock 永久降级 stub（FLUORATEGL_FAIL_FAST=1 则 abort）
- 版本伪装：src/config.rs 的 REPORTED_GL_VERSION_PREFIX 与 MAJOR/MINOR 有编译期一致性断言，
  改伪装版本须两处同步
- 入口 src/lib.rs：模块声明 + 公开 re-export（gles_compile_check / ensure_gles_context / fluorategl_init 等）

## 代码地图

- src/init 初始化+惰性后端加载 · src/backend dlopen+分发 · src/gl / src/egl / src/egl_sys 拦截导出层
- src/shader_translator preprocess→spirv_compile→spirv_opt→gles_compile→postprocess+缓存 · src/state ID 映射
- src/context / src/compile_check 离线 GLES 测试辅助 · src/config · src/symbols · src/util

## 仓库约定

- 提交：Conventional Commits（如 feat(shader):），描述用中文；格式化手动 `cargo fmt`
- CI 仅 .github/workflows/Android_Build.yml（push main + 手动触发），只构建不跑测试
- TODO/ 每个主题一个 md、完成即移出 · spec/ 只规划不改代码 · tools/ 只读工具不参与构建
