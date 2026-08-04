/* diff_harness.h — 差分测试框架（桌面 GL 3.3 core vs FluorateGL 注入 GLES 3.2）
 *
 * 设计：
 * - 同一 GL 调用序列在两个 backend 上执行，输出统一格式日志（STEP 行），
 *   由外部脚本 diff 对比。
 * - 函数指针统一 dlsym 填表：desktop 从 libGL.so.1，translate 从 libfluorategl.so。
 * - EGL context 双模式：desktop = surfaceless_mesa + no_config + 3.3 core + NO_SURFACE
 *   （T0 探针确认）；translate = 拦截层 EGL（eglGetDisplay + chooseConfig + pbuffer，
 *   参考 tests/gl/test_shader_translation.c）。
 * - ID 归一化：GL 对象 ID 绝对值不可比（两套命名空间），用 reg_id 记录映射，
 *   日志输出逻辑名。
 * - 结果摘要：FNV-1a 64 位哈希（glReadPixels 像素 / buffer 数据）。
 */
#ifndef DIFF_HARNESS_H
#define DIFF_HARNESS_H

#include <stdint.h>
#include <stddef.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>

/* ============ 分类（cls）============ */
#define CLS_MUST 0 /* 纯转发，行为应一致，可严格对比 */
#define CLS_CAP  1 /* 能力相关，允许数值差异 */
#define CLS_EXP  2 /* 已知差异（伪造字符串/过滤/错误序列不同），豁免对比 */
#define CLS_TBD  3 /* 模拟/降级分支，结果取决于宿主能力 */

/* ============ GL 常量（GL 3.3 core / GLES 3.2 共享值）============ */
#define GL_NO_ERROR           0
#define GL_INVALID_ENUM       0x0500
#define GL_INVALID_VALUE      0x0501
#define GL_INVALID_OPERATION  0x0502
#define GL_OUT_OF_MEMORY      0x0505
#define GL_ARRAY_BUFFER       0x8892
#define GL_ELEMENT_ARRAY_BUFFER 0x8893
#define GL_STATIC_DRAW        0x88E4
#define GL_DYNAMIC_DRAW       0x88E8
#define GL_BYTE               0x1400
#define GL_UNSIGNED_BYTE      0x1401
#define GL_SHORT              0x1402
#define GL_UNSIGNED_SHORT     0x1403
#define GL_FLOAT              0x1406
#define GL_RGBA               0x1908
#define GL_RGBA8              0x8058
#define GL_TEXTURE_2D         0x0DE1
#define GL_TEXTURE_BINDING_2D 0x8069
#define GL_DEPTH_COMPONENT24  0x81A6
#define GL_DEPTH24_STENCIL8   0x88F0
#define GL_DEPTH_STENCIL      0x84F9
#define GL_COLOR_ATTACHMENT0  0x8CE0
#define GL_DEPTH_ATTACHMENT   0x8D00
#define GL_STENCIL_ATTACHMENT 0x8D20
#define GL_FRAMEBUFFER        0x8D40
#define GL_FRAMEBUFFER_COMPLETE 0x8CD5
#define GL_RENDERBUFFER       0x8D41
#define GL_READ_FRAMEBUFFER   0x8CA8
#define GL_READ_BUFFER        0x0C02
#define GL_COLOR              0x1800
#define GL_VERSION            0x1F02
#define GL_RENDERER           0x1F01
#define GL_VENDOR             0x1F00
#define GL_EXTENSIONS         0x1F03
#define GL_NUM_EXTENSIONS     0x821D
#define GL_BUFFER_SIZE        0x8764
#define GL_BUFFER_USAGE       0x8765
#define GL_TRIANGLES          0x0004
#define GL_VIEWPORT           0x0BA2
#define GL_BLEND              0x0BE2
#define GL_DEPTH_TEST         0x0B71
#define GL_SRC_ALPHA          0x0302
#define GL_ONE_MINUS_SRC_ALPHA 0x0303
#define GL_TEXTURE0           0x84C0
#define GL_TEXTURE_MIN_FILTER 0x2801
#define GL_TEXTURE_MAG_FILTER 0x2800
#define GL_LINEAR             0x2601
#define GL_NEAREST            0x2600
#define GL_FRONT              0x0404
#define GL_BACK               0x0405
#define GL_FRAMEBUFFER_BINDING 0x8CA6
#define GL_PACK_ALIGNMENT     0x0D05
#define GL_UNPACK_ALIGNMENT   0x0CF5

