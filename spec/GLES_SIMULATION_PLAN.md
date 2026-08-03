# GL 3.3 core 特性模拟清单（GLES 3.1+ / 优先 3.2）

> 规划文档：记录 GL 3.3 core 中 GLES 3.1/3.2 缺失、需要模拟的特性及模拟策略。
> 本文件只做规划，不涉及代码改动。实现状态随代码演进更新。

## 1. 目标与原则

- 模拟目标：GL 3.3 core（桌面），底层 GLES 3.1+（设备优先走 3.2 功能，不行回退 3.1）
- 原则：
  1. **优先使用扩展**（每项标注具体扩展名，如 `GL_EXT_texture_border_clamp`）
  2. **无扩展回退模拟实现**（循环/转换/FBO 读回/CPU 同步等）
  3. **实在无法模拟才 stub/忽略**（标注风险）
- 范围：仅 GL 3.3 core 中 GLES 3.1/3.2 缺失的特性（GLES 原生支持的直接透传，不在清单内）
- 状态标记：✅ 已实现 / 🔶 部分（说明缺口）/ ⬜ 未实现 / stub（no-op 或固定值）
- ★ = 已知风险/待办缺口

## 2. 特性模拟清单（按类别）

### 2.1 纹理类

| # | GL 3.3 特性 | GLES 3.1 | GLES 3.2 | 模拟策略（扩展优先 → 回退） | 当前状态 |
|---|---|---|---|---|---|
| 1 | glTexImage1D / glTexSubImage1D / glCopyTexImage1D / glCopyTexSubImage1D / glCompressedTexImage1D / glCompressedTexSubImage1D（1D 纹理，GLES 无 GL_TEXTURE_1D） | 无 | 无 | 无扩展 → 模拟（2D 高度 1 替代）或忽略（MC 不使用 1D 纹理） | 🔶 glFramebufferTexture1D 为 stub；其余未导出/未实现 |
| 2 | glTexImage2DMultisample / glTexImage3DMultisample（mutable MSAA 纹理） | 无 | 无 | 无扩展 → 回退 glTexStorage2DMultisample（immutable，语义近似；mutable 语义不可完全模拟） | stub |
| 3 | glGetTexImage / glGetCompressedTexImage（纹理回读） | 无 | 无 | GL_ANGLE_get_image（可查）→ 模拟 FBO attach + glReadPixels | ✅ 已模拟（glGetTexImage，受限：RGB 读回格式组合有限，3D/2D_ARRAY 无法精确模拟） |
| 4 | GL_TEXTURE_LOD_BIAS 参数 | 无 | 无 | 无对应 → 忽略（MC 已调用，拦截忽略） | ✅ 已实现（is_unsupported_tex_parameter 过滤） |
| 5 | GL_TEXTURE_CUBE_MAP_SEAMLESS | 无 | 无 | 优先 GL_EXT_texture_cube_map_seamless → 无则忽略 | ✅ 已实现（glEnable 吞掉 cap） |
| 6 | BGRA 像素格式（glTexImage2D format=GL_BGRA） | 无 | 无 | GL_EXT_texture_format_BGRA8888 → 回退 swizzle/格式转换 | ✅ 已实现（normalize_format_param：GL_BGR→GL_RGB、GL_BGRA→GL_RGBA，首次转换 warn；**红蓝互换风险仍标注 ★**） |
| 7 | 压缩纹理 | ETC2/EAC 原生 | 原生 | S3TC/DXT：GL_EXT_texture_compression_s3tc → 无则模拟解压或忽略；ASTC：GL_KHR_texture_compression_astc_ldr | ✅ 已实现（BPTC/RGTC/EAC-signed 识别补齐 + S3TC 运行时能力检测 GL_NUM_COMPRESSED_TEXTURE_FORMATS + 非压缩降级 texImage2D） |
| 8 | glTexParameterIiv / glGetTexParameterIiv | 无 | 原生 | 3.2 原生 → 3.1 回退 float 版本转换 | ⬜ 未实现 |
| 9 | GL_TEXTURE_BORDER_COLOR / GL_CLAMP_TO_BORDER | 无 | 无 | GL_EXT_texture_border_clamp → 忽略 | 🔶 FAKE_EXTENSIONS 已声明 GL_EXT_texture_border_clamp，参数处理待确认 |
| 10 | GL_TEXTURE_MAX_ANISOTROPY | 无 | 无 | GL_EXT_texture_filter_anisotropic（双向扩展，需在 FAKE_EXTENSIONS 声明） | ✅ FAKE_EXTENSIONS 已声明；参数直通 |
| 11 | glClearTexImage / glClearTexSubImage | 无 | 无 | GL_ARB_clear_texture（desktop 4.4/扩展）→ 模拟（临时缓冲 + SubImage） | stub |
| 12 | glFramebufferTexture3D | 无 | 无 | → glFramebufferTextureLayer | ✅ 已实现（framebuffer.rs 109 行 glFramebufferTextureLayer 覆盖） |
| 13 | glFramebufferTexture1D | 无 | 无 | 无对应 → stub | stub |

