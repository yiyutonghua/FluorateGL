# FluorateGL 常用工具集

按用途分目录归档的辅助脚本与参考数据。均为只读工具，不参与构建。

## tools/gl/

- **storage_probe.c** — glBufferStorage 三后端（desktop/gles/translate）本机验证程序：
  dlopen 对应 GL 库 + EGL surfaceless 上下文，验证 `glBufferStorage(flags=0/0x42, data=预填)`
  的预填数据能否读回（GetBufferSubData / MapBufferRange READ），用于快速判断
  模拟层与驱动行为差异（真机 Adreno 曾对 flags=0 预填失效，本工具在本机复现/验证）。
  编译：`gcc -o storage_probe storage_probe.c -ldl -lEGL`
  运行（translate 后端需 FLUORATEGL_BACKEND=llvmpipe + 指向 target/debug 的
  LD_LIBRARY_PATH）：
  ```
  LIBGL_ALWAYS_SOFTWARE=1 EGL_PLATFORM=surfaceless ./storage_probe desktop
  LIBGL_ALWAYS_SOFTWARE=1 EGL_PLATFORM=surfaceless ./storage_probe gles
  LIBGL_ALWAYS_SOFTWARE=1 EGL_PLATFORM=surfaceless \
      FLUORATEGL_BACKEND=llvmpipe LD_LIBRARY_PATH=target/debug ./storage_probe translate
  ```

## tools/dump/

- **glslang_fails_baseline.txt / glslang_fails_new.txt** — glslang_suite 翻译套件
  失败清单基线对比（重构前 7ffee37 vs 重构后）：1085 → 1087 编译失败的
  ±2 审计参考数据（新增失败为 Vulkan-only/420pack 边界用例，非真实回归）。
  重新生成方法：`GLSLANG_DUMP_FAILURES=<dir> cargo run --example glslang_suite`
  （glslang_suite.rs 内置 dump 支持）。