/* ============ EGL 常量 ============ */
#define EGL_DEFAULT_DISPLAY   ((void*)0)
#define EGL_OPENGL_API        0x30A2
#define EGL_OPENGL_ES_API     0x30A0
#define EGL_NO_CONTEXT        ((void*)0)
#define EGL_NO_SURFACE        ((void*)0)
#define EGL_CONTEXT_MAJOR_VERSION 0x3098
#define EGL_CONTEXT_MINOR_VERSION 0x30FB
#define EGL_CONTEXT_OPENGL_PROFILE_MASK 0x30FD
#define EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT 0x00000001
#define EGL_CONTEXT_CLIENT_VERSION 0x3098
#define EGL_PLATFORM_SURFACELESS_MESA 0x31DD
#define EGL_WIDTH             0x3057
#define EGL_HEIGHT            0x3056
#define EGL_NONE              0x3038
#define EGL_RENDERABLE_TYPE   0x3040
#define EGL_OPENGL_ES2_BIT    0x0004
#define EGL_OPENGL_ES3_BIT    0x0040

/* ============ backend 枚举 ============ */
#define BACKEND_DESKTOP   0 /* 原生桌面 GL 3.3 core（libGL.so.1） */
#define BACKEND_TRANSLATE 1 /* FluorateGL 注入 GLES（libfluorategl.so） */
#define BACKEND_GLES      2 /* native GLES 3.2（libGLESv2.so.2，阶段 B 对照） */

/* ============ 函数指针表 ============ */
typedef struct {
    const char* name; /* 形如 "glGetString" */
    void* fptr;
    int cls;
} GLFn;

/* 宏：展开为 {name, NULL, cls} */
#define F(name, cls) { "gl" #name, NULL, cls }

extern GLFn g_fns[];
extern int g_fn_count;
extern int g_backend;         /* 当前 backend */
extern int g_gl_version_major; /* 解析后的版本 */

/* ============ 初始化 ============ */
/* 返回 0 成功；-1 失败。fluorategl_path 仅 translate 模式用 */
int diff_init_backend(int backend, const char* fluorategl_path);

/* 从函数表取指针（未找到返回 NULL） */
void* g_fn(const char* name);
#define GLF(name) ((name##_t)g_fn("gl" #name))

/* ============ 日志 ============ */
extern FILE* g_log;
void diff_log(const char* fmt, ...);           /* 普通日志行 */
void diff_log_step(const char* case_id, int step, const char* op,
                   const char* fmt, ...);      /* STEP 行 */
void diff_check_errors(const char* case_id, int step); /* 排空 glGetError 队列并记录 */

/* ============ 工具 ============ */
uint64_t diff_fnv1a64(const void* data, size_t len);
void diff_reg_id(uint32_t logical, uint32_t actual); /* ID 归一化（仅日志） */

/* ============ FBO 渲染辅助 ============ */
/* 创建 RGBA8 纹理 + FBO + DEPTH24_STENCIL8 renderbuffer；返回 0 成功 */
int diff_make_render_target(int w, int h, uint32_t* tex_out,
                            uint32_t* fbo_out, uint32_t* rbo_out);
/* draw 后读回像素哈希（需要当前 FBO 已绑定） */
uint64_t diff_render_and_hash(int w, int h);

#endif /* DIFF_HARNESS_H */