### 2.2 Buffer 类

| # | GL 3.3 特性 | GLES 3.1 | GLES 3.2 | 模拟策略（扩展优先 → 回退） | 当前状态 |
|---|---|---|---|---|---|
| 14 | glBufferStorage（immutable） | 无 | 无 | GL_EXT_buffer_storage → 回退 glBufferData（不可完全模拟 immutable 语义，标注限制） | ✅ 已实现（PERSISTENT shadow 路径 + 降级 glBufferData） |
| 15 | glMapBuffer（旧接口） | 无 | 无 | → glMapBufferRange 转换 | ✅ 已实现 |
| 16 | glGetBufferSubData | 无 | 无 | 无扩展 → 模拟 map + memcpy | ✅ 已实现 |
| 17 | glFlushMappedBufferRange（shadow 同步链路） | 3.0 原生 ✓ | 原生 | GLES 原生 ✓（无需模拟）；**shadow 路径**：持久映射 buffer 的脏区需在 draw 前同步到 GLES | ✅ 已实现（P1 sync 泛化：全量遍历 persistent_buffers 脏区按 bound_target 同步；glBindBufferBase/Range 记录 bound_target；GL_UNIFORM_BUFFER 等全部 target 覆盖） |

### 2.3 Draw 类

| # | GL 3.3 特性 | GLES 3.1 | GLES 3.2 | 模拟策略（扩展优先 → 回退） | 当前状态 |
|---|---|---|---|---|---|
| 18 | glMultiDrawArrays / glMultiDrawElements | 无 | 无 | GL_EXT_multi_draw_arrays → 回退循环单次 draw | ✅ 已实现（循环） |
| 19 | glMultiDrawElementsBaseVertex | 无 | 3.2 原生 | 3.2 原生 → GL_EXT_draw_elements_base_vertex → 循环（保留 basevertex）→ 循环普通 draw（丢 basevertex） | ✅ 已实现（三级；MultiDraw 循环降级路径已补 draw 前 sync；basevertex 降级含索引指针补偿） |
| 20 | glMultiDrawArraysIndirect / glMultiDrawElementsIndirect | 无 | 3.2 原生 | 3.2 原生 → GL_EXT_multi_draw_indirect → 循环单次 Indirect | ✅ 已实现（循环降级路径已补 draw 前 sync） |
| 21 | glMultiDrawArraysIndirectCount / glMultiDrawElementsIndirectCount | 无 | 无 | GL_EXT_multi_draw_indirect（含 count 版）→ 回退 CPU 读 count 循环 | ✅ 已实现（CPU 模拟；限制：临时改绑 GL_COPY_READ_BUFFER，需恢复绑定） |
| 22 | base vertex 家族（glDrawElementsBaseVertex 等） | 无 | 3.2 原生 | 3.2 原生 → GL_EXT_draw_elements_base_vertex → 无则降级丢 basevertex（best-effort） | ✅ 已实现（caps 检测 + 降级 + **索引指针补偿 offset_indices**） |
| 23 | glDrawBuffer（单数） | 无 | 无 | → glDrawBuffers 转换（GL_BACK/GL_NONE/GL_COLOR_ATTACHMENTi） | ✅ 已实现（framebuffer.rs 285 行） |
| 24 | glPrimitiveRestartIndex + GL_PRIMITIVE_RESTART | 仅 FIXED_INDEX | 仅 FIXED_INDEX | 匹配 0xFFFF/0xFFFFFFFF 时用 fixed；否则需重排 index buffer（复杂，标注） | ✅ 已实现（translate_enable_cap：GL_PRIMITIVE_RESTART 0x8F3D→GL_PRIMITIVE_RESTART_FIXED_INDEX 0x8D63；索引值匹配 fixed 语义） |
| 25 | occlusion query GL_SAMPLES_PASSED | 无 | 无 | → GL_ANY_SAMPLES_PASSED（语义近似） | ✅ 已实现（translate_query_target：0x8914→0x8C2F，Begin/End/GetQueryiv 三处） |
| 26 | timer query（glQueryCounter / glGetQueryObjecti64v / ui64v / GL_TIME_ELAPSED） | 无 | GL_EXT_disjoint_timer_query | GL_EXT_disjoint_timer_query → stub 返回 0 | ✅ stub 保留（返回 0）；**FAKE_EXTENSIONS 的 GL_EXT_disjoint_timer_query 声明已移除（矛盾解除）；GL_ARB_timer_query 保留声明 + stub** |
| 27 | glGetQueryObjectiv | 无 | 无 | → glGetQueryObjectuiv 替代 | ✅ 已实现 |
| 28 | glBeginConditionalRender / glEndConditionalRender | 无 | 无 | GL_NV_conditional_render → 模拟（查询 + 跳过）或忽略 | stub |

