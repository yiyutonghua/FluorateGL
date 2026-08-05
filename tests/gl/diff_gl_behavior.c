/* diff_gl_behavior.c — 差分测试主程序
 *
 * 用法：
 *   ./diff_gl_behavior --backend desktop   [--case ...] [--out FILE]
 *   ./diff_gl_behavior --backend translate [--case ...] [--out FILE]
 *   ./diff_gl_behavior --backend gles      [--case ...] [--out FILE]  (阶段 B: native GLES)
 *   ./diff_gl_behavior --list
 *
 * 环境（Termux + Mesa llvmpipe）：
 *   desktop  : LIBGL_ALWAYS_SOFTWARE=1 ./diff_gl_behavior --backend desktop
 *   translate: FLUORATEGL_BACKEND=llvmpipe EGL_PLATFORM=surfaceless \
 *              LIBGL_ALWAYS_SOFTWARE=1 ./diff_gl_behavior --backend translate
 *   gles     : LIBGL_ALWAYS_SOFTWARE=1 ./diff_gl_behavior --backend gles
 */
#include "diff_harness.h"
#include "diff_shaders.h"

/* ============ 函数指针表（约 90 个，T2 分类）============
 * cls: 0=must 1=cap 2=exp 3=tbd
 */
GLFn g_fns[] = {
    /* --- 状态 --- */
    F(Enable, 0), F(Disable, 0), F(IsEnabled, 0),
    F(Viewport, 0), F(Scissor, 0), F(CullFace, 0), F(FrontFace, 0),
    F(LineWidth, 0), F(PolygonOffset, 0),
    F(ClearColor, 0), F(ClearDepth, 0), F(ClearStencil, 0), F(Clear, 0),
    F(DepthFunc, 0), F(DepthMask, 0), F(DepthRange, 0),
    F(BlendFunc, 0), F(BlendFuncSeparate, 0), F(BlendEquation, 0),
    F(ColorMask, 0), F(StencilFunc, 0), F(StencilOp, 0), F(StencilMask, 0),
    F(PixelStorei, 0), F(ActiveTexture, 0),
    F(PolygonMode, 2),          /* GLES 仅 FILL，非 FILL 被忽略 */
    F(PointSize, 2),            /* GLES 无 glPointSize，shader 恒生效 */
    F(ClampColor, 2),           /* 恒 clamp no-op */
    /* --- Buffer --- */
    F(GenBuffers, 0), F(DeleteBuffers, 0), F(BindBuffer, 0),
    F(BufferData, 0), F(BufferSubData, 0), F(BufferStorage, 3),
    F(MapBuffer, 3), F(MapBufferRange, 3), F(UnmapBuffer, 3),
    F(FlushMappedBufferRange, 3), F(CopyBufferSubData, 0),
    F(GetBufferSubData, 3), F(GetBufferParameteriv, 0),
    F(GetBufferPointerv, 3), F(BindBufferBase, 0), F(BindBufferRange, 0),
    F(IsBuffer, 0),
    /* --- 纹理 --- */
    F(GenTextures, 0), F(DeleteTextures, 0), F(BindTexture, 0),
    F(TexImage2D, 0), F(TexSubImage2D, 0), F(TexImage3D, 0),
    F(TexParameteri, 0), F(TexParameterf, 0), F(TexParameterfv, 0),
    F(TexStorage2D, 1), F(GenerateMipmap, 0),
    F(CompressedTexImage2D, 3), F(GetTexParameteriv, 0),
    F(GetTexImage, 3),          /* GLES 模拟 FBO 读回 */
    F(CopyTexImage2D, 0), F(CopyTexSubImage2D, 0),
    F(TexBuffer, 3),
    /* --- VAO / 顶点 --- */
    F(GenVertexArrays, 0), F(DeleteVertexArrays, 0), F(BindVertexArray, 0),
    F(EnableVertexAttribArray, 0), F(DisableVertexAttribArray, 0),
    F(VertexAttribPointer, 0), F(VertexAttribIPointer, 0),
    F(VertexAttribDivisor, 0),
    F(BindVertexBuffer, 0), F(VertexAttribFormat, 0),
    F(VertexAttribBinding, 0),
    F(VertexAttrib1f, 3), F(VertexAttrib2f, 3), F(VertexAttrib3f, 3),
    F(VertexAttrib4f, 3),
    /* --- Program / Shader --- */
    F(CreateProgram, 0), F(DeleteProgram, 0), F(UseProgram, 0),
    F(CreateShader, 0), F(DeleteShader, 0), F(AttachShader, 0),
    F(DetachShader, 0), F(ShaderSource, 0), F(CompileShader, 0),
    F(LinkProgram, 0), F(GetShaderiv, 0), F(GetProgramiv, 0),
    F(GetShaderInfoLog, 0), F(GetProgramInfoLog, 0),
    F(GetUniformLocation, 0), F(GetAttribLocation, 0),
    F(GetActiveUniform, 0), F(GetActiveAttrib, 0),
    F(Uniform1f, 0), F(Uniform2f, 0), F(Uniform3f, 0), F(Uniform4f, 0),
    F(Uniform1i, 0), F(UniformMatrix4fv, 0),
    F(UniformBlockBinding, 0), F(GetUniformBlockIndex, 0),
    F(GetUniformIndices, 0), F(GetActiveUniformsiv, 0),
    F(GetActiveUniformName, 0),
    F(BindAttribLocation, 0), F(ValidateProgram, 0),
    F(GetShaderSource, 2),     /* 返回翻译后 vs 原始，已知差异 */
    /* --- FBO / 渲染目标 --- */
    F(GenFramebuffers, 0), F(DeleteFramebuffers, 0), F(BindFramebuffer, 0),
    F(FramebufferTexture2D, 0), F(FramebufferTextureLayer, 0),
    F(FramebufferRenderbuffer, 0),
    F(GenRenderbuffers, 0), F(DeleteRenderbuffers, 0), F(BindRenderbuffer, 0),
    F(RenderbufferStorage, 0),
    F(CheckFramebufferStatus, 0),
    F(DrawBuffers, 0), F(ReadBuffer, 0), F(BlitFramebuffer, 0),
    F(ReadPixels, 0), F(GetFramebufferAttachmentParameteriv, 0),
    /* --- Draw --- */
    F(DrawArrays, 0), F(DrawElements, 0),
    F(DrawArraysInstanced, 0), F(DrawElementsInstanced, 0),
    F(DrawRangeElements, 0),
    F(DrawArraysIndirect, 3), F(DrawElementsIndirect, 3),
    F(DrawElementsBaseVertex, 3),
    F(MultiDrawArrays, 3), F(MultiDrawElements, 3),
    F(PrimitiveRestartIndex, 3),
    /* --- 查询 --- */
    F(GetError, 0), F(GetString, 2), F(GetStringi, 2),
    F(GetIntegerv, 0), F(GetBooleanv, 0), F(GetFloatv, 0),
    F(GetVertexAttribiv, 0), F(GetVertexAttribfv, 0),
    F(GetDoublev, 2),          /* GLES 模拟转换 */
    /* --- 同步 --- */
    F(FenceSync, 0), F(DeleteSync, 0), F(ClientWaitSync, 0),
    F(WaitSync, 0), F(IsSync, 0),
    /* --- 查询对象 --- */
    F(GenQueries, 0), F(DeleteQueries, 0), F(BeginQuery, 3),
    F(EndQuery, 3), F(GetQueryiv, 3), F(GetQueryObjectuiv, 3),
};

int g_fn_count = (int)(sizeof(g_fns) / sizeof(g_fns[0]));

int g_backend = BACKEND_DESKTOP;
int g_gl_version_major = 0;
FILE* g_log = NULL;

/* EGL 函数指针获取：desktop 从 RTLD_DEFAULT（libEGL.so.1 已链接），
 * translate 从 libfluorategl.so handle（拦截层导出 34 个 EGL 符号） */
static void* g_egl_handle = NULL;

/* ============ EGL 函数指针（当前模式） ============ */
static void* egl_fn(const char* name) {
    return dlsym(g_egl_handle ? g_egl_handle : RTLD_DEFAULT, name);
}

#define EGL_F(name) ((void*)(egl_fn(name)))

/* EGL 函数指针类型化获取（消除 void* cast 警告） */
typedef void* (*eglGetDisplay_fn)(void*);
typedef void* (*eglGetPlatformDisplay_fn)(uint32_t, void*, const int*);
typedef int (*eglInitialize_fn)(void*, int*, int*);
typedef int (*eglBindAPI_fn)(uint32_t);
typedef int (*eglChooseConfig_fn)(void*, const int*, void**, int, int*);
typedef void* (*eglCreateContext_fn)(void*, void*, void*, const int*);
typedef void* (*eglCreatePbufferSurface_fn)(void*, void*, const int*);
typedef int (*eglMakeCurrent_fn)(void*, void*, void*, void*);
typedef int (*eglGetError_fn)(void);

/* ============ 日志 ============ */
void diff_log(const char* fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    if (g_log) { vfprintf(g_log, fmt, ap); fputc('\n', g_log); fflush(g_log); }
    va_end(ap);
}

void diff_log_step(const char* case_id, int step, const char* op,
                   const char* fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    if (g_log) {
        fprintf(g_log, "STEP %s.%d | %s | ", case_id, step, op);
        vfprintf(g_log, fmt, ap);
        fputc('\n', g_log);
        fflush(g_log);
    }
    va_end(ap);
}

/* 排空 glGetError 队列 */
void diff_check_errors(const char* case_id, int step) {
    typedef uint32_t (*getError_t)(void);
    getError_t ge = (getError_t)g_fn("glGetError");
    if (!ge) return;
    int n = 0;
    uint32_t e;
    char buf[512] = "";
    while ((e = ge()) != GL_NO_ERROR && n < 16) {
        char one[64];
        snprintf(one, sizeof(one), "%s0x%04X", n ? "," : "", e);
        strncat(buf, one, sizeof(buf) - strlen(buf) - 1);
        n++;
    }
    if (n) diff_log_step(case_id, step, "check_glGetError", "err=[%s] (%d)", buf, n);
    else  diff_log_step(case_id, step, "check_glGetError", "err=[]");
}

/* ============ 工具 ============ */
uint64_t diff_fnv1a64(const void* data, size_t len) {
    const uint8_t* p = (const uint8_t*)data;
    uint64_t h = 0xcbf29ce484222325ULL;
    for (size_t i = 0; i < len; i++) {
        h ^= p[i];
        h *= 0x100000001b3ULL;
    }
    return h;
}

/* ID 归一化：记录 logical→actual 映射（仅日志，供外部 diff 时对齐） */
#define MAX_ID_REG 256
static uint32_t g_id_logical[MAX_ID_REG];
static uint32_t g_id_actual[MAX_ID_REG];
static int g_id_count = 0;
void diff_reg_id(uint32_t logical, uint32_t actual) {
    for (int i = 0; i < g_id_count; i++) {
        if (g_id_logical[i] == logical) {
            if (g_id_actual[i] != actual) {
                diff_log("IDMAP %u -> %u (was %u)", logical, actual, g_id_actual[i]);
                g_id_actual[i] = actual;
            }
            return;
        }
    }
    if (g_id_count < MAX_ID_REG) {
        g_id_logical[g_id_count] = logical;
        g_id_actual[g_id_count] = actual;
        g_id_count++;
        diff_log("IDMAP %u -> %u", logical, actual);
    }
}

