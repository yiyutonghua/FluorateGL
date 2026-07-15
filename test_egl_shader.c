/* Manual test for FluorateGL shader translation on Linux with Mesa llvmpipe.
 *
 * Build:
 *   gcc -o test_egl_shader test_egl_shader.c -ldl -lEGL
 * Run (surfaceless platform, libGLESv3.so symlink to libGLESv2.so.2):
 *   ln -sf /lib/x86_64-linux-gnu/libGLESv2.so.2 libGLESv3.so
 *   EGL_PLATFORM=surfaceless LD_LIBRARY_PATH=. MESA_LOADER_DRIVER_OVERRIDE=llvmpipe ./test_egl_shader
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>
#include <EGL/egl.h>
#include <GLES3/gl3.h>

typedef int (*fluorategl_init_fn)(void);
typedef GLuint (*glCreateShader_fn)(GLenum);
typedef void (*glShaderSource_fn)(GLuint, GLsizei, const GLchar *const*, const GLint*);
typedef void (*glCompileShader_fn)(GLuint);
typedef void (*glGetShaderiv_fn)(GLuint, GLenum, GLint*);
typedef void (*glGetShaderInfoLog_fn)(GLuint, GLsizei, GLsizei*, GLchar*);
typedef void (*glDeleteShader_fn)(GLuint);

static void* get_sym(void* handle, const char* name) {
    void* p = dlsym(handle, name);
    if (!p) {
        fprintf(stderr, "dlsym(%s) failed: %s\n", name, dlerror());
    }
    return p;
}

static int setup_egl(void) {
    EGLDisplay display = eglGetDisplay(EGL_DEFAULT_DISPLAY);
    if (display == EGL_NO_DISPLAY) {
        fprintf(stderr, "eglGetDisplay failed\n");
        return -1;
    }

    EGLint major, minor;
    if (!eglInitialize(display, &major, &minor)) {
        fprintf(stderr, "eglInitialize failed\n");
        return -1;
    }
    printf("EGL version: %d.%d\n", major, minor);

    EGLint config_attribs[] = {
        EGL_RENDERABLE_TYPE, EGL_OPENGL_ES3_BIT,
        EGL_SURFACE_TYPE, EGL_PBUFFER_BIT,
        EGL_NONE
    };
    EGLConfig config;
    EGLint num_configs;
    if (!eglChooseConfig(display, config_attribs, &config, 1, &num_configs) || num_configs == 0) {
        fprintf(stderr, "eglChooseConfig failed\n");
        return -1;
    }

    EGLint context_attribs[] = {
        EGL_CONTEXT_CLIENT_VERSION, 3,
        EGL_NONE
    };
    EGLContext context = eglCreateContext(display, config, EGL_NO_CONTEXT, context_attribs);
    if (context == EGL_NO_CONTEXT) {
        fprintf(stderr, "eglCreateContext failed\n");
        return -1;
    }

    EGLint pbuffer_attribs[] = {
        EGL_WIDTH, 1,
        EGL_HEIGHT, 1,
        EGL_NONE
    };
    EGLSurface surface = eglCreatePbufferSurface(display, config, pbuffer_attribs);
    if (surface == EGL_NO_SURFACE) {
        fprintf(stderr, "eglCreatePbufferSurface failed\n");
        return -1;
    }

    if (!eglMakeCurrent(display, surface, surface, context)) {
        fprintf(stderr, "eglMakeCurrent failed\n");
        return -1;
    }

    printf("EGL context current (llvmpipe expected)\n");
    return 0;
}

static int compile_test_shader(
    glCreateShader_fn glCreateShader,
    glShaderSource_fn glShaderSource,
    glCompileShader_fn glCompileShader,
    glGetShaderiv_fn glGetShaderiv,
    glGetShaderInfoLog_fn glGetShaderInfoLog,
    glDeleteShader_fn glDeleteShader,
    GLenum stage,
    const char* stage_name,
    const char* source
) {
    GLuint shader = glCreateShader(stage);
    if (shader == 0) {
        fprintf(stderr, "[%s] glCreateShader failed\n", stage_name);
        return -1;
    }

    glShaderSource(shader, 1, &source, NULL);
    glCompileShader(shader);

    GLint status = 0;
    glGetShaderiv(shader, GL_COMPILE_STATUS, &status);

    GLint log_len = 0;
    glGetShaderiv(shader, GL_INFO_LOG_LENGTH, &log_len);
    if (log_len > 0) {
        char* log = malloc(log_len);
        glGetShaderInfoLog(shader, log_len, NULL, log);
        if (strlen(log) > 0) {
            printf("[%s] info log:\n%s\n", stage_name, log);
        }
        free(log);
    }

    glDeleteShader(shader);

    if (status != GL_TRUE) {
        fprintf(stderr, "[%s] compile failed\n", stage_name);
        return -1;
    }

    printf("[%s] compile succeeded\n", stage_name);
    return 0;
}

int main(void) {
    void* handle = dlopen("./target/x86_64-unknown-linux-gnu/debug/libfluorategl.so", RTLD_NOW);
    if (!handle) {
        fprintf(stderr, "dlopen failed: %s\n", dlerror());
        return 1;
    }

    fluorategl_init_fn init = get_sym(handle, "fluorategl_init");
    if (!init) return 1;

    int ret = init();
    if (ret != 0) {
        fprintf(stderr, "fluorategl_init returned %d\n", ret);
        return 1;
    }

    if (setup_egl() != 0) {
        return 1;
    }

    glCreateShader_fn glCreateShader = get_sym(handle, "glCreateShader");
    glShaderSource_fn glShaderSource = get_sym(handle, "glShaderSource");
    glCompileShader_fn glCompileShader = get_sym(handle, "glCompileShader");
    glGetShaderiv_fn glGetShaderiv = get_sym(handle, "glGetShaderiv");
    glGetShaderInfoLog_fn glGetShaderInfoLog = get_sym(handle, "glGetShaderInfoLog");
    glDeleteShader_fn glDeleteShader = get_sym(handle, "glDeleteShader");
    if (!glCreateShader || !glShaderSource || !glCompileShader || !glGetShaderiv ||
        !glGetShaderInfoLog || !glDeleteShader) {
        return 1;
    }

    const char* vertex_source =
        "#version 150 core\n"
        "in vec3 Position;\n"
        "in vec2 UV;\n"
        "out vec2 vUV;\n"
        "uniform mat4 ModelViewProjection;\n"
        "void main() {\n"
        "    vUV = UV;\n"
        "    gl_Position = ModelViewProjection * vec4(Position, 1.0);\n"
        "}\n";

    const char* fragment_source =
        "#version 150 core\n"
        "in vec2 vUV;\n"
        "out vec4 fragColor;\n"
        "uniform sampler2D Tex;\n"
        "void main() {\n"
        "    fragColor = texture(Tex, vUV);\n"
        "}\n";

    int ok = 0;
    ok |= compile_test_shader(
        glCreateShader, glShaderSource, glCompileShader,
        glGetShaderiv, glGetShaderInfoLog, glDeleteShader,
        GL_VERTEX_SHADER, "vertex", vertex_source
    );
    ok |= compile_test_shader(
        glCreateShader, glShaderSource, glCompileShader,
        glGetShaderiv, glGetShaderInfoLog, glDeleteShader,
        GL_FRAGMENT_SHADER, "fragment", fragment_source
    );

    dlclose(handle);
    return ok;
}