### 2.4 Vertex 类

| # | GL 3.3 特性 | GLES 3.1 | GLES 3.2 | 模拟策略（扩展优先 → 回退） | 当前状态 |
|---|---|---|---|---|---|
| 29 | glVertexAttrib* 便捷族（glVertexAttrib1s-4s、1d-4d、4bv/4iv/4ubv/4usv、4N*、I1i-I3ui、I4bv/s/ubv/usv、glVertexAttribP* 等 70+） | 仅 4 分量 float | 仅 4 分量 float | 无扩展 → 模拟映射到 4 分量 float/int（多余分量置 0/1） | ✅ 大部分已实现（**4N 系列最负值 clamp 已修**；double 精度损失、非 N 版本 as f32 仍标注） |
| 30 | GL_DOUBLE 顶点类型（glVertexAttribPointer type=GL_DOUBLE） | 无 | 无 | → GL_FLOAT 转换（需重排顶点数据，复杂） | ⬜ 直通（GLES 收到 GL_DOUBLE 可能报错，标注） |
| 31 | glGetVertexAttribdv | 无 | 无 | → 临时 f32 转 f64 | ✅ 已实现（getter.rs 165 行） |
| 32 | glGetDoublev | 无 | 无 | → float 转换 | ✅ 已实现（临时 f32 缓冲转 f64，弃用直通） |

### 2.5 Shader/Program 类

| # | GL 3.3 特性 | GLES 3.1 | GLES 3.2 | 模拟策略（扩展优先 → 回退） | 当前状态 |
|---|---|---|---|---|---|
| 33 | glBindFragDataLocation / glBindFragDataLocationIndexed | 无 | 无 | GL_EXT_blend_func_extended → 回退模拟（绑定 location 0） | ✅ stub（Sodium 依赖 no-op，标注风险） |
| 34 | glGetFragDataIndex / glGetFragDataLocation | 无 | 无 | GL_EXT_blend_func_extended → 返回 -1 / 直通 | ✅ glGetFragDataIndex stub 返回 -1；**glGetFragDataLocation 已导出直通（GLES 3.0 原生，含 ID 翻译）** |
| 35 | glGetActiveUniformName | 无 | 无 | 无 → 模拟（GetActiveUniformsiv + GetActiveUniform） | ✅ 已实现（program.rs 895 行） |
| 36 | 显式 uniform location（layout(location=) uniform） | 无 | 无 | GL_EXT_explicit_uniform_location → 翻译管线处理（postprocess 移除 location） | ✅ 管线已处理 |
| 37 | glGetShaderSource / GL_SHADER_SOURCE_LENGTH 语义 | 原生但语义差异 | 原生 | 设计决策项：返回原始 GLSL vs 翻译后 GLSL 的选择（非模拟项，标注决策） | ✅ 设计已定（返回翻译后源码） |