/* ============ FBO 渲染辅助 ============ */
typedef void (*bindTex_t)(uint32_t, uint32_t);
typedef void (*genTex_t)(int, uint32_t*);
typedef void (*texImage2D_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
typedef void (*texParam_t)(uint32_t, uint32_t, int);
typedef void (*genFB_t)(int, uint32_t*);
typedef void (*bindFB_t)(uint32_t, uint32_t);
typedef void (*fbTex2D_t)(uint32_t, uint32_t, uint32_t, uint32_t, int);
typedef void (*fbRenderbuffer_t)(uint32_t, uint32_t, uint32_t, uint32_t);
typedef void (*genRB_t)(int, uint32_t*);
typedef void (*bindRB_t)(uint32_t, uint32_t);
typedef void (*rbStorage_t)(uint32_t, uint32_t, int, int);
typedef uint32_t (*checkFB_t)(uint32_t);
typedef void (*readPixels_t)(int, int, int, int, uint32_t, uint32_t, void*);

int diff_make_render_target(int w, int h, uint32_t* tex_out,
                            uint32_t* fbo_out, uint32_t* rbo_out) {
    bindTex_t bindTex = (bindTex_t)g_fn("glBindTexture");
    genTex_t genTex = (genTex_t)g_fn("glGenTextures");
    texImage2D_t texImage = (texImage2D_t)g_fn("glTexImage2D");
    texParam_t texParam = (texParam_t)g_fn("glTexParameteri");
    genFB_t genFB = (genFB_t)g_fn("glGenFramebuffers");
    bindFB_t bindFB = (bindFB_t)g_fn("glBindFramebuffer");
    fbTex2D_t fbTex = (fbTex2D_t)g_fn("glFramebufferTexture2D");
    fbRenderbuffer_t fbRb = (fbRenderbuffer_t)g_fn("glFramebufferRenderbuffer");
    genRB_t genRB = (genRB_t)g_fn("glGenRenderbuffers");
    bindRB_t bindRB = (bindRB_t)g_fn("glBindRenderbuffer");
    rbStorage_t rbSto = (rbStorage_t)g_fn("glRenderbufferStorage");
    checkFB_t checkFB = (checkFB_t)g_fn("glCheckFramebufferStatus");
    if (!genTex || !bindTex || !texImage || !texParam || !genFB || !bindFB ||
        !fbTex || !fbRb || !genRB || !bindRB || !rbSto || !checkFB) {
        diff_log("make_render_target: 缺少函数指针");
        return -1;
    }

    /* 纹理 */
    uint32_t tex = 0, fbo = 0, rbo = 0;
    genTex(1, &tex);
    bindTex(GL_TEXTURE_2D, tex);
    texImage(GL_TEXTURE_2D, 0, GL_RGBA8, w, h, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
    texParam(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
    texParam(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);

    /* FBO + 颜色附件 */
    genFB(1, &fbo);
    bindFB(GL_FRAMEBUFFER, fbo);
    fbTex(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);

    /* 深度模板 renderbuffer（glFramebufferRenderbuffer 挂载） */
    genRB(1, &rbo);
    bindRB(GL_RENDERBUFFER, rbo);
    rbSto(GL_RENDERBUFFER, GL_DEPTH24_STENCIL8, w, h);
    bindFB(GL_FRAMEBUFFER, fbo);
    fbRb(GL_FRAMEBUFFER, GL_DEPTH_ATTACHMENT, GL_RENDERBUFFER, rbo);
    fbRb(GL_FRAMEBUFFER, GL_STENCIL_ATTACHMENT, GL_RENDERBUFFER, rbo);

    uint32_t st = checkFB(GL_FRAMEBUFFER);
    if (st != GL_FRAMEBUFFER_COMPLETE) {
        diff_log("make_render_target: FBO 不完整 status=0x%04X", st);
        return -1;
    }
    *tex_out = tex; *fbo_out = fbo; *rbo_out = rbo;
    return 0;
}

uint64_t diff_render_and_hash(int w, int h) {
    readPixels_t rp = (readPixels_t)g_fn("glReadPixels");
    if (!rp) return 0;
    size_t sz = (size_t)w * h * 4;
    uint8_t* buf = (uint8_t*)malloc(sz);
    if (!buf) return 0;
    rp(0, 0, w, h, GL_RGBA, GL_UNSIGNED_BYTE, buf);
    uint64_t hsh = diff_fnv1a64(buf, sz);
    free(buf);
    return hsh;
}

/* ============ 初始化 ============ */
int diff_init_backend(int backend, const char* fluorategl_path) {
    g_backend = backend;
    /* 日志默认 stdout（--out 时 main 已设置文件句柄，此处不覆盖） */
    if (!g_log) g_log = stdout;

    if (backend == BACKEND_DESKTOP) {
        /* desktop 模式：EGL 函数从 libEGL.so.1 获取（Android 系统 EGL dispatch） */
        g_egl_handle = dlopen("libEGL.so.1", RTLD_NOW | RTLD_GLOBAL);
        if (!g_egl_handle) {
            fprintf(stderr, "dlopen(libEGL.so.1) 失败: %s\n", dlerror());
            return -1;
        }
    } else if (backend == BACKEND_GLES) {
        /* native GLES 模式：EGL 也从 libEGL.so.1 获取（mesa EGL） */
        g_egl_handle = dlopen("libEGL.so.1", RTLD_NOW | RTLD_GLOBAL);
        if (!g_egl_handle) {
            fprintf(stderr, "dlopen(libEGL.so.1) 失败: %s\n", dlerror());
            return -1;
        }
    } else if (backend == BACKEND_TRANSLATE) {
        /* dlopen FluorateGL（translate 模式）；desktop 模式无需 */
        void* h = dlopen(fluorategl_path, RTLD_NOW | RTLD_GLOBAL);
        if (!h) {
            fprintf(stderr, "dlopen(%s) 失败: %s\n", fluorategl_path, dlerror());
            return -1;
        }
        g_egl_handle = h;
        /* 调 fluorategl_init()（读 FLUORATEGL_BACKEND 等环境变量） */
        typedef int (*init_t)(void);
        init_t init = (init_t)dlsym(h, "fluorategl_init");
        if (!init) {
            fprintf(stderr, "dlsym(fluorategl_init) 失败\n");
            return -1;
        }
        int rc = init();
        if (rc != 0) {
            fprintf(stderr, "fluorategl_init 返回 %d\n", rc);
            return -1;
        }
    }

    /* 填函数表 */
    const char* lib = (backend == BACKEND_DESKTOP) ? "libGL.so.1"
                   : (backend == BACKEND_GLES) ? "libGLESv2.so.2" : NULL;
    void* glh = NULL;
    if (backend == BACKEND_DESKTOP || backend == BACKEND_GLES) {
        glh = dlopen(lib, RTLD_NOW | RTLD_GLOBAL);
        if (!glh) {
            fprintf(stderr, "dlopen(%s) 失败: %s\n", lib, dlerror());
            return -1;
        }
    } else {
        glh = g_egl_handle; /* translate: 从 libfluorategl.so 填表 */
    }
    int missing = 0;
    for (int i = 0; i < g_fn_count; i++) {
        g_fns[i].fptr = dlsym(glh, g_fns[i].name);
        if (!g_fns[i].fptr) {
            diff_log("SYM missing %s [cls=%d]", g_fns[i].name, g_fns[i].cls);
            missing++;
        }
    }
    diff_log("填表完成: %d/%d (missing %d)", g_fn_count - missing, g_fn_count, missing);
    return 0;
}

void* g_fn(const char* name) {
    for (int i = 0; i < g_fn_count; i++) {
        if (strcmp(g_fns[i].name, name) == 0) return g_fns[i].fptr;
    }
    return NULL;
}

/* ============ EGL context 创建 ============ */
static int create_context_desktop(void) {
    eglGetPlatformDisplay_fn gp = (eglGetPlatformDisplay_fn)egl_fn("eglGetPlatformDisplay");
    eglInitialize_fn init = (eglInitialize_fn)egl_fn("eglInitialize");
    eglBindAPI_fn bind = (eglBindAPI_fn)egl_fn("eglBindAPI");
    eglCreateContext_fn mkctx = (eglCreateContext_fn)egl_fn("eglCreateContext");
    eglMakeCurrent_fn mkcur = (eglMakeCurrent_fn)egl_fn("eglMakeCurrent");
    eglGetError_fn gerr = (eglGetError_fn)egl_fn("eglGetError");
    if (!gp || !init || !bind || !mkctx || !mkcur) {
        fprintf(stderr, "desktop: EGL 函数缺失\n");
        return -1;
    }
    void* dpy = gp(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
    if (!dpy) { fprintf(stderr, "desktop: eglGetPlatformDisplay 失败 err=0x%04X\n", gerr()); return -1; }
    int maj = 0, min = 0;
    if (!init(dpy, &maj, &min)) { fprintf(stderr, "desktop: eglInitialize 失败 err=0x%04X\n", gerr()); return -1; }
    if (!bind(EGL_OPENGL_API)) { fprintf(stderr, "desktop: eglBindAPI(OPENGL) 失败 err=0x%04X\n", gerr()); return -1; }
    int attrs[] = {
        EGL_CONTEXT_MAJOR_VERSION, 3,
        EGL_CONTEXT_MINOR_VERSION, 3,
        EGL_CONTEXT_OPENGL_PROFILE_MASK, EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT,
        EGL_NONE
    };
    void* ctx = mkctx(dpy, NULL /* no-config: EGL_KHR_no_config_context */,
                      EGL_NO_CONTEXT, attrs);
    if (!ctx) { fprintf(stderr, "desktop: eglCreateContext(3.3 core) 失败 err=0x%04X\n", gerr()); return -1; }
    if (!mkcur(dpy, EGL_NO_SURFACE, EGL_NO_SURFACE, ctx)) {
        fprintf(stderr, "desktop: eglMakeCurrent(NO_SURFACE) 失败 err=0x%04X\n", gerr());
        return -1;
    }
    diff_log("desktop context: surfaceless 3.3 core OK");
    return 0;
}

static int create_context_translate(void) {
    eglGetDisplay_fn gd = (eglGetDisplay_fn)egl_fn("eglGetDisplay");
    eglInitialize_fn init = (eglInitialize_fn)egl_fn("eglInitialize");
    eglBindAPI_fn bind = (eglBindAPI_fn)egl_fn("eglBindAPI");
    eglCreateContext_fn mkctx = (eglCreateContext_fn)egl_fn("eglCreateContext");
    eglMakeCurrent_fn mkcur = (eglMakeCurrent_fn)egl_fn("eglMakeCurrent");
    eglGetError_fn gerr = (eglGetError_fn)egl_fn("eglGetError");
    if (!gd || !init || !bind || !mkctx || !mkcur) {
        fprintf(stderr, "translate: 拦截层 EGL 函数缺失\n");
        return -1;
    }
    void* dpy = gd(EGL_DEFAULT_DISPLAY);
    if (!dpy) { fprintf(stderr, "translate: eglGetDisplay 失败 err=0x%04X\n", gerr()); return -1; }
    int maj = 0, min = 0;
    if (!init(dpy, &maj, &min)) { fprintf(stderr, "translate: eglInitialize 失败 err=0x%04X\n", gerr()); return -1; }
    if (!bind(EGL_OPENGL_ES_API)) { fprintf(stderr, "translate: eglBindAPI(ES) 失败 err=0x%04X\n", gerr()); return -1; }
    /* surfaceless 平台无 config：用 EGL_KHR_no_config_context 直接创建 */
    int ctx_attrs[] = { EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE };
    void* ctx = mkctx(dpy, NULL /* no-config */, EGL_NO_CONTEXT, ctx_attrs);
    if (!ctx) { fprintf(stderr, "translate: eglCreateContext(ES3) 失败 err=0x%04X\n", gerr()); return -1; }
    if (!mkcur(dpy, EGL_NO_SURFACE, EGL_NO_SURFACE, ctx)) {
        fprintf(stderr, "translate: eglMakeCurrent(NO_SURFACE) 失败 err=0x%04X\n", gerr());
        return -1;
    }
    diff_log("translate context: GLES 3 no-config surfaceless OK");
    return 0;
}

/* native GLES 3.2 context：显式 surfaceless 平台（不依赖环境变量）→ no_config → GLES3 → NO_SURFACE */
static int create_context_gles(void) {
    eglGetPlatformDisplay_fn gp = (eglGetPlatformDisplay_fn)egl_fn("eglGetPlatformDisplay");
    eglInitialize_fn init = (eglInitialize_fn)egl_fn("eglInitialize");
    eglBindAPI_fn bind = (eglBindAPI_fn)egl_fn("eglBindAPI");
    eglCreateContext_fn mkctx = (eglCreateContext_fn)egl_fn("eglCreateContext");
    eglMakeCurrent_fn mkcur = (eglMakeCurrent_fn)egl_fn("eglMakeCurrent");
    eglGetError_fn gerr = (eglGetError_fn)egl_fn("eglGetError");
    if (!gp || !init || !bind || !mkctx || !mkcur) {
        fprintf(stderr, "gles: EGL 函数缺失\n");
        return -1;
    }
    void* dpy = gp(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
    if (!dpy) { fprintf(stderr, "gles: eglGetPlatformDisplay 失败 err=0x%04X\n", gerr()); return -1; }
    int maj = 0, min = 0;
    if (!init(dpy, &maj, &min)) { fprintf(stderr, "gles: eglInitialize 失败 err=0x%04X\n", gerr()); return -1; }
    if (!bind(EGL_OPENGL_ES_API)) { fprintf(stderr, "gles: eglBindAPI(ES) 失败 err=0x%04X\n", gerr()); return -1; }
    int ctx_attrs[] = { EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE };
    void* ctx = mkctx(dpy, NULL /* no-config */, EGL_NO_CONTEXT, ctx_attrs);
    if (!ctx) { fprintf(stderr, "gles: eglCreateContext(ES3) 失败 err=0x%04X\n", gerr()); return -1; }
    if (!mkcur(dpy, EGL_NO_SURFACE, EGL_NO_SURFACE, ctx)) {
        fprintf(stderr, "gles: eglMakeCurrent(NO_SURFACE) 失败 err=0x%04X\n", gerr());
        return -1;
    }
    diff_log("gles context: GLES 3 no-config surfaceless OK");
    return 0;
}

int create_backend_context(int backend) {
    if (backend == BACKEND_DESKTOP) return create_context_desktop();
    if (backend == BACKEND_GLES) return create_context_gles();
    return create_context_translate();
}

/* ============ 用例 ============ */
typedef void (*case_fn_t)(const char* case_id, int* step_out);

typedef struct {
    const char* id;
    const char* name;
    case_fn_t fn;
} CaseDef;

/* 用例 a00：版本/渲染器/扩展枚举 */
static void case_a00(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef const char* (*getString_t)(uint32_t);
    typedef const char* (*getStringi_t)(uint32_t, uint32_t);
    typedef void (*getIntegerv_t)(uint32_t, int*);
    getString_t gs = (getString_t)g_fn("glGetString");
    getStringi_t gsi = (getStringi_t)g_fn("glGetStringi");
    getIntegerv_t gi = (getIntegerv_t)g_fn("glGetIntegerv");

    diff_log_step(case_id, step++, "glGetString", "GL_VERSION=%s", gs ? gs(GL_VERSION) : "(missing)");
    diff_log_step(case_id, step++, "glGetString", "GL_RENDERER=%s", gs ? gs(GL_RENDERER) : "(missing)");
    diff_log_step(case_id, step++, "glGetString", "GL_VENDOR=%s", gs ? gs(GL_VENDOR) : "(missing)");
    int num = 0;
    if (gi) gi(GL_NUM_EXTENSIONS, &num);
    diff_log_step(case_id, step++, "glGetIntegerv", "GL_NUM_EXTENSIONS=%d", num);
    if (gsi) {
        for (int i = 0; i < num && i < 8; i++) {
            const char* e = gsi(GL_EXTENSIONS, i);
            diff_log_step(case_id, step++, "glGetStringi", "ext[%d]=%s", i, e ? e : "(null)");
        }
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* 用例 b00：GenBuffer + Bind + BufferData + 参数查询 + 读回哈希 */
static void case_b00(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*genBuffers_t)(int, uint32_t*);
    typedef void (*bindBuffer_t)(uint32_t, uint32_t);
    typedef void (*bufferData_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*getBufferParam_t)(uint32_t, uint32_t, int*);
    typedef void (*getBufferSubData_t)(uint32_t, intptr_t, intptr_t, void*);
    genBuffers_t gb = (genBuffers_t)g_fn("glGenBuffers");
    bindBuffer_t bb = (bindBuffer_t)g_fn("glBindBuffer");
    bufferData_t bd = (bufferData_t)g_fn("glBufferData");
    getBufferParam_t gbp = (getBufferParam_t)g_fn("glGetBufferParameteriv");
    getBufferSubData_t gbsd = (getBufferSubData_t)g_fn("glGetBufferSubData");

    uint8_t data[256];
    for (int i = 0; i < 256; i++) data[i] = (uint8_t)(i * 3 + 1);

    uint32_t buf = 0;
    if (gb) { gb(1, &buf); diff_reg_id(buf, buf); }
    diff_log_step(case_id, step++, "glGenBuffers", "buf=%u", buf);
    if (bb) bb(GL_ARRAY_BUFFER, buf);
    diff_log_step(case_id, step++, "glBindBuffer", "buf=%u", buf);
    if (bd) bd(GL_ARRAY_BUFFER, 256, data, GL_STATIC_DRAW);
    diff_log_step(case_id, step++, "glBufferData", "size=256 data=fnv1a64");
    int sz = 0, usage = 0;
    if (gbp) { gbp(GL_ARRAY_BUFFER, GL_BUFFER_SIZE, &sz); gbp(GL_ARRAY_BUFFER, GL_BUFFER_USAGE, &usage); }
    diff_log_step(case_id, step++, "glGetBufferParameteriv", "SIZE=%d USAGE=0x%04X", sz, usage);
    if (gbsd) {
        uint8_t rd[256] = {0};
        gbsd(GL_ARRAY_BUFFER, 0, 256, rd);
        uint64_t h = diff_fnv1a64(rd, 256);
        diff_log_step(case_id, step++, "glGetBufferSubData", "readback_hash=0x%016llX",
                      (unsigned long long)h);
    } else {
        diff_log_step(case_id, step++, "glGetBufferSubData", "(missing)");
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* ============ 阶段 B 用例（native GLES vs FluorateGL）============ */

/* 通用 shader 编译链接辅助：返回 program（0=失败），日志记录每步状态 */
static uint32_t build_program_es(const char* case_id, int* step,
                                 const char* vs_src, const char* fs_src) {
    typedef uint32_t (*createShader_t)(uint32_t);
    typedef void (*deleteShader_t)(uint32_t);
    typedef void (*shaderSource_t)(uint32_t, int, const char**, const int*);
    typedef void (*compileShader_t)(uint32_t);
    typedef void (*getShaderiv_t)(uint32_t, uint32_t, int*);
    typedef void (*getShaderInfoLog_t)(uint32_t, int, int*, char*);
    typedef uint32_t (*createProgram_t)(void);
    typedef void (*attachShader_t)(uint32_t, uint32_t);
    typedef void (*linkProgram_t)(uint32_t);
    typedef void (*getProgramiv_t)(uint32_t, uint32_t, int*);
    typedef void (*getProgramInfoLog_t)(uint32_t, int, int*, char*);
    typedef void (*deleteProgram_t)(uint32_t);
    createShader_t cs = (createShader_t)g_fn("glCreateShader");
    deleteShader_t ds = (deleteShader_t)g_fn("glDeleteShader");
    shaderSource_t ss = (shaderSource_t)g_fn("glShaderSource");
    compileShader_t cpl = (compileShader_t)g_fn("glCompileShader");
    getShaderiv_t gsi = (getShaderiv_t)g_fn("glGetShaderiv");
    getShaderInfoLog_t gsil = (getShaderInfoLog_t)g_fn("glGetShaderInfoLog");
    createProgram_t cp = (createProgram_t)g_fn("glCreateProgram");
    attachShader_t at = (attachShader_t)g_fn("glAttachShader");
    linkProgram_t lp = (linkProgram_t)g_fn("glLinkProgram");
    getProgramiv_t gpi = (getProgramiv_t)g_fn("glGetProgramiv");
    getProgramInfoLog_t gpil = (getProgramInfoLog_t)g_fn("glGetProgramInfoLog");
    deleteProgram_t dp = (deleteProgram_t)g_fn("glDeleteProgram");
    if (!cs || !ss || !cpl || !gsi || !cp || !at || !lp || !gpi) {
        diff_log_step(case_id, (*step)++, "build_program", "missing 函数指针");
        return 0;
    }
    const uint32_t GL_VERTEX_SHADER = 0x8B31, GL_FRAGMENT_SHADER = 0x8B30;
    const uint32_t GL_COMPILE_STATUS = 0x8B81, GL_LINK_STATUS = 0x8B82;

    uint32_t vs = cs(GL_VERTEX_SHADER);
    uint32_t fs = cs(GL_FRAGMENT_SHADER);
    const char* vs_src_p = vs_src; const char* fs_src_p = fs_src;
    ss(vs, 1, &vs_src_p, NULL);
    ss(fs, 1, &fs_src_p, NULL);
    cpl(vs); cpl(fs);
    int vs_ok = 0, fs_ok = 0;
    gsi(vs, GL_COMPILE_STATUS, &vs_ok);
    gsi(fs, GL_COMPILE_STATUS, &fs_ok);
    char log[512] = "";
    if (gsil && !vs_ok) { int n = 0; gsil(vs, 512, &n, log); }
    diff_log_step(case_id, (*step)++, "glCompileShader", "vs_ok=%d fs_ok=%d vs_log=%s", vs_ok, fs_ok, log);
    uint32_t prog = cp();
    at(prog, vs); at(prog, fs);
    lp(prog);
    int link_ok = 0;
    gpi(prog, GL_LINK_STATUS, &link_ok);
    if (gpil && !link_ok) { int n = 0; gpil(prog, 512, &n, log); }
    diff_log_step(case_id, (*step)++, "glLinkProgram", "link_ok=%d log=%s", link_ok, log);
    if (ds) { ds(vs); ds(fs); }
    if (!link_ok && dp) dp(prog);
    return prog;
}

/* b01s：ES shader 编译链接透传（T1 纯色模板） */
static void case_b01s(const char* case_id, int* step_out) {
    int step = *step_out;
    uint32_t prog = build_program_es(case_id, &step, T1_VS_320, T1_FS_320);
    diff_log_step(case_id, step++, "glCreateProgram", "prog=%u", prog);
    if (prog) {
        typedef void (*useProgram_t)(uint32_t);
        typedef int (*getUniformLocation_t)(uint32_t, const char*);
        useProgram_t up = (useProgram_t)g_fn("glUseProgram");
        getUniformLocation_t gul = (getUniformLocation_t)g_fn("glGetUniformLocation");
        if (up) up(prog);
        diff_log_step(case_id, step++, "glUseProgram", "prog=%u", prog);
        if (gul) {
            int loc = gul(prog, "uColor");
            diff_log_step(case_id, step++, "glGetUniformLocation", "uColor=%d", loc);
        }
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* a01s：状态机子集（BLEND/DEPTH_TEST/CULL_FACE/SCISSOR_TEST 直通 cap） */
static void case_a01s(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*enable_t)(uint32_t);
    typedef void (*disable_t)(uint32_t);
    typedef uint8_t (*isEnabled_t)(uint32_t);
    enable_t en = (enable_t)g_fn("glEnable");
    disable_t dis = (disable_t)g_fn("glDisable");
    isEnabled_t ie = (isEnabled_t)g_fn("glIsEnabled");
    if (!en || !dis || !ie) {
        diff_log_step(case_id, step++, "state", "(missing)");
        *step_out = step;
        return;
    }
    const uint32_t caps[4] = { 0x0BE2 /*GL_BLEND*/, 0x0B71 /*GL_DEPTH_TEST*/,
                               0x0B44 /*GL_CULL_FACE*/, 0x0C11 /*GL_SCISSOR_TEST*/ };
    const char* names[4] = { "BLEND", "DEPTH_TEST", "CULL_FACE", "SCISSOR_TEST" };
    for (int i = 0; i < 4; i++) {
        dis(caps[i]);
        diff_log_step(case_id, step++, "glDisable", "%s", names[i]);
        uint8_t s0 = ie(caps[i]);
        diff_log_step(case_id, step++, "glIsEnabled", "%s=%u", names[i], s0);
        en(caps[i]);
        diff_log_step(case_id, step++, "glEnable", "%s", names[i]);
        uint8_t s1 = ie(caps[i]);
        diff_log_step(case_id, step++, "glIsEnabled", "%s=%u", names[i], s1);
        dis(caps[i]);
        uint8_t s2 = ie(caps[i]);
        diff_log_step(case_id, step++, "glIsEnabled", "%s=%u", names[i], s2);
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* h01s：纯色三角形像素渲染（ES T1 模板，draw 后 readPixels 哈希） */
static void case_h01s(const char* case_id, int* step_out) {
    int step = *step_out;
    const int W = 64, H = 64;
    typedef void (*genBuffers_t)(int, uint32_t*);
    typedef void (*bindBuffer_t)(uint32_t, uint32_t);
    typedef void (*bufferData_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*genVAO_t)(int, uint32_t*);
    typedef void (*bindVAO_t)(uint32_t);
    typedef void (*enableAttrib_t)(uint32_t);
    typedef void (*attribPtr_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*useProgram_t)(uint32_t);
    typedef void (*uniform4f_t)(int, float, float, float, float);
    typedef void (*drawArrays_t)(uint32_t, int, int);
    typedef void (*clear_t)(uint32_t);
    typedef void (*clearColor_t)(float, float, float, float);
    typedef void (*viewport_t)(int, int, int, int);
    genBuffers_t gb = (genBuffers_t)g_fn("glGenBuffers");
    bindBuffer_t bb = (bindBuffer_t)g_fn("glBindBuffer");
    bufferData_t bd = (bufferData_t)g_fn("glBufferData");
    genVAO_t gv = (genVAO_t)g_fn("glGenVertexArrays");
    bindVAO_t bv = (bindVAO_t)g_fn("glBindVertexArray");
    enableAttrib_t ea = (enableAttrib_t)g_fn("glEnableVertexAttribArray");
    attribPtr_t ap = (attribPtr_t)g_fn("glVertexAttribPointer");
    useProgram_t up = (useProgram_t)g_fn("glUseProgram");
    uniform4f_t u4 = (uniform4f_t)g_fn("glUniform4f");
    drawArrays_t da = (drawArrays_t)g_fn("glDrawArrays");
    clear_t cl = (clear_t)g_fn("glClear");
    clearColor_t cc = (clearColor_t)g_fn("glClearColor");
    viewport_t vp = (viewport_t)g_fn("glViewport");
    if (!gb || !bb || !bd || !gv || !bv || !ea || !ap || !up || !da) {
        diff_log_step(case_id, step++, "h01s", "(missing 函数指针)");
        *step_out = step;
        return;
    }

    uint32_t prog = build_program_es(case_id, &step, T1_VS_320, T1_FS_320);
    if (!prog) { diff_log_step(case_id, step++, "h01s", "program 构建失败"); *step_out = step; return; }

    /* 渲染目标 64x64 */
    uint32_t tex = 0, fbo = 0, rbo = 0;
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) {
        diff_log_step(case_id, step++, "h01s", "FBO 创建失败");
        *step_out = step;
        return;
    }

    if (vp) vp(0, 0, W, H);
    if (cc) cc(0.0f, 0.0f, 0.0f, 1.0f);
    if (cl) cl(0x00004000 /*GL_COLOR_BUFFER_BIT*/);

    /* 三角形：三个顶点（NDC 全屏三角） */
    float verts[6] = { -1.0f, -1.0f, 3.0f, -1.0f, -1.0f, 3.0f };
    uint32_t vbo = 0, vao = 0;
    if (gv) gv(1, &vao);
    if (bv) bv(vao);
    if (gb) gb(1, &vbo);
    if (bb) bb(GL_ARRAY_BUFFER, vbo);
    if (bd) bd(GL_ARRAY_BUFFER, sizeof(verts), verts, GL_STATIC_DRAW);
    if (ea) ea(0);
    if (ap) ap(0, 2, GL_FLOAT, 0 /*GL_FALSE*/, 0, (const void*)0);
    if (up) up(prog);
    if (u4) {
        int loc = 0;
        typedef int (*getUniformLocation_t)(uint32_t, const char*);
        getUniformLocation_t gul = (getUniformLocation_t)g_fn("glGetUniformLocation");
        if (gul) loc = gul(prog, "uColor");
        u4(loc, 0.2f, 0.4f, 0.8f, 1.0f);
    }
    if (da) da(GL_TRIANGLES, 0, 3);
    diff_log_step(case_id, step++, "glDrawArrays", "triangle drawn");

    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "hash=0x%016llX", (unsigned long long)h);

    /* 清理（VAO 删除前解绑避免残留） */
    typedef void (*deleteBuffers_t)(int, const uint32_t*);
    typedef void (*deleteVAO_t)(int, const uint32_t*);
    typedef void (*deleteTexture_t)(int, const uint32_t*);
    typedef void (*deleteFB_t)(int, const uint32_t*);
    typedef void (*deleteRB_t)(int, const uint32_t*);
    deleteBuffers_t db = (deleteBuffers_t)g_fn("glDeleteBuffers");
    deleteVAO_t dv = (deleteVAO_t)g_fn("glDeleteVertexArrays");
    deleteTexture_t dt = (deleteTexture_t)g_fn("glDeleteTextures");
    deleteFB_t df = (deleteFB_t)g_fn("glDeleteFramebuffers");
    deleteRB_t dr = (deleteRB_t)g_fn("glDeleteRenderbuffers");
    if (bv) bv(0);
    if (db) db(1, &vbo);
    if (dv) dv(1, &vao);
    if (df) df(1, &fbo);
    if (dr) dr(1, &rbo);
    if (dt) dt(1, &tex);
    diff_check_errors(case_id, step++);
    *step_out = step;
}


/* ============ T5 阶段 A 全量用例 ============ */

/* 通用 shader 版本选择：desktop/translate 喂 330 core（translate 走翻译管线），gles 喂 320 es */
static void pick_shader(const ShaderPair* p, const char** vs, const char** fs) {
    if (g_backend == BACKEND_GLES) { *vs = p->vs_320; *fs = p->fs_320; }
    else { *vs = p->vs_330; *fs = p->fs_330; }
}

/* 通用 cap 状态机辅助：每个 cap 做 D→Is→E→Is→D→Is */
static void cap_machine_cls(const char* case_id, int* step, const char* name,
                            uint32_t cap, int do_log_errors, int cls);
static void cap_machine(const char* case_id, int* step, const char* name,
                        uint32_t cap, int do_log_errors) {
    cap_machine_cls(case_id, step, name, cap, do_log_errors, -1);
}

static void cap_machine_cls(const char* case_id, int* step, const char* name,
                            uint32_t cap, int do_log_errors, int cls) {
    typedef void (*enable_t)(uint32_t);
    typedef void (*disable_t)(uint32_t);
    typedef uint8_t (*isEnabled_t)(uint32_t);
    enable_t en = (enable_t)g_fn("glEnable");
    disable_t dis = (disable_t)g_fn("glDisable");
    isEnabled_t ie = (isEnabled_t)g_fn("glIsEnabled");
    if (!en || !dis || !ie) { diff_log_step(case_id, (*step)++, name, "(missing)"); return; }
    dis(cap);
    uint8_t s0 = ie(cap);
    en(cap);
    uint8_t s1 = ie(cap);
    dis(cap);
    uint8_t s2 = ie(cap);
    if (cls >= 0) {
        diff_log_step(case_id, (*step)++, name, "E->IsEnabled=%u D->IsEnabled=%u D->IsEnabled=%u [cls=%d]",
                      s1, s0, s2, cls);
    } else {
        diff_log_step(case_id, (*step)++, name, "E->IsEnabled=%u D->IsEnabled=%u D->IsEnabled=%u",
                      s1, s0, s2);
    }
    if (do_log_errors) diff_check_errors(case_id, (*step)++);
}

/* 通用 getIntegerv 单值输出 */
static void dump_iv(const char* case_id, int* step, const char* op,
                    uint32_t pname, const char* label) {
    typedef void (*gi_t)(uint32_t, int*);
    gi_t gi = (gi_t)g_fn("glGetIntegerv");
    int v = -999;
    if (gi) gi(pname, &v);
    diff_log_step(case_id, (*step)++, op, "%s=%d", label, v);
}

/* ===== a 组：状态机 ===== */

/* a01: 直通 cap 状态机（10 个 GLES 原生 cap） */
static void case_a01(const char* case_id, int* step_out) {
    int step = *step_out;
    /* 逐个处理（避免数组初始化歧义） */
    cap_machine(case_id, &step, "BLEND", 0x0BE2, 1);
    cap_machine(case_id, &step, "DEPTH_TEST", 0x0B71, 0);
    cap_machine(case_id, &step, "CULL_FACE", 0x0B44, 0);
    cap_machine(case_id, &step, "SCISSOR_TEST", 0x0C11, 0);
    cap_machine(case_id, &step, "DITHER", 0x0BD0, 0);
    cap_machine(case_id, &step, "POLYGON_OFFSET_FILL", 0x8037, 0);
    cap_machine(case_id, &step, "SAMPLE_ALPHA_TO_COVERAGE", 0x809E, 0);
    cap_machine(case_id, &step, "SAMPLE_COVERAGE", 0x80A0, 0);
    cap_machine(case_id, &step, "SAMPLE_MASK", 0x8E51, 0);
    cap_machine(case_id, &step, "RASTERIZER_DISCARD", 0x8C89, 0);
    *step_out = step;
}

/* a02: 过滤 cap 记录（GLES 无对应，enable 被吞/忽略；IsEnabled 差异 → exp） */
static void case_a02(const char* case_id, int* step_out) {
    int step = *step_out;
    static const struct { const char* n; uint32_t c; } items[12] = {
        { "MULTISAMPLE", 0x809D }, { "PROGRAM_POINT_SIZE", 0x8642 },
        { "LINE_SMOOTH", 0x0B20 }, { "POLYGON_SMOOTH", 0x0B41 },
        { "POLYGON_STIPPLE", 0x0B42 }, { "TEXTURE_CUBE_MAP_SEAMLESS", 0x884F },
        { "ALPHA_TEST", 0x0BC0 }, { "COLOR_LOGIC_OP", 0x0BF2 },
        { "DEBUG_OUTPUT", 0x92E0 }, { "SAMPLE_ALPHA_TO_ONE", 0x809F },
        { "FRAMEBUFFER_SRGB", 0x8DB9 }, { "DEPTH_CLAMP", 0x864F },
    };
    for (int i = 0; i < 12; i++) {
        cap_machine_cls(case_id, &step, items[i].n, items[i].c, 0, 2);
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* a03: PRIMITIVE_RESTART 翻译 */
static void case_a03(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*enable_t)(uint32_t);
    typedef uint8_t (*isEnabled_t)(uint32_t);
    typedef void (*restartIndex_t)(uint32_t);
    enable_t en = (enable_t)g_fn("glEnable");
    isEnabled_t ie = (isEnabled_t)g_fn("glIsEnabled");
    restartIndex_t ri = (restartIndex_t)g_fn("glPrimitiveRestartIndex");
    if (!en || !ie) { diff_log_step(case_id, step++, "PRIMITIVE_RESTART", "(missing)"); *step_out = step; return; }
    en(0x8F9D /*GL_PRIMITIVE_RESTART*/);
    uint8_t s = ie(0x8F9D);
    diff_log_step(case_id, step++, "glIsEnabled", "PRIMITIVE_RESTART=%u", s);
    if (ri) {
        ri(0xFFFF);
        diff_log_step(case_id, step++, "glPrimitiveRestartIndex", "0xFFFF");
        ri(0x1234); /* 自定义索引：GLES 仅 FIXED_INDEX，exp */
        diff_log_step(case_id, step++, "glPrimitiveRestartIndex", "0x1234 (custom)");
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* a04: 初始状态 dump（15 pname） */
static void case_a04(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gi_t)(uint32_t, int*);
    typedef uint8_t (*gb_t)(uint32_t, uint8_t*);
    gi_t gi = (gi_t)g_fn("glGetIntegerv");
    gb_t gb = (gb_t)g_fn("glGetBooleanv");
    if (!gi) { diff_log_step(case_id, step++, "initial_state", "(missing)"); *step_out = step; return; }
    dump_iv(case_id, &step, "glGetIntegerv", 0x8B8D, "CURRENT_PROGRAM");
    dump_iv(case_id, &step, "glGetIntegerv", 0x84E0, "ACTIVE_TEXTURE");
    dump_iv(case_id, &step, "glGetIntegerv", 0x8894, "ARRAY_BUFFER_BINDING");
    dump_iv(case_id, &step, "glGetIntegerv", 0x8895, "ELEMENT_ARRAY_BUFFER_BINDING");
    dump_iv(case_id, &step, "glGetIntegerv", 0x85B5, "VERTEX_ARRAY_BINDING");
    dump_iv(case_id, &step, "glGetIntegerv", 0x8069, "TEXTURE_BINDING_2D");
    dump_iv(case_id, &step, "glGetIntegerv", 0x8CA6, "DRAW_FRAMEBUFFER_BINDING");
    dump_iv(case_id, &step, "glGetIntegerv", 0x8D65, "RENDERBUFFER_BINDING");
    dump_iv(case_id, &step, "glGetIntegerv", 0x8B8A, "ACTIVE_TEXTURE_UNITS?"); /* 占位（实际 0x84C0） */
    if (gb) {
        uint8_t b[8] = { 9,9,9,9,9,9,9,9 };
        gb(0x0BE2, b); diff_log_step(case_id, step++, "glGetBooleanv", "BLEND=%u", b[0]);
        gb(0x0B71, b); diff_log_step(case_id, step++, "glGetBooleanv", "DEPTH_TEST=%u", b[0]);
        gb(0x0B44, b); diff_log_step(case_id, step++, "glGetBooleanv", "CULL_FACE=%u", b[0]);
        gb(0x0BD0, b); diff_log_step(case_id, step++, "glGetBooleanv", "DITHER=%u", b[0]);
        gb(0x809D, b); diff_log_step(case_id, step++, "glGetBooleanv", "MULTISAMPLE=%u", b[0]);
        gb(0x8C89, b); diff_log_step(case_id, step++, "glGetBooleanv", "RASTERIZER_DISCARD=%u", b[0]);
        gb(0x809E, b); diff_log_step(case_id, step++, "glGetBooleanv", "SAMPLE_ALPHA_TO_COVERAGE=%u", b[0]);
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* a05: Enablei 合法 + 越界 */
static void case_a05(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*enablei_t)(uint32_t, uint32_t);
    typedef void (*disablei_t)(uint32_t, uint32_t);
    typedef uint8_t (*isEnabledi_t)(uint32_t, uint32_t);
    enablei_t eni = (enablei_t)g_fn("glEnablei");
    disablei_t dni = (disablei_t)g_fn("glDisablei");
    isEnabledi_t iei = (isEnabledi_t)g_fn("glIsEnabledi");
    if (!eni || !dni || !iei) { diff_log_step(case_id, step++, "Enablei", "(missing)"); *step_out = step; return; }
    eni(0x0BE2 /*BLEND*/, 0);
    uint8_t s = iei(0x0BE2, 0);
    diff_log_step(case_id, step++, "glIsEnabledi", "BLEND[0]=%u", s);
    dni(0x0BE2, 0);
    s = iei(0x0BE2, 0);
    diff_log_step(case_id, step++, "glIsEnabledi", "BLEND[0]=%u", s);
    /* 越界索引 16：桌面 INVALID_VALUE；GLES 同 */
    eni(0x0BE2, 16);
    diff_log_step(case_id, step++, "glEnablei", "BLEND[16] (越界)");
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* a06: Clear 颜色/深度/模板设置后读回 */
static void case_a06(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cd_t)(double);
    typedef void (*cs_t)(int);
    typedef void (*gf_t)(uint32_t, float*);
    cc_t cc = (cc_t)g_fn("glClearColor");
    cd_t cd = (cd_t)g_fn("glClearDepth");
    cs_t cs = (cs_t)g_fn("glClearStencil");
    gf_t gf = (gf_t)g_fn("glGetFloatv");
    typedef void (*gi_t)(uint32_t, int*);
    gi_t gi = (gi_t)g_fn("glGetIntegerv");
    if (cc && gf) {
        cc(0.1f, 0.2f, 0.3f, 0.4f);
        float v[4] = { 0, 0, 0, 0 };
        gf(0x1575 /*GL_COLOR_CLEAR_VALUE*/, v);
        diff_log_step(case_id, step++, "glGetFloatv", "COLOR_CLEAR_VALUE=(%g,%g,%g,%g)", v[0], v[1], v[2], v[3]);
    }
    if (cd && gf) {
        cd(0.5);
        float v = 0;
        gf(0x0B55 /*GL_DEPTH_CLEAR_VALUE*/, &v);
        diff_log_step(case_id, step++, "glGetFloatv", "DEPTH_CLEAR_VALUE=%g", v);
    }
    if (cs && gi) {
        cs(7);
        int v = 0;
        gi(0x0B96 /*GL_STENCIL_CLEAR_VALUE*/, &v);
        diff_log_step(case_id, step++, "glGetIntegerv", "STENCIL_CLEAR_VALUE=%d", v);
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* ===== b 组：buffer ===== */

/* b01: 生命周期 + IsBuffer */
static void case_b01(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef uint8_t (*is_t)(uint32_t);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*del_t)(int, const uint32_t*);
    gen_t gen = (gen_t)g_fn("glGenBuffers");
    is_t is = (is_t)g_fn("glIsBuffer");
    bind_t bind = (bind_t)g_fn("glBindBuffer");
    del_t del = (del_t)g_fn("glDeleteBuffers");
    if (!gen || !is || !bind || !del) { diff_log_step(case_id, step++, "b01", "(missing)"); *step_out = step; return; }
    uint32_t b = 0;
    gen(1, &b);
    uint8_t s0 = is(b);
    bind(GL_ARRAY_BUFFER, b);
    uint8_t s1 = is(b);
    del(1, &b);
    uint8_t s2 = is(b);
    diff_log_step(case_id, step++, "b01_lifecycle", "is_after_gen=%u is_after_bind=%u is_after_del=%u", s0, s1, s2);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* b02: BufferData + SIZE/USAGE */
static void case_b02(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*data_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*param_t)(uint32_t, uint32_t, int*);
    gen_t gen = (gen_t)g_fn("glGenBuffers");
    bind_t bind = (bind_t)g_fn("glBindBuffer");
    data_t data = (data_t)g_fn("glBufferData");
    param_t param = (param_t)g_fn("glGetBufferParameteriv");
    if (!gen || !bind || !data || !param) { diff_log_step(case_id, step++, "b02", "(missing)"); *step_out = step; return; }
    uint32_t b = 0;
    gen(1, &b); bind(GL_ARRAY_BUFFER, b);
    data(GL_ARRAY_BUFFER, 512, NULL, GL_DYNAMIC_DRAW);
    int sz = 0, us = 0;
    param(GL_ARRAY_BUFFER, GL_BUFFER_SIZE, &sz);
    param(GL_ARRAY_BUFFER, GL_BUFFER_USAGE, &us);
    diff_log_step(case_id, step++, "glGetBufferParameteriv", "SIZE=%d USAGE=0x%04X", sz, us);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* b03: SubData 写 pattern + 读回哈希 */
static void case_b03(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*data_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*sub_t)(uint32_t, intptr_t, intptr_t, const void*);
    typedef void (*getsub_t)(uint32_t, intptr_t, intptr_t, void*);
    gen_t gen = (gen_t)g_fn("glGenBuffers");
    bind_t bind = (bind_t)g_fn("glBindBuffer");
    data_t data = (data_t)g_fn("glBufferData");
    sub_t sub = (sub_t)g_fn("glBufferSubData");
    getsub_t getsub = (getsub_t)g_fn("glGetBufferSubData");
    if (!gen || !bind || !data || !sub || !getsub) { diff_log_step(case_id, step++, "b03", "(missing)"); *step_out = step; return; }
    uint8_t pat[128];
    for (int i = 0; i < 128; i++) pat[i] = (uint8_t)(i * 7 + 3);
    uint32_t b = 0;
    gen(1, &b); bind(GL_ARRAY_BUFFER, b);
    data(GL_ARRAY_BUFFER, 256, NULL, GL_STATIC_DRAW);
    sub(GL_ARRAY_BUFFER, 0, 128, pat);
    uint8_t rd[128] = { 0 };
    getsub(GL_ARRAY_BUFFER, 0, 128, rd);
    uint64_t h = diff_fnv1a64(rd, 128);
    diff_log_step(case_id, step++, "glGetBufferSubData", "hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* b04: MapBufferRange 写 + Flush + Unmap + 读回 */
static void case_b04(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*data_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void* (*map_t)(uint32_t, intptr_t, intptr_t, uint32_t);
    typedef uint8_t (*unmap_t)(uint32_t);
    typedef void (*flush_t)(uint32_t, intptr_t, intptr_t);
    typedef void (*getsub_t)(uint32_t, intptr_t, intptr_t, void*);
    gen_t gen = (gen_t)g_fn("glGenBuffers");
    bind_t bind = (bind_t)g_fn("glBindBuffer");
    data_t data = (data_t)g_fn("glBufferData");
    map_t map = (map_t)g_fn("glMapBufferRange");
    unmap_t unmap = (unmap_t)g_fn("glUnmapBuffer");
    flush_t flush = (flush_t)g_fn("glFlushMappedBufferRange");
    getsub_t getsub = (getsub_t)g_fn("glGetBufferSubData");
    if (!gen || !bind || !data || !map || !unmap || !flush || !getsub) {
        diff_log_step(case_id, step++, "b04", "(missing)"); *step_out = step; return;
    }
    uint32_t b = 0;
    gen(1, &b); bind(GL_ARRAY_BUFFER, b);
    data(GL_ARRAY_BUFFER, 256, NULL, GL_DYNAMIC_DRAW);
    void* p = map(GL_ARRAY_BUFFER, 0, 256, 0x0002 /*GL_MAP_WRITE_BIT*/);
    if (p) {
        uint8_t* bp = (uint8_t*)p;
        for (int i = 0; i < 256; i++) bp[i] = (uint8_t)(255 - i);
        flush(GL_ARRAY_BUFFER, 0, 256);
        uint8_t u = unmap(GL_ARRAY_BUFFER);
        uint8_t rd[256] = { 0 };
        getsub(GL_ARRAY_BUFFER, 0, 256, rd);
        uint64_t h = diff_fnv1a64(rd, 256);
        diff_log_step(case_id, step++, "b04_map_flush", "unmap=%u readback_hash=0x%016llX", u, (unsigned long long)h);
    } else {
        diff_log_step(case_id, step++, "b04_map_flush", "map 返回 NULL");
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* b05: MapBufferRange(WRITE|INVALIDATE_BUFFER) 直写 + Unmap 读回（无 flush） */
static void case_b05(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*data_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void* (*map_t)(uint32_t, intptr_t, intptr_t, uint32_t);
    typedef uint8_t (*unmap_t)(uint32_t);
    typedef void (*getsub_t)(uint32_t, intptr_t, intptr_t, void*);
    gen_t gen = (gen_t)g_fn("glGenBuffers");
    bind_t bind = (bind_t)g_fn("glBindBuffer");
    data_t data = (data_t)g_fn("glBufferData");
    map_t map = (map_t)g_fn("glMapBufferRange");
    unmap_t unmap = (unmap_t)g_fn("glUnmapBuffer");
    getsub_t getsub = (getsub_t)g_fn("glGetBufferSubData");
    if (!gen || !bind || !data || !map || !unmap || !getsub) {
        diff_log_step(case_id, step++, "b05", "(missing)"); *step_out = step; return;
    }
    uint32_t b = 0;
    gen(1, &b); bind(GL_ARRAY_BUFFER, b);
    data(GL_ARRAY_BUFFER, 64, NULL, GL_DYNAMIC_DRAW);
    void* p = map(GL_ARRAY_BUFFER, 0, 64, 0x0002 | 0x0008 /*WRITE|INVALIDATE_BUFFER*/);
    if (p) {
        uint8_t* bp = (uint8_t*)p;
        for (int i = 0; i < 64; i++) bp[i] = (uint8_t)(i * 3);
        unmap(GL_ARRAY_BUFFER);
        uint8_t rd[64] = { 0 };
        getsub(GL_ARRAY_BUFFER, 0, 64, rd);
        uint64_t h = diff_fnv1a64(rd, 64);
        diff_log_step(case_id, step++, "b05_invalidate", "readback_hash=0x%016llX", (unsigned long long)h);
    } else {
        diff_log_step(case_id, step++, "b05_invalidate", "map 返回 NULL");
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* b06: BindBufferBase/Range + GetIntegeri_v 回读 */
static void case_b06(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bindbase_t)(uint32_t, uint32_t, uint32_t);
    typedef void (*bindrange_t)(uint32_t, uint32_t, uint32_t, intptr_t, intptr_t);
    typedef void (*gii_t)(uint32_t, uint32_t, int*);
    gen_t gen = (gen_t)g_fn("glGenBuffers");
    bindbase_t bb = (bindbase_t)g_fn("glBindBufferBase");
    bindrange_t br = (bindrange_t)g_fn("glBindBufferRange");
    gii_t gii = (gii_t)g_fn("glGetIntegeri_v");
    if (!gen || !bb || !gii) { diff_log_step(case_id, step++, "b06", "(missing)"); *step_out = step; return; }
    uint32_t b = 0;
    gen(1, &b);
    bb(0x8A11 /*GL_UNIFORM_BUFFER*/, 0, b);
    int v = -1;
    if (gii) gii(0x8A28 /*GL_UNIFORM_BUFFER_BINDING*/, 0, &v);
    diff_log_step(case_id, step++, "glGetIntegeri_v", "UBO_BINDING[0]=%d", v);
    if (br) br(0x8A11, 1, b, 0, 128);
    if (gii) gii(0x8A28, 1, &v);
    diff_log_step(case_id, step++, "glGetIntegeri_v", "UBO_BINDING[1]=%d", v);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* b07: CopyBufferSubData 读回 */
static void case_b07(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*data_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*copy_t)(uint32_t, uint32_t, intptr_t, intptr_t, intptr_t);
    typedef void (*getsub_t)(uint32_t, intptr_t, intptr_t, void*);
    gen_t gen = (gen_t)g_fn("glGenBuffers");
    bind_t bind = (bind_t)g_fn("glBindBuffer");
    data_t data = (data_t)g_fn("glBufferData");
    copy_t copy = (copy_t)g_fn("glCopyBufferSubData");
    getsub_t getsub = (getsub_t)g_fn("glGetBufferSubData");
    if (!gen || !bind || !data || !copy || !getsub) { diff_log_step(case_id, step++, "b07", "(missing)"); *step_out = step; return; }
    uint8_t pat[64];
    for (int i = 0; i < 64; i++) pat[i] = (uint8_t)(i + 1);
    uint32_t src = 0, dst = 0;
    gen(1, &src); bind(0x8F36 /*GL_COPY_READ_BUFFER*/, src);
    data(0x8F36, 64, pat, GL_STATIC_DRAW);
    gen(1, &dst); bind(0x8F37 /*GL_COPY_WRITE_BUFFER*/, dst);
    data(0x8F37, 64, NULL, GL_STATIC_DRAW);
    copy(0x8F36, 0x8F37, 0, 0, 64);
    uint8_t rd[64] = { 0 };
    getsub(0x8F37, 0, 64, rd);
    uint64_t h = diff_fnv1a64(rd, 64);
    diff_log_step(case_id, step++, "glCopyBufferSubData", "readback_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* b08: SubData 部分覆盖读回 */
static void case_b08(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*data_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*sub_t)(uint32_t, intptr_t, intptr_t, const void*);
    typedef void (*getsub_t)(uint32_t, intptr_t, intptr_t, void*);
    gen_t gen = (gen_t)g_fn("glGenBuffers");
    bind_t bind = (bind_t)g_fn("glBindBuffer");
    data_t data = (data_t)g_fn("glBufferData");
    sub_t sub = (sub_t)g_fn("glBufferSubData");
    getsub_t getsub = (getsub_t)g_fn("glGetBufferSubData");
    if (!gen || !bind || !data || !sub || !getsub) { diff_log_step(case_id, step++, "b08", "(missing)"); *step_out = step; return; }
    uint8_t init[128];
    for (int i = 0; i < 128; i++) init[i] = 0xFF;
    uint8_t ovr[64];
    for (int i = 0; i < 64; i++) ovr[i] = (uint8_t)(i);
    uint32_t b = 0;
    gen(1, &b); bind(GL_ARRAY_BUFFER, b);
    data(GL_ARRAY_BUFFER, 128, init, GL_STATIC_DRAW);
    sub(GL_ARRAY_BUFFER, 32, 64, ovr);
    uint8_t rd[128] = { 0 };
    getsub(GL_ARRAY_BUFFER, 0, 128, rd);
    uint64_t h = diff_fnv1a64(rd, 128);
    diff_log_step(case_id, step++, "glGetBufferSubData", "partial_overwrite_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* b09: 删除已绑定 buffer 后绑定查询（应自动解绑为 0） */
static void case_b09(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*data_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*del_t)(int, const uint32_t*);
    typedef void (*gi_t)(uint32_t, int*);
    gen_t gen = (gen_t)g_fn("glGenBuffers");
    bind_t bind = (bind_t)g_fn("glBindBuffer");
    data_t data = (data_t)g_fn("glBufferData");
    del_t del = (del_t)g_fn("glDeleteBuffers");
    gi_t gi = (gi_t)g_fn("glGetIntegerv");
    if (!gen || !bind || !data || !del || !gi) { diff_log_step(case_id, step++, "b09", "(missing)"); *step_out = step; return; }
    uint32_t b = 0;
    gen(1, &b); bind(GL_ARRAY_BUFFER, b);
    data(GL_ARRAY_BUFFER, 16, NULL, GL_STATIC_DRAW);
    del(1, &b); /* 删除绑定中的 buffer */
    int v = -1;
    gi(0x8894 /*GL_ARRAY_BUFFER_BINDING*/, &v);
    diff_log_step(case_id, step++, "glGetIntegerv", "ARRAY_BUFFER_BINDING_after_del=%d", v);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* ===== c 组：纹理 ===== */

/* 辅助：生成并绑定 2D 纹理 */
static uint32_t tex_new2d(const char* case_id, int* step) {
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    gen_t gen = (gen_t)g_fn("glGenTextures");
    bind_t bind = (bind_t)g_fn("glBindTexture");
    uint32_t t = 0;
    if (gen) gen(1, &t);
    if (bind) bind(GL_TEXTURE_2D, t);
    diff_log_step(case_id, (*step)++, "glGenTextures", "tex=%u", t);
    return t;
}

/* c01: 默认参数读回 */
static void case_c01(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*getp_t)(uint32_t, uint32_t, int*);
    getp_t gp = (getp_t)g_fn("glGetTexParameteriv");
    if (!gp) { diff_log_step(case_id, step++, "c01", "(missing)"); *step_out = step; return; }
    tex_new2d(case_id, &step);
    static const struct { uint32_t p; const char* n; } ps[7] = {
        { 0x2801, "MIN_FILTER" }, { 0x2800, "MAG_FILTER" },
        { 0x2802, "WRAP_S" }, { 0x2803, "WRAP_T" },
        { 0x813C, "BASE_LEVEL" }, { 0x813D, "MAX_LEVEL" },
        { 0x8E22, "SWIZZLE_R" },
    };
    for (int i = 0; i < 7; i++) {
        int v = -1;
        gp(GL_TEXTURE_2D, ps[i].p, &v);
        diff_log_step(case_id, step++, "glGetTexParameteriv", "%s=%d", ps[i].n, v);
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c02: TexImage2D RGBA8 + LevelParameteriv */
static void case_c02(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*img_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    typedef void (*getlvl_t)(uint32_t, int, uint32_t, int*);
    img_t img = (img_t)g_fn("glTexImage2D");
    getlvl_t gl = (getlvl_t)g_fn("glGetTexLevelParameteriv");
    if (!img || !gl) { diff_log_step(case_id, step++, "c02", "(missing)"); *step_out = step; return; }
    tex_new2d(case_id, &step);
    img(GL_TEXTURE_2D, 0, 0x8058 /*GL_RGBA8*/, 256, 256, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
    static const struct { uint32_t p; const char* n; } ps[5] = {
        { 0x1000, "WIDTH" }, { 0x1001, "HEIGHT" },
        { 0x1003, "INTERNAL_FORMAT" }, { 0x8C35?0:0, "x" },
    };
    int v = -1;
    gl(GL_TEXTURE_2D, 0, 0x1000, &v); diff_log_step(case_id, step++, "glGetTexLevelParameteriv", "WIDTH=%d", v);
    gl(GL_TEXTURE_2D, 0, 0x1001, &v); diff_log_step(case_id, step++, "glGetTexLevelParameteriv", "HEIGHT=%d", v);
    gl(GL_TEXTURE_2D, 0, 0x1003, &v); diff_log_step(case_id, step++, "glGetTexLevelParameteriv", "INTERNAL_FORMAT=0x%04X", v);
    gl(GL_TEXTURE_2D, 0, 0x805C /*GL_RED_SIZE*/, &v); diff_log_step(case_id, step++, "glGetTexLevelParameteriv", "RED_SIZE=%d", v);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c03: unsized RGB 归一化（INTERNAL_FORMAT 对比） */
static void case_c03(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*img_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    typedef void (*getlvl_t)(uint32_t, int, uint32_t, int*);
    img_t img = (img_t)g_fn("glTexImage2D");
    getlvl_t gl = (getlvl_t)g_fn("glGetTexLevelParameteriv");
    if (!img || !gl) { diff_log_step(case_id, step++, "c03", "(missing)"); *step_out = step; return; }
    tex_new2d(case_id, &step);
    img(GL_TEXTURE_2D, 0, 0x1907 /*GL_RGB unsized*/, 64, 64, 0, 0x1907, GL_UNSIGNED_BYTE, NULL);
    int v = -1;
    gl(GL_TEXTURE_2D, 0, 0x1003, &v);
    diff_log_step(case_id, step++, "glGetTexLevelParameteriv", "unsized_RGB_INTERNAL_FORMAT=0x%04X", v);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c04: RGBA16F + HALF_FLOAT */
static void case_c04(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*img_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    typedef void (*getlvl_t)(uint32_t, int, uint32_t, int*);
    img_t img = (img_t)g_fn("glTexImage2D");
    getlvl_t gl = (getlvl_t)g_fn("glGetTexLevelParameteriv");
    if (!img || !gl) { diff_log_step(case_id, step++, "c04", "(missing)"); *step_out = step; return; }
    uint16_t px[4] = { 0x3C00 /*1.0*/, 0x0000, 0x3800 /*0.5*/, 0x3C00 };
    tex_new2d(case_id, &step);
    img(GL_TEXTURE_2D, 0, 0x881A /*GL_RGBA16F*/, 2, 2, 0, GL_RGBA, 0x140B /*GL_HALF_FLOAT*/, px);
    int v = -1;
    gl(GL_TEXTURE_2D, 0, 0x1003, &v);
    diff_log_step(case_id, step++, "glGetTexLevelParameteriv", "RGBA16F_INTERNAL_FORMAT=0x%04X", v);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c05: TexParameteri 6 项设置回读 */
static void case_c05(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*set_t)(uint32_t, uint32_t, int);
    typedef void (*get_t)(uint32_t, uint32_t, int*);
    set_t set = (set_t)g_fn("glTexParameteri");
    get_t get = (get_t)g_fn("glGetTexParameteriv");
    if (!set || !get) { diff_log_step(case_id, step++, "c05", "(missing)"); *step_out = step; return; }
    tex_new2d(case_id, &step);
    static const struct { uint32_t p; int v; const char* n; } items[6] = {
        { 0x2801, 0x2600 /*NEAREST*/, "MIN_FILTER" },
        { 0x2800, 0x2601 /*LINEAR*/, "MAG_FILTER" },
        { 0x2802, 0x812F /*CLAMP_TO_EDGE*/, "WRAP_S" },
        { 0x2803, 0x2901 /*REPEAT*/, "WRAP_T" },
        { 0x813C, 1, "BASE_LEVEL" },
        { 0x813D, 4, "MAX_LEVEL" },
    };
    for (int i = 0; i < 6; i++) {
        set(GL_TEXTURE_2D, items[i].p, items[i].v);
        int v = -1;
        get(GL_TEXTURE_2D, items[i].p, &v);
        diff_log_step(case_id, step++, "glGetTexParameteriv", "%s=%d", items[i].n, v);
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c06: TexSubImage2D 局部更新 + 读回（GetTexImage 或 map） */
static void case_c06(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*img_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    typedef void (*sub_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    img_t img = (img_t)g_fn("glTexImage2D");
    sub_t sub = (sub_t)g_fn("glTexSubImage2D");
    if (!img || !sub) { diff_log_step(case_id, step++, "c06", "(missing)"); *step_out = step; return; }
    uint8_t init[16 * 16 * 4];
    for (int i = 0; i < 16 * 16 * 4; i++) init[i] = 0x11;
    uint8_t patch[8 * 8 * 4];
    for (int i = 0; i < 8 * 8 * 4; i++) patch[i] = 0xAA;
    tex_new2d(case_id, &step);
    img(GL_TEXTURE_2D, 0, GL_RGBA8, 16, 16, 0, GL_RGBA, GL_UNSIGNED_BYTE, init);
    sub(GL_TEXTURE_2D, 0, 4, 4, 8, 8, GL_RGBA, GL_UNSIGNED_BYTE, patch);
    diff_log_step(case_id, step++, "glTexSubImage2D", "patch 8x8 @ (4,4)");
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c07: GenerateMipmap 状态（仅记录，无读回） */
static void case_c07(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*img_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    typedef void (*gen_t)(uint32_t);
    img_t img = (img_t)g_fn("glTexImage2D");
    gen_t gen = (gen_t)g_fn("glGenerateMipmap");
    if (!img || !gen) { diff_log_step(case_id, step++, "c07", "(missing)"); *step_out = step; return; }
    uint8_t px[32 * 32 * 4];
    for (int i = 0; i < 32 * 32 * 4; i++) px[i] = (uint8_t)(i % 256);
    tex_new2d(case_id, &step);
    img(GL_TEXTURE_2D, 0, GL_RGBA8, 32, 32, 0, GL_RGBA, GL_UNSIGNED_BYTE, px);
    gen(GL_TEXTURE_2D);
    diff_log_step(case_id, step++, "glGenerateMipmap", "called");
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c08: 错误路径（非法 target / 负 level / ActiveTexture 越界） */
static void case_c08(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*img_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    typedef void (*at_t)(uint32_t);
    img_t img = (img_t)g_fn("glTexImage2D");
    at_t at = (at_t)g_fn("glActiveTexture");
    if (img) {
        img(0xFFFF /*非法 target*/, 0, GL_RGBA8, 4, 4, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
        diff_log_step(case_id, step++, "glTexImage2D", "非法 target 0xFFFF");
        img(GL_TEXTURE_2D, -1 /*负 level*/, GL_RGBA8, 4, 4, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
        diff_log_step(case_id, step++, "glTexImage2D", "负 level -1");
    }
    if (at) {
        at(0x84C0 + 32 /*GL_TEXTURE32，越界（上限 MAX_COMBINED）*/);
        diff_log_step(case_id, step++, "glActiveTexture", "GL_TEXTURE32 (越界)");
        at(0x84C0); /* 恢复 GL_TEXTURE0 */
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c09: TEXTURE_COMPARE_MODE 设置回读 */
static void case_c09(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*set_t)(uint32_t, uint32_t, int);
    typedef void (*get_t)(uint32_t, uint32_t, int*);
    set_t set = (set_t)g_fn("glTexParameteri");
    get_t get = (get_t)g_fn("glGetTexParameteriv");
    if (!set || !get) { diff_log_step(case_id, step++, "c09", "(missing)"); *step_out = step; return; }
    tex_new2d(case_id, &step);
    set(GL_TEXTURE_2D, 0x884C /*GL_TEXTURE_COMPARE_MODE*/, 0x884E /*GL_COMPARE_REF_TO_TEXTURE*/);
    int v = -1;
    get(GL_TEXTURE_2D, 0x884C, &v);
    diff_log_step(case_id, step++, "glGetTexParameteriv", "COMPARE_MODE=0x%04X", v);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c10: SWIZZLE 设置回读 */
static void case_c10(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*set_t)(uint32_t, uint32_t, int);
    typedef void (*get_t)(uint32_t, uint32_t, int*);
    set_t set = (set_t)g_fn("glTexParameteri");
    get_t get = (get_t)g_fn("glGetTexParameteriv");
    if (!set || !get) { diff_log_step(case_id, step++, "c10", "(missing)"); *step_out = step; return; }
    tex_new2d(case_id, &step);
    set(GL_TEXTURE_2D, 0x8E22 /*GL_TEXTURE_SWIZZLE_R*/, 0x1903 /*GL_GREEN*/);
    set(GL_TEXTURE_2D, 0x8E23, 0x1902 /*GL_BLUE*/);
    int v = -1;
    get(GL_TEXTURE_2D, 0x8E22, &v); diff_log_step(case_id, step++, "glGetTexParameteriv", "SWIZZLE_R=0x%04X", v);
    get(GL_TEXTURE_2D, 0x8E23, &v); diff_log_step(case_id, step++, "glGetTexParameteriv", "SWIZZLE_G=0x%04X", v);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c11: cubemap 6 面上传 + 参数 */
static void case_c11(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*img_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    gen_t gen = (gen_t)g_fn("glGenTextures");
    bind_t bind = (bind_t)g_fn("glBindTexture");
    img_t img = (img_t)g_fn("glTexImage2D");
    if (!gen || !bind || !img) { diff_log_step(case_id, step++, "c11", "(missing)"); *step_out = step; return; }
    uint8_t px[8 * 8 * 4];
    for (int i = 0; i < 8 * 8 * 4; i++) px[i] = (uint8_t)(i / 4 % 256);
    uint32_t t = 0;
    gen(1, &t);
    bind(0x8513 /*GL_TEXTURE_CUBE_MAP*/, t);
    for (int f = 0; f < 6; f++) {
        img(0x8515 + f, 0, GL_RGBA8, 8, 8, 0, GL_RGBA, GL_UNSIGNED_BYTE, px);
    }
    diff_log_step(case_id, step++, "glTexImage2D", "cubemap 6 faces 8x8");
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* c12: 删除纹理后绑定查询 */
static void case_c12(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*del_t)(int, const uint32_t*);
    typedef void (*gi_t)(uint32_t, int*);
    gen_t gen = (gen_t)g_fn("glGenTextures");
    bind_t bind = (bind_t)g_fn("glBindTexture");
    del_t del = (del_t)g_fn("glDeleteTextures");
    gi_t gi = (gi_t)g_fn("glGetIntegerv");
    if (!gen || !bind || !del || !gi) { diff_log_step(case_id, step++, "c12", "(missing)"); *step_out = step; return; }
    uint32_t t = 0;
    gen(1, &t);
    bind(GL_TEXTURE_2D, t);
    del(1, &t);
    int v = -1;
    gi(0x8069 /*GL_TEXTURE_BINDING_2D*/, &v);
    diff_log_step(case_id, step++, "glGetIntegerv", "TEXTURE_BINDING_2D_after_del=%d", v);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* ===== d 组：VAO ===== */

/* d01: 生命周期 + 默认无 enabled attrib */
static void case_d01(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t);
    typedef uint8_t (*is_t)(uint32_t);
    typedef void (*gva_t)(uint32_t, uint32_t, int*);
    gen_t gen = (gen_t)g_fn("glGenVertexArrays");
    bind_t bind = (bind_t)g_fn("glBindVertexArray");
    is_t is = (is_t)g_fn("glIsVertexArray");
    gva_t gva = (gva_t)g_fn("glGetVertexAttribiv");
    if (!gen || !bind || !is || !gva) { diff_log_step(case_id, step++, "d01", "(missing)"); *step_out = step; return; }
    uint32_t vao = 0;
    gen(1, &vao);
    uint8_t s0 = is(vao);
    bind(vao);
    uint8_t s1 = is(vao);
    int en = -1;
    gva(0, 0x8622 /*GL_VERTEX_ATTRIB_ARRAY_ENABLED*/, &en);
    diff_log_step(case_id, step++, "d01_vao", "is_after_gen=%u is_after_bind=%u attrib0_enabled=%d", s0, s1, en);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* d02: VertexAttribPointer + Enable + 参数读回 */
static void case_d02(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t);
    typedef void (*gvb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*data_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ea_t)(uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*gva_t)(uint32_t, uint32_t, int*);
    gen_t gen = (gen_t)g_fn("glGenVertexArrays");
    bind_t bind = (bind_t)g_fn("glBindVertexArray");
    gvb_t gvb = (gvb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    data_t data = (data_t)g_fn("glBufferData");
    ea_t ea = (ea_t)g_fn("glEnableVertexAttribArray");
    ap_t ap = (ap_t)g_fn("glVertexAttribPointer");
    gva_t gva = (gva_t)g_fn("glGetVertexAttribiv");
    if (!gen || !bind || !gvb || !bv || !data || !ea || !ap || !gva) {
        diff_log_step(case_id, step++, "d02", "(missing)"); *step_out = step; return;
    }
    float v[16] = { 0 };
    uint32_t vao = 0, buf = 0;
    gen(1, &vao); bind(vao);
    gvb(1, &buf); bv(GL_ARRAY_BUFFER, buf);
    data(GL_ARRAY_BUFFER, 64, v, GL_STATIC_DRAW);
    ap(0, 3, GL_FLOAT, 0, 16, (const void*)4);
    ea(0);
    int vals[4] = { -1, -1, -1, -1 };
    gva(0, 0x8622, &vals[0]); /* ENABLED */
    gva(0, 0x8623, &vals[1]); /* SIZE */
    gva(0, 0x8624, &vals[2]); /* STRIDE */
    gva(0, 0x8625, &vals[3]); /* TYPE */
    diff_log_step(case_id, step++, "glGetVertexAttribiv",
                  "ENABLED=%d SIZE=%d STRIDE=%d TYPE=0x%04X", vals[0], vals[1], vals[2], vals[3]);
    gva(0, 0x886A /*GL_VERTEX_ATTRIB_ARRAY_NORMALIZED*/, &vals[0]);
    gva(0, 0x8645 /*GL_VERTEX_ATTRIB_ARRAY_POINTER*/, &vals[1]);
    diff_log_step(case_id, step++, "glGetVertexAttribiv", "NORMALIZED=%d POINTER=%d", vals[0], vals[1]);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* d03: VertexAttribIPointer */
static void case_d03(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t);
    typedef void (*gvb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*data_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, int, const void*);
    typedef void (*gva_t)(uint32_t, uint32_t, int*);
    gen_t gen = (gen_t)g_fn("glGenVertexArrays");
    bind_t bind = (bind_t)g_fn("glBindVertexArray");
    gvb_t gvb = (gvb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    data_t data = (data_t)g_fn("glBufferData");
    ap_t ap = (ap_t)g_fn("glVertexAttribIPointer");
    gva_t gva = (gva_t)g_fn("glGetVertexAttribiv");
    if (!gen || !bind || !gvb || !bv || !data || !ap || !gva) {
        diff_log_step(case_id, step++, "d03", "(missing)"); *step_out = step; return;
    }
    int32_t v[8] = { 1, 2, 3, 4, 5, 6, 7, 8 };
    uint32_t vao = 0, buf = 0;
    gen(1, &vao); bind(vao);
    gvb(1, &buf); bv(GL_ARRAY_BUFFER, buf);
    data(GL_ARRAY_BUFFER, sizeof(v), v, GL_STATIC_DRAW);
    ap(0, 4, 0x1404 /*GL_INT*/, 0, (const void*)0);
    int tp = -1, sz = -1;
    gva(0, 0x8625, &tp);
    gva(0, 0x8623, &sz);
    diff_log_step(case_id, step++, "glGetVertexAttribiv", "IPOINTER_TYPE=0x%04X SIZE=%d", tp, sz);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* d04: VertexAttribDivisor */
static void case_d04(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*div_t)(uint32_t, uint32_t);
    typedef void (*gva_t)(uint32_t, uint32_t, int*);
    div_t div = (div_t)g_fn("glVertexAttribDivisor");
    gva_t gva = (gva_t)g_fn("glGetVertexAttribiv");
    if (!div || !gva) { diff_log_step(case_id, step++, "d04", "(missing)"); *step_out = step; return; }
    div(0, 2);
    int v = -1;
    gva(0, 0x88FE /*GL_VERTEX_ATTRIB_ARRAY_DIVISOR*/, &v);
    diff_log_step(case_id, step++, "glGetVertexAttribiv", "DIVISOR=%d", v);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* d05: VertexAttrib4fv + CURRENT_VERTEX_ATTRIB */
static void case_d05(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*a4_t)(uint32_t, const float*);
    typedef void (*gfv_t)(uint32_t, uint32_t, float*);
    a4_t a4 = (a4_t)g_fn("glVertexAttrib4fv");
    gfv_t gfv = (gfv_t)g_fn("glGetVertexAttribfv");
    if (!a4 || !gfv) { diff_log_step(case_id, step++, "d05", "(missing)"); *step_out = step; return; }
    float c[4] = { 0.25f, 0.5f, 0.75f, 1.0f };
    a4(3, c);
    float out[4] = { 0, 0, 0, 0 };
    gfv(3, 0x8626 /*GL_CURRENT_VERTEX_ATTRIB*/, out);
    diff_log_step(case_id, step++, "glGetVertexAttribfv", "CURRENT=(%g,%g,%g,%g)", out[0], out[1], out[2], out[3]);
    diff_check_errors(case_id, step++);
    *step_out = step;
}



/* ===== e 组：program ===== */

/* e01: Create + IsProgram */
static void case_e01(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef uint32_t (*cp_t)(void);
    typedef uint8_t (*is_t)(uint32_t);
    cp_t cp = (cp_t)g_fn("glCreateProgram");
    is_t is = (is_t)g_fn("glIsProgram");
    if (!cp || !is) { diff_log_step(case_id, step++, "e01", "(missing)"); *step_out = step; return; }
    uint32_t p = cp();
    uint8_t s = is(p);
    diff_log_step(case_id, step++, "glIsProgram", "prog=%u is_program=%u", p, s);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* e02: 编译成功（按 backend 选 shader 版本） */
static void case_e02(const char* case_id, int* step_out) {
    int step = *step_out;
    const ShaderPair* p = &SHADER_PAIRS[0]; /* T1 纯色 */
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    diff_log_step(case_id, step++, "e02", "prog=%u", prog);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* e03: 编译失败（语法错误 → COMPILE_STATUS FALSE + INFO_LOG_LENGTH>0） */
static void case_e03(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef uint32_t (*cs_t)(uint32_t);
    typedef void (*ss_t)(uint32_t, int, const char**, const int*);
    typedef void (*cpl_t)(uint32_t);
    typedef void (*gsi_t)(uint32_t, uint32_t, int*);
    typedef void (*gsil_t)(uint32_t, int, int*, char*);
    cs_t cs = (cs_t)g_fn("glCreateShader");
    ss_t ss = (ss_t)g_fn("glShaderSource");
    cpl_t cpl = (cpl_t)g_fn("glCompileShader");
    gsi_t gsi = (gsi_t)g_fn("glGetShaderiv");
    gsil_t gsil = (gsil_t)g_fn("glGetShaderInfoLog");
    if (!cs || !ss || !cpl || !gsi) { diff_log_step(case_id, step++, "e03", "(missing)"); *step_out = step; return; }
    const char* bad = (g_backend == BACKEND_GLES)
        ? "#version 320 es\nprecision mediump float;\nvoid main(){ syntax error here }\n"
        : "#version 330 core\nvoid main(){ syntax error here }\n";
    uint32_t sh = cs(0x8B31 /*GL_VERTEX_SHADER*/);
    ss(sh, 1, &bad, NULL);
    cpl(sh);
    int ok = -1;
    gsi(sh, 0x8B81 /*GL_COMPILE_STATUS*/, &ok);
    int len = 0;
    if (gsil) gsil(sh, 0, &len, NULL); /* 查询 INFO_LOG_LENGTH */
    gsi(sh, 0x8B84 /*GL_INFO_LOG_LENGTH*/, &len);
    diff_log_step(case_id, step++, "glGetShaderiv", "COMPILE_STATUS=%d INFO_LOG_LENGTH=%d", ok, len);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* e04: link 成功 */
static void case_e04(const char* case_id, int* step_out) {
    int step = *step_out;
    const ShaderPair* p = &SHADER_PAIRS[0];
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    diff_log_step(case_id, step++, "e04", "prog=%u", prog);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* e05: link 失败（VS out float 与 FS in vec2 类型不匹配） */
static void case_e05(const char* case_id, int* step_out) {
    int step = *step_out;
    const char* vs330 = "#version 330 core\nlayout(location=0) in vec2 aP;\nout float vA;\nvoid main(){ vA=1.0; gl_Position=vec4(aP,0,1); }\n";
    const char* fs330 = "#version 330 core\nin vec2 vA;\nout vec4 c;\nvoid main(){ c=vec4(vA,0,1); }\n";
    const char* vs320 = "#version 320 es\nlayout(location=0) in vec2 aP;\nout float vA;\nvoid main(){ vA=1.0; gl_Position=vec4(aP,0,1); }\n";
    const char* fs320 = "#version 320 es\nprecision mediump float;\nin vec2 vA;\nout vec4 c;\nvoid main(){ c=vec4(vA,0,1); }\n";
    const char* vs = (g_backend == BACKEND_GLES) ? vs320 : vs330;
    const char* fs = (g_backend == BACKEND_GLES) ? fs320 : fs330;
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    diff_log_step(case_id, step++, "e05", "prog=%u (预期 link 失败)", prog);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* e06: GetUniformLocation/GetAttribLocation 值（tbd：数值可能不同，记录） */
static void case_e06(const char* case_id, int* step_out) {
    int step = *step_out;
    const ShaderPair* p = &SHADER_PAIRS[0];
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    typedef int (*gul_t)(uint32_t, const char*);
    typedef int (*gal_t)(uint32_t, const char*);
    gul_t gul = (gul_t)g_fn("glGetUniformLocation");
    gal_t gal = (gal_t)g_fn("glGetAttribLocation");
    if (gul && prog) {
        int loc = gul(prog, "uColor");
        diff_log_step(case_id, step++, "glGetUniformLocation", "uColor=%d", loc);
    }
    if (gal && prog) {
        int loc = gal(prog, "aPos");
        diff_log_step(case_id, step++, "glGetAttribLocation", "aPos=%d", loc);
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* e07: Uniform 设置 + GetUniformfv 读回（T1 纯色被引用 uniform） */
static void case_e07(const char* case_id, int* step_out) {
    int step = *step_out;
    const ShaderPair* p = &SHADER_PAIRS[0];
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    typedef void (*use_t)(uint32_t);
    typedef int (*gul_t)(uint32_t, const char*);
    typedef void (*u4_t)(int, float, float, float, float);
    typedef void (*guf_t)(uint32_t, int, float*);
    use_t use = (use_t)g_fn("glUseProgram");
    gul_t gul = (gul_t)g_fn("glGetUniformLocation");
    u4_t u4 = (u4_t)g_fn("glUniform4f");
    guf_t guf = (guf_t)g_fn("glGetUniformfv");
    if (!prog || !use || !gul || !u4 || !guf) { diff_log_step(case_id, step++, "e07", "(missing/skip)"); *step_out = step; return; }
    use(prog);
    int loc = gul(prog, "uColor");
    if (loc >= 0) {
        u4(loc, 0.1f, 0.2f, 0.3f, 0.4f);
        float out[4] = { 0, 0, 0, 0 };
        guf(prog, loc, out);
        diff_log_step(case_id, step++, "glGetUniformfv", "uColor=(%g,%g,%g,%g)", out[0], out[1], out[2], out[3]);
    } else {
        diff_log_step(case_id, step++, "e07", "uColor loc=%d (无 uniform)", loc);
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* e08: BindAttribLocation 预绑定 + link 后 GetAttribLocation */
static void case_e08(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef uint32_t (*cs_t)(uint32_t);
    typedef void (*ss_t)(uint32_t, int, const char**, const int*);
    typedef void (*cpl_t)(uint32_t);
    typedef uint32_t (*cp_t)(void);
    typedef void (*at_t)(uint32_t, uint32_t);
    typedef void (*bal_t)(uint32_t, uint32_t, const char*);
    typedef void (*lp_t)(uint32_t);
    typedef int (*gal_t)(uint32_t, const char*);
    cs_t cs = (cs_t)g_fn("glCreateShader");
    ss_t ss = (ss_t)g_fn("glShaderSource");
    cpl_t cpl = (cpl_t)g_fn("glCompileShader");
    cp_t cp = (cp_t)g_fn("glCreateProgram");
    at_t at = (at_t)g_fn("glAttachShader");
    bal_t bal = (bal_t)g_fn("glBindAttribLocation");
    lp_t lp = (lp_t)g_fn("glLinkProgram");
    gal_t gal = (gal_t)g_fn("glGetAttribLocation");
    if (!cs || !ss || !cpl || !cp || !at || !bal || !lp || !gal) {
        diff_log_step(case_id, step++, "e08", "(missing)"); *step_out = step; return;
    }
    const char* vs = (g_backend == BACKEND_GLES)
        ? "#version 320 es\nlayout(location=0) in vec2 aPos;\nvoid main(){gl_Position=vec4(aPos,0,1);}\n"
        : "#version 330 core\nlayout(location=0) in vec2 aPos;\nvoid main(){gl_Position=vec4(aPos,0,1);}\n";
    const char* fs = (g_backend == BACKEND_GLES)
        ? "#version 320 es\nprecision mediump float;\nout vec4 c;\nvoid main(){c=vec4(1);}\n"
        : "#version 330 core\nout vec4 c;\nvoid main(){c=vec4(1);}\n";
    uint32_t sh = cs(0x8B31);
    ss(sh, 1, &vs, NULL);
    cpl(sh);
    uint32_t prog = cp();
    at(prog, sh);
    bal(prog, 7, "aPos"); /* 预绑定 location 7 */
    lp(prog);
    int loc = gal(prog, "aPos");
    diff_log_step(case_id, step++, "glGetAttribLocation", "aPos_after_bind7=%d", loc);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* e09: UseProgram + CURRENT_PROGRAM */
static void case_e09(const char* case_id, int* step_out) {
    int step = *step_out;
    const ShaderPair* p = &SHADER_PAIRS[0];
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    typedef void (*use_t)(uint32_t);
    typedef void (*gi_t)(uint32_t, int*);
    use_t use = (use_t)g_fn("glUseProgram");
    gi_t gi = (gi_t)g_fn("glGetIntegerv");
    if (!prog || !use || !gi) { diff_log_step(case_id, step++, "e09", "(skip)"); *step_out = step; return; }
    use(prog);
    int v = -1;
    gi(0x8B8D /*GL_CURRENT_PROGRAM*/, &v);
    diff_log_step(case_id, step++, "glGetIntegerv", "CURRENT_PROGRAM=%d", v);
    use(0);
    gi(0x8B8D, &v);
    diff_log_step(case_id, step++, "glGetIntegerv", "CURRENT_PROGRAM_after_0=%d", v);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* e10: GetActiveUniform（tbd：数值可能不同） */
static void case_e10(const char* case_id, int* step_out) {
    int step = *step_out;
    const ShaderPair* p = &SHADER_PAIRS[0];
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    typedef void (*gau_t)(uint32_t, uint32_t, int, int*, int*, uint32_t*, char*);
    typedef void (*gpi_t)(uint32_t, uint32_t, int*);
    gau_t gau = (gau_t)g_fn("glGetActiveUniform");
    gpi_t gpi = (gpi_t)g_fn("glGetProgramiv");
    if (!prog || !gau || !gpi) { diff_log_step(case_id, step++, "e10", "(skip)"); *step_out = step; return; }
    int count = 0;
    gpi(prog, 0x8B86 /*GL_ACTIVE_UNIFORMS*/, &count);
    char name[128] = "";
    int size = 0; uint32_t type = 0;
    if (count > 0) {
        int len = 0;
        gau(prog, 0, (int)sizeof(name), &len, &size, &type, name);
        name[127] = 0;
        diff_log_step(case_id, step++, "glGetActiveUniform", "count=%d u[0]={%s size=%d type=0x%04X}", count, name, size, type);
    } else {
        diff_log_step(case_id, step++, "glGetProgramiv", "ACTIVE_UNIFORMS=%d", count);
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* ===== f 组：FBO ===== */

/* f01: 无附件 FBO → INCOMPLETE_MISSING_ATTACHMENT 对比 */
static void case_f01(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef uint32_t (*check_t)(uint32_t);
    gen_t gen = (gen_t)g_fn("glGenFramebuffers");
    bind_t bind = (bind_t)g_fn("glBindFramebuffer");
    check_t check = (check_t)g_fn("glCheckFramebufferStatus");
    if (!gen || !bind || !check) { diff_log_step(case_id, step++, "f01", "(missing)"); *step_out = step; return; }
    uint32_t fbo = 0;
    gen(1, &fbo);
    bind(GL_FRAMEBUFFER, fbo);
    uint32_t st = check(GL_FRAMEBUFFER);
    diff_log_step(case_id, step++, "glCheckFramebufferStatus", "no_attach=0x%04X", st);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* f02: color attach → COMPLETE + 渲染纯色哈希 */
static void case_f02(const char* case_id, int* step_out) {
    int step = *step_out;
    const ShaderPair* p = &SHADER_PAIRS[0];
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    if (!prog) { *step_out = step; return; }
    const int W = 64, H = 64;
    uint32_t tex = 0, fbo = 0, rbo = 0;
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) {
        diff_log_step(case_id, step++, "f02", "FBO 失败"); *step_out = step; return;
    }
    typedef void (*vp_t)(int, int, int, int);
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cl_t)(uint32_t);
    typedef void (*gb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*bd_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ea_t)(uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*use_t)(uint32_t);
    typedef int (*gul_t)(uint32_t, const char*);
    typedef void (*u4_t)(int, float, float, float, float);
    typedef void (*da_t)(uint32_t, int, int);
    vp_t vp = (vp_t)g_fn("glViewport");
    cc_t cc = (cc_t)g_fn("glClearColor");
    cl_t cl = (cl_t)g_fn("glClear");
    gb_t gb = (gb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    bd_t bd = (bd_t)g_fn("glBufferData");
    ea_t ea = (ea_t)g_fn("glEnableVertexAttribArray");
    ap_t ap = (ap_t)g_fn("glVertexAttribPointer");
    use_t use = (use_t)g_fn("glUseProgram");
    gul_t gul = (gul_t)g_fn("glGetUniformLocation");
    u4_t u4 = (u4_t)g_fn("glUniform4f");
    da_t da = (da_t)g_fn("glDrawArrays");
    if (!vp || !cc || !cl || !gb || !bv || !bd || !ea || !ap || !use || !u4 || !da) {
        diff_log_step(case_id, step++, "f02", "(missing)"); *step_out = step; return;
    }
    if (vp) vp(0, 0, W, H);
    if (cc) cc(0.0f, 0.0f, 0.0f, 1.0f);
    if (cl) cl(0x4000);
    {
        /* 桌面 core 无默认 VAO：显式创建（f02/f03 共用此段） */
        typedef void (*genvao_t)(int, uint32_t*);
        typedef void (*bindvao_t)(uint32_t);
        genvao_t gv = (genvao_t)g_fn("glGenVertexArrays");
        bindvao_t bv = (bindvao_t)g_fn("glBindVertexArray");
        uint32_t vao = 0;
        if (gv) gv(1, &vao);
        if (bv) bv(vao);
    }
    float verts[6] = { -1, -1, 3, -1, -1, 3 };
    uint32_t vbo = 0;
    gb(1, &vbo); bv(GL_ARRAY_BUFFER, vbo);
    bd(GL_ARRAY_BUFFER, sizeof(verts), verts, GL_STATIC_DRAW);
    ea(0); ap(0, 2, GL_FLOAT, 0, 0, (const void*)0);
    use(prog);
    int loc = gul ? gul(prog, "uColor") : -1;
    if (u4) u4(loc, 0.5f, 0.25f, 0.125f, 1.0f);
    da(GL_TRIANGLES, 0, 3);
    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "color_attach_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* f03: color + depth → 深度测试渲染哈希 */
static void case_f03(const char* case_id, int* step_out) {
    int step = *step_out;
    const ShaderPair* p = &SHADER_PAIRS[3]; /* T4 深度模板 */
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    if (!prog) { *step_out = step; return; }
    const int W = 64, H = 64;
    uint32_t tex = 0, fbo = 0, rbo = 0;
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) {
        diff_log_step(case_id, step++, "f03", "FBO 失败"); *step_out = step; return;
    }
    typedef void (*vp_t)(int, int, int, int);
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cl_t)(uint32_t);
    typedef void (*en_t)(uint32_t);
    typedef void (*df_t)(uint32_t);
    typedef void (*gb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*bd_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ea_t)(uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*use_t)(uint32_t);
    typedef int (*gul_t)(uint32_t, const char*);
    typedef void (*u4_t)(int, float, float, float, float);
    typedef void (*da_t)(uint32_t, int, int);
    vp_t vp = (vp_t)g_fn("glViewport");
    cc_t cc = (cc_t)g_fn("glClearColor");
    cl_t cl = (cl_t)g_fn("glClear");
    en_t en = (en_t)g_fn("glEnable");
    df_t df = (df_t)g_fn("glDepthFunc");
    gb_t gb = (gb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    bd_t bd = (bd_t)g_fn("glBufferData");
    ea_t ea = (ea_t)g_fn("glEnableVertexAttribArray");
    ap_t ap = (ap_t)g_fn("glVertexAttribPointer");
    use_t use = (use_t)g_fn("glUseProgram");
    gul_t gul = (gul_t)g_fn("glGetUniformLocation");
    u4_t u4 = (u4_t)g_fn("glUniform4f");
    da_t da = (da_t)g_fn("glDrawArrays");
    if (!vp || !cc || !cl || !en || !df || !gb || !bv || !bd || !ea || !ap || !use || !da) {
        diff_log_step(case_id, step++, "f03", "(missing)"); *step_out = step; return;
    }
    if (vp) vp(0, 0, W, H);
    if (en) en(0x0B71 /*DEPTH_TEST*/);
    if (df) df(0x0201 /*GL_LESS*/);
    if (cc) cc(0, 0, 0, 1);
    if (cl) cl(0x4000 | 0x0100 /*COLOR|DEPTH*/);
    /* 两个三角形：z=-0.5 红（先画），z=+0.5 绿（后画，深度更大被遮挡） */
    float verts[18] = { -1,-1,-0.5f, 3,-1,-0.5f, -1,3,-0.5f,  /* 红 z=-0.5 */
                         -1,-1, 0.5f, 3,-1, 0.5f, -1,3, 0.5f }; /* 绿 z=+0.5 */
    uint32_t vbo = 0;
    gb(1, &vbo); bv(GL_ARRAY_BUFFER, vbo);
    bd(GL_ARRAY_BUFFER, sizeof(verts), verts, GL_STATIC_DRAW);
    ea(0); ap(0, 3, GL_FLOAT, 0, 0, (const void*)0);
    use(prog);
    int loc = gul ? gul(prog, "uColor") : -1;
    if (u4) { u4(loc, 1.0f, 0, 0, 1.0f); da(GL_TRIANGLES, 0, 3); }
    if (u4) { u4(loc, 0, 1.0f, 0, 1.0f); da(GL_TRIANGLES, 3, 3); }
    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "depth_test_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* f04: depth-only FBO（无 color 附件） */
static void case_f04(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*fbrb_t)(uint32_t, uint32_t, uint32_t, uint32_t);
    typedef void (*rb_t)(uint32_t, uint32_t, int, int);
    typedef uint32_t (*check_t)(uint32_t);
    gen_t gen = (gen_t)g_fn("glGenFramebuffers");
    bind_t bind = (bind_t)g_fn("glBindFramebuffer");
    fbrb_t fbrb = (fbrb_t)g_fn("glFramebufferRenderbuffer");
    rb_t rbs = (rb_t)g_fn("glRenderbufferStorage");
    check_t check = (check_t)g_fn("glCheckFramebufferStatus");
    if (!gen || !bind || !fbrb || !rbs || !check) { diff_log_step(case_id, step++, "f04", "(missing)"); *step_out = step; return; }
    uint32_t fbo = 0, rbo = 0;
    gen(1, &fbo);
    bind(GL_FRAMEBUFFER, fbo);
    gen(1, &rbo);
    bind(0x8D41 /*GL_RENDERBUFFER*/, rbo);
    rbs(0x8D41, 0x81A6 /*GL_DEPTH_COMPONENT24*/, 16, 16);
    fbrb(GL_FRAMEBUFFER, 0x8D00 /*GL_DEPTH_ATTACHMENT*/, 0x8D41, rbo);
    uint32_t st = check(GL_FRAMEBUFFER);
    diff_log_step(case_id, step++, "glCheckFramebufferStatus", "depth_only=0x%04X", st);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* f05: RenderbufferStorage + GetRenderbufferParameteriv */
static void case_f05(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*rb_t)(uint32_t, uint32_t, int, int);
    typedef void (*grp_t)(uint32_t, uint32_t, int*);
    gen_t gen = (gen_t)g_fn("glGenRenderbuffers");
    bind_t bind = (bind_t)g_fn("glBindRenderbuffer");
    rb_t rbs = (rb_t)g_fn("glRenderbufferStorage");
    grp_t grp = (grp_t)g_fn("glGetRenderbufferParameteriv");
    if (!gen || !bind || !rbs || !grp) { diff_log_step(case_id, step++, "f05", "(missing)"); *step_out = step; return; }
    uint32_t rbo = 0;
    gen(1, &rbo);
    bind(0x8D41, rbo);
    rbs(0x8D41, GL_RGBA8, 32, 64);
    int w = -1, h = -1, fmt = -1;
    grp(0x8D41, 0x8DAA /*GL_RENDERBUFFER_WIDTH*/, &w);
    grp(0x8D41, 0x8DAB /*GL_RENDERBUFFER_HEIGHT*/, &h);
    grp(0x8D41, 0x8D81 /*GL_RENDERBUFFER_INTERNAL_FORMAT*/, &fmt);
    diff_log_step(case_id, step++, "glGetRenderbufferParameteriv", "W=%d H=%d FMT=0x%04X", w, h, fmt);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* f06: attach 已删纹理 → INCOMPLETE */
static void case_f06(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*img_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    typedef void (*del_t)(int, const uint32_t*);
    typedef void (*fbt_t)(uint32_t, uint32_t, uint32_t, uint32_t, int);
    typedef uint32_t (*check_t)(uint32_t);
    gen_t gen = (gen_t)g_fn("glGenTextures");
    bind_t bind = (bind_t)g_fn("glBindTexture");
    img_t img = (img_t)g_fn("glTexImage2D");
    del_t del = (del_t)g_fn("glDeleteTextures");
    fbt_t fbt = (fbt_t)g_fn("glFramebufferTexture2D");
    check_t check = (check_t)g_fn("glCheckFramebufferStatus");
    if (!gen || !bind || !img || !del || !fbt || !check) { diff_log_step(case_id, step++, "f06", "(missing)"); *step_out = step; return; }
    uint32_t tex = 0, fbo = 0;
    gen(1, &tex);
    bind(GL_TEXTURE_2D, tex);
    img(GL_TEXTURE_2D, 0, GL_RGBA8, 8, 8, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
    gen(1, &fbo);
    bind(GL_FRAMEBUFFER, fbo);
    fbt(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
    del(1, &tex); /* 删除已 attach 的纹理 */
    uint32_t st = check(GL_FRAMEBUFFER);
    diff_log_step(case_id, step++, "glCheckFramebufferStatus", "after_tex_del=0x%04X", st);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* ===== g 组：查询 ===== */

/* g01: 版本字符串（exp） */
static void case_g01(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef const char* (*gs_t)(uint32_t);
    gs_t gs = (gs_t)g_fn("glGetString");
    if (!gs) { diff_log_step(case_id, step++, "g01", "(missing)"); *step_out = step; return; }
    diff_log_step(case_id, step++, "glGetString", "VERSION=%s", gs(GL_VERSION));
    diff_log_step(case_id, step++, "glGetString", "SHADING_LANGUAGE=%s", gs(0x8B8C));
    *step_out = step;
}

/* g02: GetStringi 越界（exp） */
static void case_g02(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef const char* (*gsi_t)(uint32_t, uint32_t);
    gsi_t gsi = (gsi_t)g_fn("glGetStringi");
    if (!gsi) { diff_log_step(case_id, step++, "g02", "(missing)"); *step_out = step; return; }
    const char* e = gsi(GL_EXTENSIONS, 9999);
    diff_log_step(case_id, step++, "glGetStringi", "ext[9999]=%s", e ? e : "(null)");
    *step_out = step;
}

/* g03: 能力值 15 项（T0 实测 14/15 一致；MAX_COMBINED cap） */
static void case_g03(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gi_t)(uint32_t, int*);
    gi_t gi = (gi_t)g_fn("glGetIntegerv");
    if (!gi) { diff_log_step(case_id, step++, "g03", "(missing)"); *step_out = step; return; }
    static const struct { uint32_t p; const char* n; int cap; } caps[15] = {
        { 0x0D33, "MAX_TEXTURE_SIZE", 0 },
        { 0x8869, "MAX_VERTEX_ATTRIBS", 0 },
        { 0x8A30, "MAX_UNIFORM_BLOCK_SIZE", 0 },
        { 0x8824, "MAX_DRAW_BUFFERS", 0 },
        { 0x8CDF, "MAX_COLOR_ATTACHMENTS", 0 },
        { 0x8D57, "MAX_SAMPLES", 0 },
        { 0x0D3A, "MAX_VIEWPORT_DIMS", 0 },
        { 0x8A34, "UNIFORM_BUFFER_OFFSET_ALIGNMENT", 0 },
        { 0x8872, "MAX_TEXTURE_IMAGE_UNITS", 0 },
        { 0x8B4D, "MAX_COMBINED_TEXTURE_IMAGE_UNITS", 1 }, /* T0: 160 vs 192 */
        { 0x8B4A, "MAX_VERTEX_UNIFORM_COMPONENTS", 0 },
        { 0x8B4B, "MAX_VARYING_FLOATS", 0 },
        { 0x8B49, "MAX_FRAGMENT_UNIFORM_COMPONENTS", 0 },
        { 0x80E8, "MAX_ELEMENTS_VERTICES", 0 },
        { 0x80E9, "MAX_ELEMENTS_INDICES", 0 },
    };
    for (int i = 0; i < 15; i++) {
        int v[2] = { -1, -1 };
        gi(caps[i].p, v);
        diff_log_step(case_id, step++, "glGetIntegerv", "%s=%d [cls=%d]", caps[i].n, v[0], caps[i].cap);
    }
    *step_out = step;
}

/* g04: 状态组（绑定查询） */
static void case_g04(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gi_t)(uint32_t, int*);
    gi_t gi = (gi_t)g_fn("glGetIntegerv");
    if (!gi) { diff_log_step(case_id, step++, "g04", "(missing)"); *step_out = step; return; }
    static const struct { uint32_t p; const char* n; } ps[6] = {
        { 0x8B8D, "CURRENT_PROGRAM" },
        { 0x84E0, "ACTIVE_TEXTURE" },
        { 0x8894, "ARRAY_BUFFER_BINDING" },
        { 0x85B5, "VERTEX_ARRAY_BINDING" },
        { 0x8CA6, "DRAW_FRAMEBUFFER_BINDING" },
        { 0x8CA8, "READ_FRAMEBUFFER_BINDING" },
    };
    for (int i = 0; i < 6; i++) {
        int v = -1;
        gi(ps[i].p, &v);
        diff_log_step(case_id, step++, "glGetIntegerv", "%s=%d", ps[i].n, v);
    }
    *step_out = step;
}

/* g05: MAJOR/MINOR（exp：T3 实测桌面 4/6 vs translate 3/3） */
static void case_g05(const char* case_id, int* step_out) {
    int step = *step_out;
    dump_iv(case_id, &step, "glGetIntegerv", 0x821B, "MAJOR_VERSION [cls=2]");
    dump_iv(case_id, &step, "glGetIntegerv", 0x821C, "MINOR_VERSION [cls=2]");
    *step_out = step;
}

/* g06: PROFILE_MASK（exp） */
static void case_g06(const char* case_id, int* step_out) {
    int step = *step_out;
    dump_iv(case_id, &step, "glGetIntegerv", 0x9126, "CONTEXT_PROFILE_MASK [cls=2]");
    dump_iv(case_id, &step, "glGetIntegerv", 0x821D, "NUM_EXTENSIONS [cls=2]");
    *step_out = step;
}

/* g07: occlusion query（SAMPLES_PASSED → ANY_SAMPLES_PASSED 翻译） */
static void case_g07(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*genq_t)(int, uint32_t*);
    typedef void (*beginq_t)(uint32_t, uint32_t);
    typedef void (*endq_t)(uint32_t);
    typedef void (*gqo_t)(uint32_t, uint32_t, uint32_t*);
    genq_t genq = (genq_t)g_fn("glGenQueries");
    beginq_t bq = (beginq_t)g_fn("glBeginQuery");
    endq_t eq = (endq_t)g_fn("glEndQuery");
    gqo_t gqo = (gqo_t)g_fn("glGetQueryObjectuiv");
    if (!genq || !bq || !eq || !gqo) { diff_log_step(case_id, step++, "g07", "(missing)"); *step_out = step; return; }
    uint32_t q = 0;
    genq(1, &q);
    bq(0x8914 /*GL_SAMPLES_PASSED*/, q);
    eq(0x8914);
    uint32_t avail = 0, result = 0;
    gqo(q, 0x8867 /*GL_QUERY_RESULT_AVAILABLE*/, &avail);
    gqo(q, 0x8866 /*GL_QUERY_RESULT*/, &result);
    diff_log_step(case_id, step++, "glGetQueryObjectuiv", "AVAILABLE=%u RESULT=%u", avail, result);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* g08: 条件渲染（exp：GLES 无） */
static void case_g08(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*bcr_t)(uint32_t, uint32_t);
    typedef void (*ecr_t)(void);
    bcr_t bcr = (bcr_t)g_fn("glBeginConditionalRender");
    ecr_t ecr = (ecr_t)g_fn("glEndConditionalRender");
    if (!bcr || !ecr) { diff_log_step(case_id, step++, "g08", "conditional_render (missing/GLES 无)"); *step_out = step; return; }
    bcr(0, 0x8F10 /*GL_QUERY_WAIT*/);
    ecr();
    diff_log_step(case_id, step++, "g08", "called");
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* g09: FenceSync 生命周期 */
static void case_g09(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void* (*fs_t)(uint32_t, uint32_t);
    typedef uint32_t (*cws_t)(void*, uint32_t, uint64_t);
    typedef uint8_t (*is_t)(void*);
    typedef void (*ds_t)(void*);
    fs_t fs = (fs_t)g_fn("glFenceSync");
    cws_t cws = (cws_t)g_fn("glClientWaitSync");
    is_t is = (is_t)g_fn("glIsSync");
    ds_t ds = (ds_t)g_fn("glDeleteSync");
    if (!fs || !cws || !is || !ds) { diff_log_step(case_id, step++, "g09", "(missing)"); *step_out = step; return; }
    void* sync = fs(0x9117 /*GL_SYNC_GPU_COMMANDS_COMPLETE*/, 0);
    diff_log_step(case_id, step++, "glFenceSync", "sync=%p", sync);
    if (sync) {
        uint8_t s = is(sync);
        uint32_t r = cws(sync, 0, 0);
        diff_log_step(case_id, step++, "glClientWaitSync", "is_sync=%u wait_ret=0x%04X", s, r);
        ds(sync);
    }
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* g10: 线宽（llvmpipe must） */
static void case_g10(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*gf_t)(uint32_t, float*);
    typedef void (*lw_t)(float);
    gf_t gf = (gf_t)g_fn("glGetFloatv");
    lw_t lw = (lw_t)g_fn("glLineWidth");
    if (!gf || !lw) { diff_log_step(case_id, step++, "g10", "(missing)"); *step_out = step; return; }
    float r[2] = { 0, 0 };
    gf(0x0B22 /*GL_LINE_WIDTH_RANGE*/, r);
    diff_log_step(case_id, step++, "glGetFloatv", "LINE_WIDTH_RANGE=(%g,%g)", r[0], r[1]);
    lw(2.0f);
    float w = 0;
    gf(0x0B21 /*GL_LINE_WIDTH*/, &w);
    diff_log_step(case_id, step++, "glGetFloatv", "LINE_WIDTH=%g", w);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* g11: 错误注入（glEnable 非法 cap → INVALID_ENUM） */
static void case_g11(const char* case_id, int* step_out) {
    int step = *step_out;
    typedef void (*en_t)(uint32_t);
    en_t en = (en_t)g_fn("glEnable");
    if (!en) { diff_log_step(case_id, step++, "g11", "(missing)"); *step_out = step; return; }
    en(0xFFFF);
    diff_log_step(case_id, step++, "glEnable", "0xFFFF (非法)");
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* ===== h 组：draw（256x256，按 backend 选 shader 版本） ===== */

/* 通用渲染辅助：构建程序 + 渲染目标 + 画纯色三角形 */
static uint32_t h_render_setup(const char* case_id, int* step, int W, int H,
                               uint32_t* tex, uint32_t* fbo, uint32_t* rbo) {
    const ShaderPair* p = &SHADER_PAIRS[0];
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, step, vs, fs);
    if (!prog) return 0;
    if (diff_make_render_target(W, H, tex, fbo, rbo) != 0) {
        diff_log_step(case_id, (*step)++, "h_setup", "FBO 失败");
        return 0;
    }
    /* 创建并绑定 VAO：桌面 GL 3.3 core 无默认 VAO（无绑定 draw 报 INVALID_OPERATION），
     * GLES 3.0 有默认 VAO 0——为对齐双端行为，统一显式创建 VAO */
    typedef void (*genvao_t)(int, uint32_t*);
    typedef void (*bindvao_t)(uint32_t);
    genvao_t gv = (genvao_t)g_fn("glGenVertexArrays");
    bindvao_t bv = (bindvao_t)g_fn("glBindVertexArray");
    uint32_t vao = 0;
    if (gv) gv(1, &vao);
    if (bv) bv(vao);
    typedef void (*vp_t)(int, int, int, int);
    vp_t vp = (vp_t)g_fn("glViewport");
    if (vp) vp(0, 0, W, H);
    return prog;
}

/* h01: 纯色三角形（严格哈希） */
static void case_h01(const char* case_id, int* step_out) {
    int step = *step_out;
    const int W = 256, H = 256;
    uint32_t tex = 0, fbo = 0, rbo = 0;
    uint32_t prog = h_render_setup(case_id, &step, W, H, &tex, &fbo, &rbo);
    if (!prog) { *step_out = step; return; }
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cl_t)(uint32_t);
    typedef void (*gb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*bd_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ea_t)(uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*use_t)(uint32_t);
    typedef int (*gul_t)(uint32_t, const char*);
    typedef void (*u4_t)(int, float, float, float, float);
    typedef void (*da_t)(uint32_t, int, int);
    cc_t cc = (cc_t)g_fn("glClearColor");
    cl_t cl = (cl_t)g_fn("glClear");
    gb_t gb = (gb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    bd_t bd = (bd_t)g_fn("glBufferData");
    ea_t ea = (ea_t)g_fn("glEnableVertexAttribArray");
    ap_t ap = (ap_t)g_fn("glVertexAttribPointer");
    use_t use = (use_t)g_fn("glUseProgram");
    gul_t gul = (gul_t)g_fn("glGetUniformLocation");
    u4_t u4 = (u4_t)g_fn("glUniform4f");
    da_t da = (da_t)g_fn("glDrawArrays");
    if (!cc || !cl || !gb || !bv || !bd || !ea || !ap || !use || !u4 || !da) {
        diff_log_step(case_id, step++, "h01", "(missing)"); *step_out = step; return;
    }
    if (cc) cc(0, 0, 0, 1);
    if (cl) cl(0x4000);
    float verts[6] = { -1, -1, 3, -1, -1, 3 };
    uint32_t vbo = 0;
    gb(1, &vbo); bv(GL_ARRAY_BUFFER, vbo);
    bd(GL_ARRAY_BUFFER, sizeof(verts), verts, GL_STATIC_DRAW);
    ea(0); ap(0, 2, GL_FLOAT, 0, 0, (const void*)0);
    use(prog);
    int loc = gul ? gul(prog, "uColor") : -1;
    if (u4) u4(loc, 0.2f, 0.4f, 0.8f, 1.0f);
    da(GL_TRIANGLES, 0, 3);
    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "solid_256_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* h02: 顶点色渐变三角形（T2 模板） */
static void case_h02(const char* case_id, int* step_out) {
    int step = *step_out;
    const int W = 256, H = 256;
    const ShaderPair* p = &SHADER_PAIRS[1]; /* T2 顶点色 */
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    if (!prog) { *step_out = step; return; }
    uint32_t tex = 0, fbo = 0, rbo = 0;
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) { *step_out = step; return; }
    typedef void (*vp_t)(int, int, int, int);
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cl_t)(uint32_t);
    typedef void (*gb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*bd_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ea_t)(uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*use_t)(uint32_t);
    typedef void (*da_t)(uint32_t, int, int);
    vp_t vp = (vp_t)g_fn("glViewport");
    cc_t cc = (cc_t)g_fn("glClearColor");
    cl_t cl = (cl_t)g_fn("glClear");
    gb_t gb = (gb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    bd_t bd = (bd_t)g_fn("glBufferData");
    ea_t ea = (ea_t)g_fn("glEnableVertexAttribArray");
    ap_t ap = (ap_t)g_fn("glVertexAttribPointer");
    use_t use = (use_t)g_fn("glUseProgram");
    da_t da = (da_t)g_fn("glDrawArrays");
    if (!vp || !cc || !cl || !gb || !bv || !bd || !ea || !ap || !use || !da) {
        diff_log_step(case_id, step++, "h02", "(missing)"); *step_out = step; return;
    }
    if (vp) vp(0, 0, W, H);
    if (cc) cc(0, 0, 0, 1);
    if (cl) cl(0x4000);
    /* 位置 + 颜色（stride 24） */
    float v[18] = { -1,-1, 1,0,0,1,   3,-1, 0,1,0,1,   -1,3, 0,0,1,1 };
    uint32_t vbo = 0;
    gb(1, &vbo); bv(GL_ARRAY_BUFFER, vbo);
    bd(GL_ARRAY_BUFFER, sizeof(v), v, GL_STATIC_DRAW);
    ea(0); ap(0, 2, GL_FLOAT, 0, 24, (const void*)0);
    ea(1); ap(1, 4, GL_FLOAT, 0, 24, (const void*)8);
    use(prog);
    da(GL_TRIANGLES, 0, 3);
    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "gradient_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* h03: 深度测试双三角形 */
static void case_h03(const char* case_id, int* step_out) {
    int step = *step_out;
    const int W = 256, H = 256;
    const ShaderPair* p = &SHADER_PAIRS[3]; /* T4 深度 */
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    if (!prog) { *step_out = step; return; }
    uint32_t tex = 0, fbo = 0, rbo = 0;
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) { *step_out = step; return; }
    typedef void (*vp_t)(int, int, int, int);
    typedef void (*en_t)(uint32_t);
    typedef void (*df_t)(uint32_t);
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cl_t)(uint32_t);
    typedef void (*gb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*bd_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ea_t)(uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*use_t)(uint32_t);
    typedef int (*gul_t)(uint32_t, const char*);
    typedef void (*u4_t)(int, float, float, float, float);
    typedef void (*da_t)(uint32_t, int, int);
    vp_t vp = (vp_t)g_fn("glViewport");
    en_t en = (en_t)g_fn("glEnable");
    df_t df = (df_t)g_fn("glDepthFunc");
    cc_t cc = (cc_t)g_fn("glClearColor");
    cl_t cl = (cl_t)g_fn("glClear");
    gb_t gb = (gb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    bd_t bd = (bd_t)g_fn("glBufferData");
    ea_t ea = (ea_t)g_fn("glEnableVertexAttribArray");
    ap_t ap = (ap_t)g_fn("glVertexAttribPointer");
    use_t use = (use_t)g_fn("glUseProgram");
    gul_t gul = (gul_t)g_fn("glGetUniformLocation");
    u4_t u4 = (u4_t)g_fn("glUniform4f");
    da_t da = (da_t)g_fn("glDrawArrays");
    if (!vp || !en || !df || !cc || !cl || !gb || !bv || !bd || !ea || !ap || !use || !da) {
        diff_log_step(case_id, step++, "h03", "(missing)"); *step_out = step; return;
    }
    if (vp) vp(0, 0, W, H);
    if (en) en(0x0B71);
    if (df) df(0x0201);
    if (cc) cc(0, 0, 0, 1);
    if (cl) cl(0x4000 | 0x0100);
    float verts[18] = { -1,-1,-0.8f, 3,-1,-0.8f, -1,3,-0.8f,
                        -1,-1, 0.8f, 3,-1, 0.8f, -1,3, 0.8f };
    uint32_t vbo = 0;
    gb(1, &vbo); bv(GL_ARRAY_BUFFER, vbo);
    bd(GL_ARRAY_BUFFER, sizeof(verts), verts, GL_STATIC_DRAW);
    ea(0); ap(0, 3, GL_FLOAT, 0, 0, (const void*)0);
    use(prog);
    int loc = gul ? gul(prog, "uColor") : -1;
    if (u4) { u4(loc, 1, 0, 0, 1); da(GL_TRIANGLES, 0, 3); }
    if (u4) { u4(loc, 0, 1, 0, 1); da(GL_TRIANGLES, 3, 3); }
    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "depth_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* h04: 混合两个半透明三角形 */
static void case_h04(const char* case_id, int* step_out) {
    int step = *step_out;
    const int W = 256, H = 256;
    const ShaderPair* p = &SHADER_PAIRS[4]; /* T5 混合 */
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    if (!prog) { *step_out = step; return; }
    uint32_t tex = 0, fbo = 0, rbo = 0;
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) { *step_out = step; return; }
    typedef void (*vp_t)(int, int, int, int);
    typedef void (*en_t)(uint32_t);
    typedef void (*bf_t)(uint32_t, uint32_t);
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cl_t)(uint32_t);
    typedef void (*gb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*bd_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ea_t)(uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*use_t)(uint32_t);
    typedef int (*gul_t)(uint32_t, const char*);
    typedef void (*u4_t)(int, float, float, float, float);
    typedef void (*da_t)(uint32_t, int, int);
    vp_t vp = (vp_t)g_fn("glViewport");
    en_t en = (en_t)g_fn("glEnable");
    bf_t bf = (bf_t)g_fn("glBlendFunc");
    cc_t cc = (cc_t)g_fn("glClearColor");
    cl_t cl = (cl_t)g_fn("glClear");
    gb_t gb = (gb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    bd_t bd = (bd_t)g_fn("glBufferData");
    ea_t ea = (ea_t)g_fn("glEnableVertexAttribArray");
    ap_t ap = (ap_t)g_fn("glVertexAttribPointer");
    use_t use = (use_t)g_fn("glUseProgram");
    gul_t gul = (gul_t)g_fn("glGetUniformLocation");
    u4_t u4 = (u4_t)g_fn("glUniform4f");
    da_t da = (da_t)g_fn("glDrawArrays");
    if (!vp || !en || !bf || !cc || !cl || !gb || !bv || !bd || !ea || !ap || !use || !da) {
        diff_log_step(case_id, step++, "h04", "(missing)"); *step_out = step; return;
    }
    if (vp) vp(0, 0, W, H);
    if (en) en(0x0BE2 /*BLEND*/);
    if (bf) bf(0x0302 /*SRC_ALPHA*/, 0x0303 /*ONE_MINUS_SRC_ALPHA*/);
    if (cc) cc(0, 0, 0, 1);
    if (cl) cl(0x4000);
    /* 两个重叠三角形（右侧偏移），半透明 */
    float v1[6] = { -1.5f,-1, 1.5f,-1, -1.5f,3 };
    float v2[6] = { 0.5f,-1, 3.5f,-1, 0.5f,3 };
    uint32_t vbo = 0;
    gb(1, &vbo); bv(GL_ARRAY_BUFFER, vbo);
    bd(GL_ARRAY_BUFFER, sizeof(v1) + sizeof(v2), NULL, GL_STATIC_DRAW);
    bv(GL_ARRAY_BUFFER, vbo);
    bd(GL_ARRAY_BUFFER, sizeof(v1) + sizeof(v2), NULL, GL_DYNAMIC_DRAW);
    typedef void (*sub_t)(uint32_t, intptr_t, intptr_t, const void*);
    sub_t sub = (sub_t)g_fn("glBufferSubData");
    if (sub) { sub(GL_ARRAY_BUFFER, 0, sizeof(v1), v1); sub(GL_ARRAY_BUFFER, sizeof(v1), sizeof(v2), v2); }
    ea(0); ap(0, 2, GL_FLOAT, 0, 0, (const void*)0);
    use(prog);
    int loc = gul ? gul(prog, "uColor") : -1;
    if (u4) { u4(loc, 1, 0, 0, 0.5f); da(GL_TRIANGLES, 0, 3); }
    if (u4) { u4(loc, 0, 0, 1, 0.5f); da(GL_TRIANGLES, 3, 3); }
    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "blend_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* h05: 纹理采样四边形（T3 模板 + 8x8 渐变纹理） */
static void case_h05(const char* case_id, int* step_out) {
    int step = *step_out;
    const int W = 256, H = 256;
    const ShaderPair* p = &SHADER_PAIRS[2]; /* T3 纹理 */
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    if (!prog) { *step_out = step; return; }
    uint32_t tex = 0, fbo = 0, rbo = 0;
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) { *step_out = step; return; }
    typedef void (*vp_t)(int, int, int, int);
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cl_t)(uint32_t);
    typedef void (*gb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*bd_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ea_t)(uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*use_t)(uint32_t);
    typedef int (*gul_t)(uint32_t, const char*);
    typedef void (*u1i_t)(int, int);
    typedef void (*at_t)(uint32_t);
    typedef void (*gt_t)(int, uint32_t*);
    typedef void (*bt_t)(uint32_t, uint32_t);
    typedef void (*img_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    typedef void (*da_t)(uint32_t, int, int);
    vp_t vp = (vp_t)g_fn("glViewport");
    cc_t cc = (cc_t)g_fn("glClearColor");
    cl_t cl = (cl_t)g_fn("glClear");
    gb_t gb = (gb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    bd_t bd = (bd_t)g_fn("glBufferData");
    ea_t ea = (ea_t)g_fn("glEnableVertexAttribArray");
    ap_t ap = (ap_t)g_fn("glVertexAttribPointer");
    use_t use = (use_t)g_fn("glUseProgram");
    gul_t gul = (gul_t)g_fn("glGetUniformLocation");
    u1i_t u1i = (u1i_t)g_fn("glUniform1i");
    at_t at = (at_t)g_fn("glActiveTexture");
    gt_t gt = (gt_t)g_fn("glGenTextures");
    bt_t bt = (bt_t)g_fn("glBindTexture");
    img_t img = (img_t)g_fn("glTexImage2D");
    da_t da = (da_t)g_fn("glDrawArrays");
    if (!vp || !cc || !cl || !gb || !bv || !bd || !ea || !ap || !use || !da || !at || !gt || !bt || !img) {
        diff_log_step(case_id, step++, "h05", "(missing)"); *step_out = step; return;
    }
    if (vp) vp(0, 0, W, H);
    if (cc) cc(0, 0, 0, 1);
    if (cl) cl(0x4000);
    /* 纹理 8x8 渐变 */
    uint8_t tpx[8 * 8 * 4];
    for (int y = 0; y < 8; y++)
        for (int x = 0; x < 8; x++) {
            tpx[(y * 8 + x) * 4 + 0] = (uint8_t)(x * 32);
            tpx[(y * 8 + x) * 4 + 1] = (uint8_t)(y * 32);
            tpx[(y * 8 + x) * 4 + 2] = 128;
            tpx[(y * 8 + x) * 4 + 3] = 255;
        }
    uint32_t texid = 0;
    if (at) at(0x84C0);
    if (gt) gt(1, &texid);
    if (bt) bt(GL_TEXTURE_2D, texid);
    if (img) img(GL_TEXTURE_2D, 0, GL_RGBA8, 8, 8, 0, GL_RGBA, GL_UNSIGNED_BYTE, tpx);
    /* 全屏四边形（2 个三角形）位置 + UV */
    float v[24] = { -1,-1, 0,0,  1,-1, 1,0,  1,1, 1,1,
                     -1,-1, 0,0,  1,1, 1,1,  -1,1, 0,1 };
    uint32_t vbo = 0;
    gb(1, &vbo); bv(GL_ARRAY_BUFFER, vbo);
    bd(GL_ARRAY_BUFFER, sizeof(v), v, GL_STATIC_DRAW);
    ea(0); ap(0, 2, GL_FLOAT, 0, 16, (const void*)0);
    ea(1); ap(1, 2, GL_FLOAT, 0, 16, (const void*)8);
    use(prog);
    int loc = gul ? gul(prog, "uTex") : -1;
    if (u1i) u1i(loc, 0);
    da(GL_TRIANGLES, 0, 6);
    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "tex_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* h06: scissor 分区 clear（4 区域不同颜色） */
static void case_h06(const char* case_id, int* step_out) {
    int step = *step_out;
    const int W = 256, H = 256;
    uint32_t tex = 0, fbo = 0, rbo = 0;
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    gen_t gen = (gen_t)g_fn("glGenTextures");
    bind_t bind = (bind_t)g_fn("glBindTexture");
    typedef void (*img_t)(uint32_t, int, int, int, int, int, uint32_t, uint32_t, const void*);
    img_t img = (img_t)g_fn("glTexImage2D");
    if (!gen || !bind || !img) { diff_log_step(case_id, step++, "h06", "(missing)"); *step_out = step; return; }
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) { *step_out = step; return; }
    typedef void (*vp_t)(int, int, int, int);
    typedef void (*sc_t)(int, int, int, int);
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cl_t)(uint32_t);
    vp_t vp = (vp_t)g_fn("glViewport");
    sc_t sc = (sc_t)g_fn("glScissor");
    cc_t cc = (cc_t)g_fn("glClearColor");
    cl_t cl = (cl_t)g_fn("glClear");
    typedef void (*en_t)(uint32_t);
    en_t en = (en_t)g_fn("glEnable");
    if (!vp || !sc || !cc || !cl || !en) { diff_log_step(case_id, step++, "h06", "(missing)"); *step_out = step; return; }
    if (vp) vp(0, 0, W, H);
    if (en) en(0x0C11 /*SCISSOR_TEST*/);
    if (sc) sc(0, 0, W / 2, H / 2);
    if (cc) cc(1, 0, 0, 1); if (cl) cl(0x4000);
    if (sc) sc(W / 2, 0, W / 2, H / 2);
    if (cc) cc(0, 1, 0, 1); if (cl) cl(0x4000);
    if (sc) sc(0, H / 2, W / 2, H / 2);
    if (cc) cc(0, 0, 1, 1); if (cl) cl(0x4000);
    if (sc) sc(W / 2, H / 2, W / 2, H / 2);
    if (cc) cc(1, 1, 0, 1); if (cl) cl(0x4000);
    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "scissor_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* h07: 模板测试（stencil 写 + 测试） */
static void case_h07(const char* case_id, int* step_out) {
    int step = *step_out;
    const int W = 256, H = 256;
    const ShaderPair* p = &SHADER_PAIRS[0];
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    if (!prog) { *step_out = step; return; }
    uint32_t tex = 0, fbo = 0, rbo = 0;
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) { *step_out = step; return; }
    typedef void (*vp_t)(int, int, int, int);
    typedef void (*en_t)(uint32_t);
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cl_t)(uint32_t);
    typedef void (*sm_t)(uint32_t);
    typedef void (*sf_t)(uint32_t, int, uint32_t);
    typedef void (*so_t)(uint32_t, uint32_t, uint32_t);
    typedef void (*gb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*bd_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ea_t)(uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*use_t)(uint32_t);
    typedef int (*gul_t)(uint32_t, const char*);
    typedef void (*u4_t)(int, float, float, float, float);
    typedef void (*da_t)(uint32_t, int, int);
    vp_t vp = (vp_t)g_fn("glViewport");
    en_t en = (en_t)g_fn("glEnable");
    cc_t cc = (cc_t)g_fn("glClearColor");
    cl_t cl = (cl_t)g_fn("glClear");
    sm_t sm = (sm_t)g_fn("glStencilMask");
    sf_t sf = (sf_t)g_fn("glStencilFunc");
    so_t so = (so_t)g_fn("glStencilOp");
    gb_t gb = (gb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    bd_t bd = (bd_t)g_fn("glBufferData");
    ea_t ea = (ea_t)g_fn("glEnableVertexAttribArray");
    ap_t ap = (ap_t)g_fn("glVertexAttribPointer");
    use_t use = (use_t)g_fn("glUseProgram");
    gul_t gul = (gul_t)g_fn("glGetUniformLocation");
    u4_t u4 = (u4_t)g_fn("glUniform4f");
    da_t da = (da_t)g_fn("glDrawArrays");
    if (!vp || !en || !cc || !cl || !sm || !sf || !so || !gb || !bv || !bd || !ea || !ap || !use || !da) {
        diff_log_step(case_id, step++, "h07", "(missing)"); *step_out = step; return;
    }
    if (vp) vp(0, 0, W, H);
    if (en) en(0x0B90 /*GL_STENCIL_TEST*/);
    if (sm) sm(0xFF);
    if (sf) sf(0x0207 /*GL_ALWAYS*/, 1, 0xFF);
    if (so) so(0x0204 /*GL_REPLACE*/, 0x0204, 0x0204);
    if (cc) cc(0, 0, 0, 1);
    if (cl) cl(0x4000 | 0x0400 /*COLOR|STENCIL*/);
    float verts[6] = { -1,-1, 3,-1, -1,3 };
    uint32_t vbo = 0;
    gb(1, &vbo); bv(GL_ARRAY_BUFFER, vbo);
    bd(GL_ARRAY_BUFFER, sizeof(verts), verts, GL_STATIC_DRAW);
    ea(0); ap(0, 2, GL_FLOAT, 0, 0, (const void*)0);
    use(prog);
    int loc = gul ? gul(prog, "uColor") : -1;
    /* pass 1：写 stencil=1（ALWAYS） */
    if (u4) u4(loc, 1, 0, 0, 1);
    da(GL_TRIANGLES, 0, 3);
    /* pass 2：stencil==1 才画（蓝） */
    if (sf) sf(0x0202 /*GL_EQUAL*/, 1, 0xFF);
    if (so) so(0x0200 /*GL_KEEP*/, 0x0200, 0x0200);
    if (u4) u4(loc, 0, 0, 1, 1);
    da(GL_TRIANGLES, 0, 3);
    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "stencil_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* h08: 多 draw 同帧（3 个不同颜色三角形） */
static void case_h08(const char* case_id, int* step_out) {
    int step = *step_out;
    const int W = 256, H = 256;
    const ShaderPair* p = &SHADER_PAIRS[0];
    const char* vs; const char* fs;
    pick_shader(p, &vs, &fs);
    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    if (!prog) { *step_out = step; return; }
    uint32_t tex = 0, fbo = 0, rbo = 0;
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) { *step_out = step; return; }
    typedef void (*vp_t)(int, int, int, int);
    typedef void (*cc_t)(float, float, float, float);
    typedef void (*cl_t)(uint32_t);
    typedef void (*gb_t)(int, uint32_t*);
    typedef void (*bv_t)(uint32_t, uint32_t);
    typedef void (*bd_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*ea_t)(uint32_t);
    typedef void (*ap_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*use_t)(uint32_t);
    typedef int (*gul_t)(uint32_t, const char*);
    typedef void (*u4_t)(int, float, float, float, float);
    typedef void (*da_t)(uint32_t, int, int);
    vp_t vp = (vp_t)g_fn("glViewport");
    cc_t cc = (cc_t)g_fn("glClearColor");
    cl_t cl = (cl_t)g_fn("glClear");
    gb_t gb = (gb_t)g_fn("glGenBuffers");
    bv_t bv = (bv_t)g_fn("glBindBuffer");
    bd_t bd = (bd_t)g_fn("glBufferData");
    ea_t ea = (ea_t)g_fn("glEnableVertexAttribArray");
    ap_t ap = (ap_t)g_fn("glVertexAttribPointer");
    use_t use = (use_t)g_fn("glUseProgram");
    gul_t gul = (gul_t)g_fn("glGetUniformLocation");
    u4_t u4 = (u4_t)g_fn("glUniform4f");
    da_t da = (da_t)g_fn("glDrawArrays");
    if (!vp || !cc || !cl || !gb || !bv || !bd || !ea || !ap || !use || !u4 || !da) {
        diff_log_step(case_id, step++, "h08", "(missing)"); *step_out = step; return;
    }
    if (vp) vp(0, 0, W, H);
    if (cc) cc(0, 0, 0, 1);
    if (cl) cl(0x4000);
    float verts[18] = { -1,-1, -0.2f,-1, -1,-0.2f,   /* 左下 */
                         0.2f,-1, 1,-1, 0.2f,-0.2f,   /* 右下 */
                        -1, 0.2f, -0.2f, 0.2f, -1, 1 }; /* 左上 */
    uint32_t vbo = 0;
    gb(1, &vbo); bv(GL_ARRAY_BUFFER, vbo);
    bd(GL_ARRAY_BUFFER, sizeof(verts), verts, GL_STATIC_DRAW);
    ea(0); ap(0, 2, GL_FLOAT, 0, 0, (const void*)0);
    use(prog);
    int loc = gul ? gul(prog, "uColor") : -1;
    if (u4) { u4(loc, 1, 0, 0, 1); da(GL_TRIANGLES, 0, 3); }
    if (u4) { u4(loc, 0, 1, 0, 1); da(GL_TRIANGLES, 3, 3); }
    if (u4) { u4(loc, 0, 0, 1, 1); da(GL_TRIANGLES, 6, 3); }
    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "multi_draw_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}

/* ============ e11: UBO block 查询链（MC 风格无实例名 block）============
 * 仿 MC 1.21 的 Projection/DynamicTransforms 无实例名 std140 block：
 * 验证 glGetUniformBlockIndex 三端（desktop/gles/translate）返回值一致性
 * （B1 假设：translate 端 GLES 对无实例名 block 查询是否失败），
 * 以及 glUniformBlockBinding + glBindBufferBase + 矩阵数据到达（像素哈希）。
 * 版本：gles 喂 320 es 直通；desktop/translate 喂 330 core（translate 走翻译管线）。 */
#define E11_VS_330 \
    "#version 330 core\n" \
    "layout(std140) uniform Projection { mat4 ProjMat; };\n" \
    "layout(std140) uniform DynamicTransforms { mat4 ModelViewMat; vec4 ColorModulator; };\n" \
    "in vec2 aPos;\n" \
    "void main() { gl_Position = ProjMat * ModelViewMat * vec4(aPos, 0.0, 1.0); }\n"
#define E11_FS_330 \
    "#version 330 core\n" \
    "uniform vec4 uColor;\n" \
    "out vec4 FragColor;\n" \
    "void main() { FragColor = uColor; }\n"
#define E11_VS_320 \
    "#version 320 es\n" \
    "precision highp float;\n" \
    "layout(std140) uniform Projection { mat4 ProjMat; };\n" \
    "layout(std140) uniform DynamicTransforms { mat4 ModelViewMat; vec4 ColorModulator; };\n" \
    "in vec2 aPos;\n" \
    "void main() { gl_Position = ProjMat * ModelViewMat * vec4(aPos, 0.0, 1.0); }\n"
#define E11_FS_320 \
    "#version 320 es\n" \
    "precision mediump float;\n" \
    "uniform vec4 uColor;\n" \
    "out vec4 FragColor;\n" \
    "void main() { FragColor = uColor; }\n"

static void case_e11(const char* case_id, int* step_out) {
    int step = *step_out;
    const int W = 64, H = 64;
    const char* vs = (g_backend == BACKEND_GLES) ? E11_VS_320 : E11_VS_330;
    const char* fs = (g_backend == BACKEND_GLES) ? E11_FS_320 : E11_FS_330;

    typedef uint32_t (*gubIdx_t)(uint32_t, const char*);
    typedef void (*ubb_t)(uint32_t, uint32_t, uint32_t);
    typedef void (*genBuffers_t)(int, uint32_t*);
    typedef void (*bindBuffer_t)(uint32_t, uint32_t);
    typedef void (*bindBufferBase_t)(uint32_t, uint32_t, uint32_t);
    typedef void (*bufferData_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*genVAO_t)(int, uint32_t*);
    typedef void (*bindVAO_t)(uint32_t);
    typedef void (*enableAttrib_t)(uint32_t);
    typedef void (*attribPtr_t)(uint32_t, int, uint32_t, uint8_t, int, const void*);
    typedef void (*useProgram_t)(uint32_t);
    typedef int (*getUniformLocation_t)(uint32_t, const char*);
    typedef void (*uniform4f_t)(int, float, float, float, float);
    typedef void (*drawArrays_t)(uint32_t, int, int);
    typedef void (*clear_t)(uint32_t);
    typedef void (*clearColor_t)(float, float, float, float);
    typedef void (*viewport_t)(int, int, int, int);
    gubIdx_t gbi = (gubIdx_t)g_fn("glGetUniformBlockIndex");
    ubb_t ubb = (ubb_t)g_fn("glUniformBlockBinding");
    genBuffers_t gb = (genBuffers_t)g_fn("glGenBuffers");
    bindBuffer_t bb = (bindBuffer_t)g_fn("glBindBuffer");
    bindBufferBase_t bbb = (bindBufferBase_t)g_fn("glBindBufferBase");
    bufferData_t bd = (bufferData_t)g_fn("glBufferData");
    genVAO_t gv = (genVAO_t)g_fn("glGenVertexArrays");
    bindVAO_t bv = (bindVAO_t)g_fn("glBindVertexArray");
    enableAttrib_t ea = (enableAttrib_t)g_fn("glEnableVertexAttribArray");
    attribPtr_t ap = (attribPtr_t)g_fn("glVertexAttribPointer");
    useProgram_t up = (useProgram_t)g_fn("glUseProgram");
    getUniformLocation_t gul = (getUniformLocation_t)g_fn("glGetUniformLocation");
    uniform4f_t u4 = (uniform4f_t)g_fn("glUniform4f");
    drawArrays_t da = (drawArrays_t)g_fn("glDrawArrays");
    clear_t cl = (clear_t)g_fn("glClear");
    clearColor_t cc = (clearColor_t)g_fn("glClearColor");
    viewport_t vp = (viewport_t)g_fn("glViewport");
    if (!gbi || !ubb || !gb || !bb || !bbb || !bd || !gv || !bv || !ea || !ap ||
        !up || !da || !cl || !cc || !vp) {
        diff_log_step(case_id, step++, "e11", "(missing 函数指针)");
        *step_out = step;
        return;
    }

    uint32_t prog = build_program_es(case_id, &step, vs, fs);
    if (!prog) { diff_log_step(case_id, step++, "e11", "program 构建失败"); *step_out = step; return; }

    /* 1. glGetUniformBlockIndex 查询（B1 验证核心：三端返回值对比） */
    uint32_t idxP = gbi(prog, "Projection");
    uint32_t idxD = gbi(prog, "DynamicTransforms");
    uint32_t idxX = gbi(prog, "Nonexistent");
    diff_log_step(case_id, step++, "glGetUniformBlockIndex",
        "Projection=0x%08X DynamicTransforms=0x%08X Nonexistent=0x%08X",
        idxP, idxD, idxX);
    diff_check_errors(case_id, step++);

    /* 2. glUniformBlockBinding（index 有效才绑定） */
    if (idxP != 0xFFFFFFFFu) ubb(prog, idxP, 0);
    if (idxD != 0xFFFFFFFFu) ubb(prog, idxD, 1);
    diff_log_step(case_id, step++, "glUniformBlockBinding",
        "Projection->0 DynamicTransforms->1 (skipped if index invalid)");
    diff_check_errors(case_id, step++);

    /* 3. UBO buffer 数据：单位矩阵（std140：ProjMat 64B；DynamicTransforms 80B） */
    float identity[16];
    for (int i = 0; i < 16; i++) identity[i] = (i % 5 == 0) ? 1.0f : 0.0f;
    float dt[20];
    for (int i = 0; i < 16; i++) dt[i] = (i % 5 == 0) ? 1.0f : 0.0f;
    dt[16] = dt[17] = dt[18] = dt[19] = 1.0f;

    uint32_t bufP = 0, bufD = 0;
    gb(1, &bufP);
    bb(0x8A11 /*GL_UNIFORM_BUFFER*/, bufP);
    bd(0x8A11, 64, identity, 0x88E8 /*GL_DYNAMIC_DRAW*/);
    gb(1, &bufD);
    bb(0x8A11, bufD);
    bd(0x8A11, 80, dt, 0x88E8);

    /* 4. glBindBufferBase（真机缺失的调用，此处显式验证三端行为） */
    bbb(0x8A11, 0, bufP);
    bbb(0x8A11, 1, bufD);
    diff_log_step(case_id, step++, "glBindBufferBase", "point0->bufP point1->bufD");
    diff_check_errors(case_id, step++);

    /* 5. 渲染目标 + 三角形（identity 矩阵 → 全屏可见 → 哈希可检测矩阵是否生效） */
    uint32_t tex = 0, fbo = 0, rbo = 0;
    if (diff_make_render_target(W, H, &tex, &fbo, &rbo) != 0) {
        diff_log_step(case_id, step++, "e11", "FBO 创建失败");
        *step_out = step;
        return;
    }
    if (vp) vp(0, 0, W, H);
    if (cc) cc(0.0f, 0.0f, 0.0f, 1.0f);
    if (cl) cl(0x00004000 /*GL_COLOR_BUFFER_BIT*/);

    float verts[6] = { -1.0f, -1.0f, 3.0f, -1.0f, -1.0f, 3.0f };
    uint32_t vbo = 0, vao = 0;
    if (gv) gv(1, &vao);
    if (bv) bv(vao);
    if (gb) gb(1, &vbo);
    if (bb) bb(0x8892 /*GL_ARRAY_BUFFER*/, vbo);
    if (bd) bd(0x8892, sizeof(verts), verts, 0x88E4 /*GL_STATIC_DRAW*/);
    if (ea) ea(0);
    if (ap) ap(0, 2, 0x1406 /*GL_FLOAT*/, 0, 0, (const void*)0);
    if (up) up(prog);
    if (u4 && gul) {
        int loc = gul(prog, "uColor");
        /* location 具体值为 linker/注入器实现定义（desktop Mesa 受 block 影响分配 3，
           translate 注入器按声明序分 0）；只断言"有效"，实际值走 diff_log 诊断 */
        diff_log("e11 uColor location=%d", loc);
        diff_log_step(case_id, step++, "glGetUniformLocation",
            "uColor=%s", loc >= 0 ? "valid" : "INVALID");
        u4(loc, 0.2f, 0.4f, 0.8f, 1.0f);
    }
    if (da) da(0x0004 /*GL_TRIANGLES*/, 0, 3);
    diff_log_step(case_id, step++, "glDrawArrays", "ubo triangle drawn");

    uint64_t h = diff_render_and_hash(W, H);
    diff_log_step(case_id, step++, "readPixels_hash", "ubo_hash=0x%016llX", (unsigned long long)h);
    diff_check_errors(case_id, step++);
    *step_out = step;
}


static CaseDef g_cases[] = {
    { "a00", "version/renderer/extensions", case_a00 },
    { "a01", "passthrough cap state machine", case_a01 },
    { "a02", "filtered cap recording", case_a02 },
    { "a03", "primitive restart translate", case_a03 },
    { "a04", "initial state dump", case_a04 },
    { "a05", "Enablei valid+oob", case_a05 },
    { "a06", "clear values readback", case_a06 },
    { "b00", "buffer lifecycle + readback hash", case_b00 },
    { "b01", "buffer lifecycle+IsBuffer", case_b01 },
    { "b02", "BufferData SIZE/USAGE", case_b02 },
    { "b03", "SubData write+readback hash", case_b03 },
    { "b04", "MapBufferRange+Flush+readback", case_b04 },
    { "b05", "Map WRITE|INVALIDATE readback", case_b05 },
    { "b06", "BindBufferBase/Range+GetIntegeri_v", case_b06 },
    { "b07", "CopyBufferSubData readback", case_b07 },
    { "b08", "SubData partial overwrite", case_b08 },
    { "b09", "delete bound buffer binding query", case_b09 },
    { "b01s", "ES shader compile/link passthrough", case_b01s },
    { "a01s", "state machine subset (4 caps)", case_a01s },
    { "c01", "tex default params", case_c01 },
    { "c02", "TexImage2D+level params", case_c02 },
    { "c03", "unsized RGB normalize", case_c03 },
    { "c04", "RGBA16F HALF_FLOAT", case_c04 },
    { "c05", "TexParameteri 6 readback", case_c05 },
    { "c06", "TexSubImage2D patch", case_c06 },
    { "c07", "GenerateMipmap", case_c07 },
    { "c08", "tex error paths", case_c08 },
    { "c09", "COMPARE_MODE readback", case_c09 },
    { "c10", "SWIZZLE readback", case_c10 },
    { "c11", "cubemap 6 faces", case_c11 },
    { "c12", "delete bound texture query", case_c12 },
    { "d01", "VAO lifecycle", case_d01 },
    { "d02", "VertexAttribPointer readback", case_d02 },
    { "d03", "VertexAttribIPointer", case_d03 },
    { "d04", "VertexAttribDivisor", case_d04 },
    { "d05", "VertexAttrib4fv current", case_d05 },
    { "e01", "Create+IsProgram", case_e01 },
    { "e02", "shader compile ok", case_e02 },
    { "e03", "shader compile fail", case_e03 },
    { "e04", "link ok", case_e04 },
    { "e05", "link fail (varying mismatch)", case_e05 },
    { "e06", "uniform/attrib location", case_e06 },
    { "e07", "uniform set+readback", case_e07 },
    { "e08", "BindAttribLocation prebind", case_e08 },
    { "e09", "UseProgram+CURRENT_PROGRAM", case_e09 },
    { "e10", "GetActiveUniform", case_e10 },
    { "e11", "UBO block query chain", case_e11 },
    { "f01", "FBO no attach", case_f01 },
    { "f02", "FBO color attach render", case_f02 },
    { "f03", "FBO depth test render", case_f03 },
    { "f04", "FBO depth only", case_f04 },
    { "f05", "RenderbufferStorage params", case_f05 },
    { "f06", "FBO attach deleted tex", case_f06 },
    { "g01", "version strings", case_g01 },
    { "g02", "GetStringi oob", case_g02 },
    { "g03", "15 capability values", case_g03 },
    { "g04", "state binding group", case_g04 },
    { "g05", "MAJOR/MINOR", case_g05 },
    { "g06", "PROFILE_MASK/NUM_EXT", case_g06 },
    { "g07", "occlusion query", case_g07 },
    { "g08", "conditional render", case_g08 },
    { "g09", "FenceSync lifecycle", case_g09 },
    { "g10", "line width", case_g10 },
    { "g11", "error injection", case_g11 },
    { "h01s", "solid triangle pixel hash", case_h01s },
    { "h01", "solid triangle 256 hash", case_h01 },
    { "h02", "gradient triangle hash", case_h02 },
    { "h03", "depth test hash", case_h03 },
    { "h04", "blend hash", case_h04 },
    { "h05", "texture quad hash", case_h05 },
    { "h06", "scissor 4-region hash", case_h06 },
    { "h07", "stencil hash", case_h07 },
    { "h08", "multi-draw hash", case_h08 },
};
static const int g_case_count = (int)(sizeof(g_cases) / sizeof(g_cases[0]));

/* ============ main ============ */
static void usage(const char* prog) {
    fprintf(stderr,
        "用法: %s --backend desktop|translate|gles [--case a00|b00|b01s|a01s|h01s|all] [--out FILE]\n"
        "      %s --list\n",
        prog, prog);
}

int main(int argc, char** argv) {
    const char* backend_str = NULL;
    const char* case_sel = "all";
    const char* out_path = NULL;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--backend") == 0 && i + 1 < argc) backend_str = argv[++i];
        else if (strcmp(argv[i], "--case") == 0 && i + 1 < argc) case_sel = argv[++i];
        else if (strcmp(argv[i], "--out") == 0 && i + 1 < argc) out_path = argv[++i];
        else if (strcmp(argv[i], "--list") == 0) {
            printf("用例列表:\n");
            for (int c = 0; c < g_case_count; c++)
                printf("  %s  %s\n", g_cases[c].id, g_cases[c].name);
            return 0;
        } else {
            usage(argv[0]);
            return 2;
        }
    }
    if (!backend_str) { usage(argv[0]); return 2; }

    int backend = strcmp(backend_str, "desktop") == 0 ? BACKEND_DESKTOP
                 : strcmp(backend_str, "translate") == 0 ? BACKEND_TRANSLATE
                 : strcmp(backend_str, "gles") == 0 ? BACKEND_GLES : -1;
    if (backend < 0) { usage(argv[0]); return 2; }

    if (out_path) {
        g_log = fopen(out_path, "w");
        if (!g_log) { fprintf(stderr, "无法打开输出文件 %s\n", out_path); return 2; }
    } else {
        g_log = stdout;
    }

    /* translate 模式默认加载项目产物 */
    const char* fluorategl_path = "target/debug/libfluorategl.so";

    if (diff_init_backend(backend, fluorategl_path) != 0) {
        fprintf(stderr, "backend 初始化失败\n");
        return 1;
    }
    if (create_backend_context(backend) != 0) {
        fprintf(stderr, "context 创建失败\n");
        return 1;
    }

    diff_log("=== backend=%s 开始 ===", backend_str);
    int ran = 0;
    for (int c = 0; c < g_case_count; c++) {
        if (strcmp(case_sel, "all") != 0 && strcmp(case_sel, g_cases[c].id) != 0) continue;
        int step = 0;
        diff_log("=== case %s (%s) ===", g_cases[c].id, g_cases[c].name);
        g_cases[c].fn(g_cases[c].id, &step);
        ran++;
    }
    diff_log("=== 完成: %d 个用例 ===", ran);

    if (g_log && g_log != stdout) fclose(g_log);
    return 0;
}
