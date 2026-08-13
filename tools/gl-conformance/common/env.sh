#!/usr/bin/env bash
# gl-conformance 统一环境组：selfcheck / piglit / cts / trace_replay 全部共用。
# 用法：source tools/gl-conformance/common/env.sh
#
# 环境说明：
# - FLUORATEGL_BACKEND=llvmpipe：本体系目标库为 libfluorategl.so（dlopen 加载），
#   其内部再 dlopen 真实 GLES 实现——CI 无 GPU，用 Mesa llvmpipe 软件渲染
# - EGL_PLATFORM=surfaceless + MESA_LOADER_DRIVER_OVERRIDE=llvmpipe：
#   无窗口 EGL 平台（PBUFFER/无 surface 上下文）+ 强制 llvmpipe 驱动
# - LIBGL_ALWAYS_SOFTWARE=1：Mesa 兜底软件渲染（glx/egl 双保险）
#
# ⚠️ 库名隔离规则（防递归加载，铁律）：
# - 从不 LD_PRELOAD 自己（libfluorategl.so）——LD_PRELOAD 全局抢先注入会与
#   测试宿主（waffle/eglw/dlopen）冲突，且多个库互相 preload 易成环
# - 从不以 libEGL* / libGL* 命名自己——系统 EGL/GL 加载器按库名前缀解析，
#   同名会导致加载器选中自己、内部再次 dlopen 造成无限递归

export FLUORATEGL_BACKEND=llvmpipe
export EGL_PLATFORM=surfaceless
export MESA_LOADER_DRIVER_OVERRIDE=llvmpipe
export LIBGL_ALWAYS_SOFTWARE=1