### 2.6 状态类

| # | GL 3.3 特性 | GLES 3.1 | GLES 3.2 | 模拟策略（扩展优先 → 回退） | 当前状态 |
|---|---|---|---|---|---|
| 38 | glPolygonMode | 无 | 无 | GL_NV_polygon_mode → 模拟（线框需 geometry shader，复杂）或忽略 | ✅ stub 忽略（GL_FILL 外首次告警，render_state.rs 215 行） |
| 39 | glLogicOp / GL_COLOR_LOGIC_OP | 无 | 无 | 无 → 忽略/模拟 | ⬜ 未实现 |
| 40 | glPointSize / glPointParameter* | 无 | 无 | 无（shader gl_PointSize 恒生效）→ 忽略 | 🔶 glPointParameteri/iv 已实现（pixel.rs）；**glPointSize 未导出** ⬜ |
| 41 | glPixelStoref | 无 | 无 | → glPixelStorei（f→i 截断，参数均为小整数） | ✅ 已实现（render_state.rs 231 行） |
| 42 | glDepthRange / glClearDepth（double 版） | 无 | 无 | → glDepthRangef / glClearDepthf | ✅ 已实现 |
| 43 | glClampColor | 无 | 无 | 恒 clamp → no-op | ✅ stub |
| 44 | 桌面 enable cap 过滤（GL_MULTISAMPLE / GL_PROGRAM_POINT_SIZE / GL_LINE_SMOOTH / GL_POLYGON_SMOOTH / GL_SEAMLESS / GL_ALPHA_TEST 等） | — | — | → 吞掉/过滤（is_unsupported_gles_cap） | ✅ 已实现（补 4 项 cap + GL_PRIMITIVE_RESTART→FIXED_INDEX 翻译 + GL_DEPTH_CLAMP 版本感知） |

### 2.7 字符串/枚举类

| # | GL 3.3 特性 | GLES 3.1 | GLES 3.2 | 模拟策略（扩展优先 → 回退） | 当前状态 |
|---|---|---|---|---|---|
| 45 | glGetString(GL_VERSION / GL_SHADING_LANGUAGE_VERSION) | — | — | 伪造 "3.3.0 FluorateGL vX" / "3.30"（config 统一维护） | ✅ 已实现 |
| 46 | glGetString(GL_EXTENSIONS) / glGetStringi | — | — | FAKE_EXTENSIONS 注入（caps 对齐校验） | ✅ 已实现（OnceLock 惰性构建） |
| 47 | 查询 pname 枚举差异（GLES 无的 pname，如 GL_POINT_SIZE_RANGE 等） | — | — | → 转换或返回安全值 | ✅ 已实现（GL_CONTEXT_FLAGS / GL_PROVOKING_VERTEX 拦截 + 绑定查询 OBJECT_NAME 回译补齐） |

### 2.8 其他

| # | GL 3.3 特性 | GLES 3.1 | GLES 3.2 | 模拟策略（扩展优先 → 回退） | 当前状态 |
|---|---|---|---|---|---|
| 48 | glGetPointerv | 3.2 原生 | 原生 | 3.2 原生 → 3.1 模拟/忽略 | ⬜ 未实现（3.1 设备缺口） |
| 49 | 采样器/纹理整数参数版（glSamplerParameterIiv 等） | 无 | 原生 | 3.2 原生 → 3.1 转换 | ⬜ 未实现（3.1 设备缺口） |

## 3. 已排除项（明确不需要模拟）

