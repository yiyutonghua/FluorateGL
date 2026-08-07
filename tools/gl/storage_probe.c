/* 临时验证：glBufferStorage(flags=0, data=预填) 预填数据是否真正可读回（三后端） */
#include <stdio.h>
#include <string.h>
#include <dlfcn.h>
#include <stdint.h>
#include <EGL/egl.h>
#include <EGL/eglext.h>

static void* glh = NULL;
#define FN(name, type) type name = (type)dlsym(glh, #name)

int main(int argc, char** argv) {
    const char* backend = argv[1] ? argv[1] : "gles";
    if (strcmp(backend, "desktop") == 0) glh = dlopen("libGL.so.1", RTLD_NOW);
    else if (strcmp(backend, "gles") == 0) glh = dlopen("libGLESv2.so.2", RTLD_NOW);
    else glh = dlopen("libfluorategl.so", RTLD_NOW);
    if (!glh) { printf("dlopen 失败: %s\n", dlerror()); return 1; }
    void* eglh = dlopen("libEGL.so.1", RTLD_NOW);
    if (!eglh) { printf("EGL dlopen 失败: %s\n", dlerror()); return 1; }
#define EFN(name, type) type name = (type)dlsym(eglh, name##_STR)
    const char* eglGetPlatformDisplay_fn_STR = "eglGetPlatformDisplay";
    const char* eglInitialize_fn_STR = "eglInitialize";
    const char* eglBindAPI_fn_STR = "eglBindAPI";
    const char* eglCreateContext_fn_STR = "eglCreateContext";
    const char* eglMakeCurrent_fn_STR = "eglMakeCurrent";
    EFN(eglGetPlatformDisplay_fn, PFNEGLGETPLATFORMDISPLAYPROC);
    EFN(eglInitialize_fn, PFNEGLINITIALIZEPROC);
    EFN(eglBindAPI_fn, PFNEGLBINDAPIPROC);
    EFN(eglCreateContext_fn, PFNEGLCREATECONTEXTPROC);
    EFN(eglMakeCurrent_fn, PFNEGLMAKECURRENTPROC);
    PFNEGLGETERRORPROC gerr = (PFNEGLGETERRORPROC)dlsym(eglh, "eglGetError");
    if (!eglGetPlatformDisplay_fn || !eglInitialize_fn || !eglBindAPI_fn || !eglCreateContext_fn || !eglMakeCurrent_fn) {
        printf("EGL 函数缺失: gp=%p init=%p bind=%p ctx=%p cur=%p\n", (void*)eglGetPlatformDisplay_fn, (void*)eglInitialize_fn, (void*)eglBindAPI_fn, (void*)eglCreateContext_fn, (void*)eglMakeCurrent_fn); return 1;
    }
    EGLDisplay dpy = eglGetPlatformDisplay_fn(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
    if (!dpy) { printf("eglGetPlatformDisplay 失败 err=0x%X\n", gerr()); return 1; }
    int maj = 0, min = 0;
    if (!eglInitialize_fn(dpy, &maj, &min)) { printf("eglInitialize 失败 err=0x%X\n", gerr()); return 1; }
    EGLint api = (strcmp(backend, "desktop") == 0) ? EGL_OPENGL_API : EGL_OPENGL_ES_API;
    if (!eglBindAPI_fn(api)) { printf("eglBindAPI 失败 err=0x%X\n", gerr()); return 1; }
    EGLint attrs[] = { (strcmp(backend, "desktop") == 0) ? EGL_CONTEXT_MAJOR_VERSION : EGL_CONTEXT_CLIENT_VERSION,
                       (strcmp(backend, "desktop") == 0) ? 3 : 3,
                       (strcmp(backend, "desktop") == 0) ? EGL_CONTEXT_MINOR_VERSION : EGL_NONE,
                       (strcmp(backend, "desktop") == 0) ? 3 : EGL_NONE,
                       EGL_NONE };
    EGLContext ctx = eglCreateContext_fn(dpy, NULL, EGL_NO_CONTEXT, attrs);
    if (!ctx) { printf("eglCreateContext 失败 err=0x%X\n", gerr()); return 1; }
    if (!eglMakeCurrent_fn(dpy, EGL_NO_SURFACE, EGL_NO_SURFACE, ctx)) {
        printf("eglMakeCurrent 失败 err=0x%X\n", gerr()); return 1;
    }

    typedef const char* (*getstr_t)(uint32_t);
    typedef uint32_t (*geterr_t)(void);
    getstr_t getstr = (getstr_t)dlsym(glh, "glGetString");
    geterr_t geterrf = (geterr_t)dlsym(glh, "glGetError");
    if (getstr) printf("[%s] GL_VERSION=%s\n", backend, getstr(0x1F02) ? getstr(0x1F02) : "(null)");
    typedef void (*gen_t)(int, uint32_t*);
    typedef void (*bind_t)(uint32_t, uint32_t);
    typedef void (*storage_t)(uint32_t, intptr_t, const void*, uint32_t);
    typedef void (*getsub_t)(uint32_t, intptr_t, intptr_t, void*);
    typedef void (*getiv_t)(uint32_t, uint32_t, int*);
    gen_t gen = (gen_t)dlsym(glh, "glGenBuffers");
    bind_t bind = (bind_t)dlsym(glh, "glBindBuffer");
    storage_t storage = (storage_t)dlsym(glh, "glBufferStorage");
    getsub_t getsub = (getsub_t)dlsym(glh, "glGetBufferSubData");
    getiv_t getiv = (getiv_t)dlsym(glh, "glGetBufferParameteriv");
    if (!gen || !bind || !storage || !getsub || !getiv) {
        printf("[%s] 函数缺失 gen=%d bind=%d storage=%d getsub=%d getiv=%d\n", backend,
               !!gen, !!bind, !!storage, !!getsub, !!getiv);
        return 1;
    }
    const uint32_t GL_UNIFORM_BUFFER = 0x8A11;
    const uint32_t GL_BUFFER_SIZE = 0x8764;
    const uint32_t GL_BUFFER_STORAGE_FLAGS = 0x871F;

    uint8_t pat[256];
    for (int i = 0; i < 256; i++) pat[i] = (uint8_t)(i * 7 + 3);

    /* 场景 1：flags=0 + data 预填 */
    uint32_t b1 = 0;
    gen(1, &b1);
    bind(GL_UNIFORM_BUFFER, b1);
    storage(GL_UNIFORM_BUFFER, 256, pat, 0x0000);
    EGLint e = gerr();
    uint32_t gle = geterrf ? geterrf() : 0;
    uint8_t rd[256] = { 0 };
    getsub(GL_UNIFORM_BUFFER, 0, 256, rd);
    int match1 = memcmp(rd, pat, 256) == 0;
    int size1 = 0, flags1 = 0;
    getiv(GL_UNIFORM_BUFFER, GL_BUFFER_SIZE, &size1);
    getiv(GL_UNIFORM_BUFFER, GL_BUFFER_STORAGE_FLAGS, &flags1);
    printf("[%s] flags=0+data: readback=%s size=%d storage_flags=0x%X egl_err=0x%X gl_err=0x%X\n",
           backend, match1 ? "MATCH" : "MISMATCH", size1, flags1, (unsigned)e, (unsigned)gle);

    /* 场景 2：flags=0x42（PERSISTENT|WRITE）+ data 预填 */
    uint32_t b2 = 0;
    gen(1, &b2);
    bind(GL_UNIFORM_BUFFER, b2);
    storage(GL_UNIFORM_BUFFER, 256, pat, 0x0042);
    e = gerr();
    memset(rd, 0, 256);
    getsub(GL_UNIFORM_BUFFER, 0, 256, rd);
    int match2 = memcmp(rd, pat, 256) == 0;
    printf("[%s] flags=0x42+data: readback=%s err=0x%X\n", backend, match2 ? "MATCH" : "MISMATCH", (unsigned)e);

    /* 场景 3：flags=0 + SubData 更新 */
    uint8_t pat2[256];
    for (int i = 0; i < 256; i++) pat2[i] = (uint8_t)(255 - i);
    typedef void (*subdata_t)(uint32_t, intptr_t, intptr_t, const void*);
    subdata_t sub = (subdata_t)dlsym(glh, "glBufferSubData");
    sub(GL_UNIFORM_BUFFER, 0, 256, pat2);
    memset(rd, 0, 256);
    getsub(GL_UNIFORM_BUFFER, 0, 256, rd);
    int match3 = memcmp(rd, pat2, 256) == 0;
    printf("[%s] flags=0+SubData: readback=%s err=0x%X\n", backend, match3 ? "MATCH" : "MISMATCH", (unsigned)gerr());

    return 0;
}
