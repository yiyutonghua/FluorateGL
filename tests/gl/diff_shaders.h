/* diff_shaders.h — 差分测试 shader 双版本模板
 *
 * 每个用例两组 shader：GLSL 330 core（桌面）与 GLSL 320 es（GLES）。
 * 本批只提供模板字符串（T1 纯色 / T2 顶点色 / T3 纹理 / T4 深度 / T5 混合），
 * 用例在 T4 批次实现。
 */
#ifndef DIFF_SHADERS_H
#define DIFF_SHADERS_H

/* ============ T1: 纯色（输出固定色） ============ */
#define T1_VS_330 \
    "#version 330 core\n" \
    "layout(location=0) in vec2 aPos;\n" \
    "void main() { gl_Position = vec4(aPos, 0.0, 1.0); }\n"

#define T1_FS_330 \
    "#version 330 core\n" \
    "uniform vec4 uColor;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = uColor; }\n"

#define T1_VS_320 \
    "#version 320 es\n" \
    "layout(location=0) in vec2 aPos;\n" \
    "void main() { gl_Position = vec4(aPos, 0.0, 1.0); }\n"

#define T1_FS_320 \
    "#version 320 es\n" \
    "precision mediump float;\n" \
    "uniform vec4 uColor;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = uColor; }\n"

/* ============ T2: 顶点色（位置 + 颜色插值） ============ */
#define T2_VS_330 \
    "#version 330 core\n" \
    "layout(location=0) in vec2 aPos;\n" \
    "layout(location=1) in vec4 aColor;\n" \
    "out vec4 vColor;\n" \
    "void main() { vColor = aColor; gl_Position = vec4(aPos, 0.0, 1.0); }\n"

#define T2_FS_330 \
    "#version 330 core\n" \
    "in vec4 vColor;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = vColor; }\n"

#define T2_VS_320 \
    "#version 320 es\n" \
    "layout(location=0) in vec2 aPos;\n" \
    "layout(location=1) in vec4 aColor;\n" \
    "out vec4 vColor;\n" \
    "void main() { vColor = aColor; gl_Position = vec4(aPos, 0.0, 1.0); }\n"

#define T2_FS_320 \
    "#version 320 es\n" \
    "precision mediump float;\n" \
    "in vec4 vColor;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = vColor; }\n"

/* ============ T3: 纹理采样 ============ */
#define T3_VS_330 \
    "#version 330 core\n" \
    "layout(location=0) in vec2 aPos;\n" \
    "layout(location=1) in vec2 aUV;\n" \
    "out vec2 vUV;\n" \
    "void main() { vUV = aUV; gl_Position = vec4(aPos, 0.0, 1.0); }\n"

#define T3_FS_330 \
    "#version 330 core\n" \
    "uniform sampler2D uTex;\n" \
    "in vec2 vUV;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = texture(uTex, vUV); }\n"

#define T3_VS_320 \
    "#version 320 es\n" \
    "layout(location=0) in vec2 aPos;\n" \
    "layout(location=1) in vec2 aUV;\n" \
    "out vec2 vUV;\n" \
    "void main() { vUV = aUV; gl_Position = vec4(aPos, 0.0, 1.0); }\n"

#define T3_FS_320 \
    "#version 320 es\n" \
    "precision mediump float;\n" \
    "uniform sampler2D uTex;\n" \
    "in vec2 vUV;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = texture(uTex, vUV); }\n"

/* ============ T4: 深度测试 ============ */
#define T4_VS_330 \
    "#version 330 core\n" \
    "layout(location=0) in vec3 aPos;\n" \
    "void main() { gl_Position = vec4(aPos, 1.0); }\n"

#define T4_FS_330 \
    "#version 330 core\n" \
    "uniform vec4 uColor;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = uColor; }\n"

#define T4_VS_320 \
    "#version 320 es\n" \
    "layout(location=0) in vec3 aPos;\n" \
    "void main() { gl_Position = vec4(aPos, 1.0); }\n"

#define T4_FS_320 \
    "#version 320 es\n" \
    "precision mediump float;\n" \
    "uniform vec4 uColor;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = uColor; }\n"

/* ============ T5: 混合（src_alpha 半透明覆盖） ============ */
#define T5_VS_330 \
    "#version 330 core\n" \
    "layout(location=0) in vec2 aPos;\n" \
    "void main() { gl_Position = vec4(aPos, 0.0, 1.0); }\n"

#define T5_FS_330 \
    "#version 330 core\n" \
    "uniform vec4 uColor;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = uColor; }\n"

#define T5_VS_320 \
    "#version 320 es\n" \
    "layout(location=0) in vec2 aPos;\n" \
    "void main() { gl_Position = vec4(aPos, 0.0, 1.0); }\n"

#define T5_FS_320 \
    "#version 320 es\n" \
    "precision mediump float;\n" \
    "uniform vec4 uColor;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = uColor; }\n"

/* ============ T6: 实例位置偏移（e16d BaseInstance 用例） ============
 * gl_InstanceID 驱动 x 偏移——baseinstance 改变实例 ID 起始值 → 位置不同。
 * GLSL 330（desktop/translate 翻译管线）与 320 es（native gles）均为
 * 内置 gl_InstanceID（GLES 3.0+），无需扩展。 */
#define T6_VS_330 \
    "#version 330 core\n" \
    "layout(location=0) in vec2 aPos;\n" \
    "void main() { gl_Position = vec4(aPos.x + float(gl_InstanceID) * 0.5, aPos.y, 0.0, 1.0); }\n"

#define T6_FS_330 \
    "#version 330 core\n" \
    "uniform vec4 uColor;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = uColor; }\n"

#define T6_VS_320 \
    "#version 320 es\n" \
    "layout(location=0) in vec2 aPos;\n" \
    "void main() { gl_Position = vec4(aPos.x + float(gl_InstanceID) * 0.5, aPos.y, 0.0, 1.0); }\n"

#define T6_FS_320 \
    "#version 320 es\n" \
    "precision mediump float;\n" \
    "uniform vec4 uColor;\n" \
    "out vec4 fragColor;\n" \
    "void main() { fragColor = uColor; }\n"

/* ============ 模板索引表（T4 用例使用） ============ */
typedef struct {
    const char* id;      /* "T1".."T6" */
    const char* vs_330;
    const char* fs_330;
    const char* vs_320;
    const char* fs_320;
} ShaderPair;

static const ShaderPair SHADER_PAIRS[] = {
    { "T1", T1_VS_330, T1_FS_330, T1_VS_320, T1_FS_320 },
    { "T2", T2_VS_330, T2_FS_330, T2_VS_320, T2_FS_320 },
    { "T3", T3_VS_330, T3_FS_330, T3_VS_320, T3_FS_320 },
    { "T4", T4_VS_330, T4_FS_330, T4_VS_320, T4_FS_320 },
    { "T5", T5_VS_330, T5_FS_330, T5_VS_320, T5_FS_320 },
    { "T6", T6_VS_330, T6_FS_330, T6_VS_320, T6_FS_320 },
};
#define SHADER_PAIR_COUNT ((int)(sizeof(SHADER_PAIRS) / sizeof(SHADER_PAIRS[0])))

#endif /* DIFF_SHADERS_H */