- **GLES 原生支持的**：GL 3.3 core 与 GLES 3.1/3.2 共有的函数直接透传（249 个），不在清单
- **方向相反的**（GLES 3.1/3.2 有而 GL 3.3 没有）：glTexStorage*（immutable）、glDraw*Indirect、SSBO、image load/store、program pipeline（PPO）、glTexBuffer 等——GLES 侧多余能力，无兼容负担
- **超出 GL 3.3 范围的扩展函数**（虽已导出供 MC/Sodium 使用）：base instance 全家（GL 4.2）、glBufferStorage（4.4）、glTexBuffer/glTexBufferRange（3.2/EXT）等——标注"超出 3.3 范围，另行处理"，不在本清单内重复评估

## 4. 实施优先级建议

按对 MC/Sodium 实际影响排序（参考函数地图的高风险项）。**P0-P2 已全部完成（见 §5 修复记录）**：

- ~~P0：UBO shadow 同步缺口（#17）~~ ✅ 已修复（sync 泛化 + bound_target）
- ~~P1：timer query 与 FAKE 矛盾（#26）、BGRA format（#6）、4N 除数（#29）~~ ✅ 已修复
- ~~P2：其余模拟项~~ ✅ 已按审查清单修复（1D 纹理/mutable MSAA/logic op/point size 等保留 ⬜ 状态项为按需实现，非本轮范围）

## 5. 本次审查修复记录（P1-P5，13 项）

7 组只读审查（exports/render_state、framebuffer/getter/query、program/shader、texture、buffer/sync、drawing/multi_draw、vertex_array）→ 13 项修复：

| # | 修复内容 | 涉及文件 |
|---|---|---|
| 1 | **UBO shadow 同步泛化**：sync 全量遍历 persistent_buffers 脏区按 bound_target 上传；glBindBufferBase/Range 记录 bound_target；GetBufferPointerv 返回 shadow_ptr；COHERENT 剥离不再附加 UNSYNCHRONIZED | buffer.rs、state/mod.rs |
| 2 | **MultiDraw 循环降级补 draw 前 sync**（ARRAY/ELEMENT/INDIRECT 四函数） | multi_draw.rs |
| 3 | **drawing.rs 补 ELEMENT sync + basevertex 降级索引指针补偿**（offset_indices） | drawing.rs |
| 4 | **texture 5 处**：glFramebufferTexture3D ID 翻译、normalize_format_param（BGR/BGRA）、压缩格式识别（BPTC/RGTC/EAC-signed）、S3TC 运行时能力检测、边界防护 | texture.rs |
| 5 | **shader/program**：glCreateShaderProgramv 返回 desktop id、glGetFragDataLocation 导出、glCreateProgram 无 context 防御、IdMap miss 显式写 0 | shader.rs、program.rs、dispatch.rs、symbols.rs、egl/exports.rs |
| 6 | **query**：GL_SAMPLES_PASSED→GL_ANY_SAMPLES_PASSED（0x8914→0x8C2F）Begin/End/GetQueryiv | query.rs |
| 7 | **exports**：cap 过滤补 4 项（FRAMEBUFFER_SRGB/SAMPLE_ALPHA_TO_ONE/POLYGON_OFFSET_LINE/POINT_SPRITE 等）、PRIMITIVE_RESTART→FIXED_INDEX 翻译、DEPTH_CLAMP 版本感知、FAKE_EXTENSIONS 移除矛盾声明（separate_shader_objects/get_program_binary/disjoint_timer_query） | exports.rs |
| 8 | **getter**：glGetDoublev 改 f32 中转、索引绑定回译补齐 | getter.rs |
| 9 | **framebuffer**：OBJECT_NAME 回译（TEXTURE/RENDERBUFFER 分类）、RenderbufferStorage 格式降级 | framebuffer.rs |
| 10 | **vertex_array**：便捷族 4N 最负值 clamp 修复等 | vertex_array.rs |

**验收状态**（P8）：`cargo build` 零错误零警告；全量测试 **262 passed / 0 failed**（lib 93 + gles_compile 25 + spirv_compile 38 + pipeline 29 + preprocess 43 + postprocess 30 + integration 4）；clippy 18 风格警告（无新增正确性类）；静态检查 5 项全部通过。
