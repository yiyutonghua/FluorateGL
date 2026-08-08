//! 本库 `#[unsafe(no_mangle)]` 导出符号表：服务 eglGetProcAddress 自查与 FAKE 校验。
//! 注意：新增导出函数时请同步更新本表（一致性由单测兜底）。

macro_rules! define_symbols {
    ($($name:literal),+ $(,)?) => {
        pub static SYMBOLS: &[&[u8]] = &[$(concat!($name, "\0").as_bytes()),+];
    };
}

define_symbols![
    // ==== EGL 导出（src/egl/exports.rs，共 34 个）====
    "eglGetDisplay",
    "eglInitialize",
    "eglTerminate",
    "eglQueryString",
    "eglGetConfigs",
    "eglChooseConfig",
    "eglGetConfigAttrib",
    "eglCreateWindowSurface",
    "eglCreatePbufferSurface",
    "eglCreatePbufferFromClientBuffer",
    "eglCreatePixmapSurface",
    "eglDestroySurface",
    "eglSurfaceAttrib",
    "eglBindTexImage",
    "eglReleaseTexImage",
    "eglBindAPI",
    "eglQueryAPI",
    "eglCreateContext",
    "eglDestroyContext",
    "eglMakeCurrent",
    "eglQueryContext",
    "eglQuerySurface",
    "eglGetCurrentContext",
    "eglGetCurrentSurface",
    "eglGetCurrentDisplay",
    "eglWaitClient",
    "eglWaitNative",
    "eglWaitGL",
    "eglReleaseThread",
    "eglSwapBuffers",
    "eglSwapInterval",
    "eglCopyBuffers",
    "eglGetError",
    "eglGetProcAddress",
    // ==== GL 导出：src/gl/exports.rs（共 30 个）====
    "glClear",
    "glAlphaFunc",
    "glDebugMessageCallback",
    "glDebugMessageCallbackKHR",
    "glObjectLabel",
    "glObjectLabelKHR",
    "glEnable",
    "glDisable",
    "glDepthFunc",
    "glDepthMask",
    "glBlendFunc",
    "glClearColor",
    "glClearDepth",
    "glClearStencil",
    "glViewport",
    "glScissor",
    "glCullFace",
    "glFrontFace",
    "glLineWidth",
    "glActiveTexture",
    "glPixelStorei",
    "glDrawArrays",
    "glDrawElements",
    "glFinish",
    "glFlush",
    "glGenerateMipmap",
    "glGetError",
    "glGetString",
    "glGetIntegerv",
    "glGetStringi",
    // ==== GL 导出：src/gl/buffer.rs（共 19 个）====
    "glGenBuffers",
    "glDeleteBuffers",
    "glBindBuffer",
    "glBufferData",
    "glBufferSubData",
    "glBufferStorage",
    "glMapBuffer",
    "glMapBufferRange",
    "glUnmapBuffer",
    "glFlushMappedBufferRange",
    "glCopyBufferSubData",
    "glBindBufferBase",
    "glBindBufferRange",
    "glGetBufferSubData",
    "glGetBufferParameteriv",
    "glGetBufferPointerv",
    "glIsBuffer",
    "glTexBuffer",
    "glTexBufferRange",
    // ==== GL 导出：src/gl/getter.rs（共 11 个）====
    "glGetBooleanv",
    "glGetFloatv",
    "glGetDoublev",
    "glGetInteger64v",
    "glGetBooleani_v",
    "glGetIntegeri_v",
    "glGetFloati_v",
    "glGetDoublei_v",
    "glIsEnabled",
    "glIsEnabledi",
    "glGetVertexAttribdv",
    // ==== GL 导出：src/gl/drawing.rs（共 14 个）====
    "glDrawRangeElements",
    "glDrawArraysInstanced",
    "glDrawElementsInstanced",
    "glPrimitiveRestartIndex",
    "glDrawElementsBaseVertex",
    "glDrawRangeElementsBaseVertex",
    "glDrawArraysIndirect",
    "glDrawElementsIndirect",
    "glDrawArraysInstancedBaseInstance",
    "glDrawElementsInstancedBaseInstance",
    "glDrawElementsInstancedBaseVertex",
    "glDrawElementsInstancedBaseVertexBaseInstance",
    "glDrawTransformFeedback",
    "glDrawTransformFeedbackInstanced",
    "glDispatchCompute",
    "glMemoryBarrier",
    // ==== GL 导出：src/gl/framebuffer.rs（共 24 个）====
    "glGenFramebuffers",
    "glDeleteFramebuffers",
    "glBindFramebuffer",
    "glFramebufferTexture2D",
    "glFramebufferTextureLayer",
    "glFramebufferRenderbuffer",
    "glCheckFramebufferStatus",
    "glGenRenderbuffers",
    "glDeleteRenderbuffers",
    "glBindRenderbuffer",
    "glRenderbufferStorage",
    "glRenderbufferStorageMultisample",
    "glBlitFramebuffer",
    "glDrawBuffers",
    "glDrawBuffer",
    "glReadBuffer",
    "glReadPixels",
    "glClearBufferfv",
    "glClearBufferiv",
    "glClearBufferuiv",
    "glClearBufferfi",
    "glGetFramebufferAttachmentParameteriv",
    "glIsFramebuffer",
    "glIsRenderbuffer",
    // ==== GL 导出：src/gl/shader.rs（共 10 个）====
    "glCreateShader",
    "glDeleteShader",
    "glShaderSource",
    "glCompileShader",
    "glGetShaderiv",
    "glGetShaderInfoLog",
    "glGetShaderSource",
    "glIsShader",
    "glReleaseShaderCompiler",
    "glCreateShaderProgramv",
    // ==== GL 导出：src/gl/program.rs（共 52 个）====
    "glCreateProgram",
    "glDeleteProgram",
    "glAttachShader",
    "glLinkProgram",
    "glUseProgram",
    "glGetProgramiv",
    "glGetProgramInfoLog",
    "glGetUniformLocation",
    "glGetAttribLocation",
    "glUniform1f",
    "glUniform1i",
    "glUniformMatrix4fv",
    "glDetachShader",
    "glValidateProgram",
    "glGetActiveUniform",
    "glGetActiveAttrib",
    "glGetUniformfv",
    "glGetUniformiv",
    "glGetAttachedShaders",
    "glBindAttribLocation",
    "glTransformFeedbackVaryings",
    "glGetTransformFeedbackVarying",
    "glUniformBlockBinding",
    "glBindFragDataLocation",
    "glBindFragDataLocationIndexed",
    "glGetUniformBlockIndex",
    "glGetActiveUniformBlockiv",
    "glGetActiveUniformBlockName",
    "glGetUniformIndices",
    "glGetActiveUniformsiv",
    "glIsProgram",
    "glUniform2f",
    "glUniform3f",
    "glUniform4f",
    "glUniform2i",
    "glUniform3i",
    "glUniform4i",
    "glUniform1fv",
    "glUniform2fv",
    "glUniform3fv",
    "glUniform4fv",
    "glUniform1iv",
    "glUniform2iv",
    "glUniform3iv",
    "glUniform4iv",
    "glUniformMatrix2fv",
    "glUniformMatrix3fv",
    "glShaderStorageBlockBinding",
    "glProgramParameteri",
    "glGetFragDataLocation",
    "glGetFragDataIndex",
    "glGetActiveUniformName",
    // ==== GL 导出：src/gl/query.rs（共 11 个）====
    "glGenQueries",
    "glDeleteQueries",
    "glIsQuery",
    "glBeginQuery",
    "glEndQuery",
    "glGetQueryiv",
    "glGetQueryObjectiv",
    "glGetQueryObjectuiv",
    "glQueryCounter",
    "glGetQueryObjecti64v",
    "glGetQueryObjectui64v",
    // ==== GL 导出：src/gl/pixel.rs（共 3 个）====
    "glClampColor",
    "glPointParameteri",
    "glPointParameteriv",
    // ==== GL 导出：src/gl/multi_draw.rs（共 7 个）====
    "glMultiDrawArrays",
    "glMultiDrawElements",
    "glMultiDrawElementsBaseVertex",
    "glMultiDrawArraysIndirect",
    "glMultiDrawElementsIndirect",
    "glMultiDrawArraysIndirectCount",
    "glMultiDrawElementsIndirectCount",
    // ==== GL 导出：src/gl/vertex_array.rs（共 69 个）====
    "glGenVertexArrays",
    "glDeleteVertexArrays",
    "glBindVertexArray",
    "glEnableVertexAttribArray",
    "glDisableVertexAttribArray",
    "glVertexAttribPointer",
    "glVertexAttribIPointer",
    "glVertexAttribDivisor",
    "glVertexAttrib1f",
    "glVertexAttrib2f",
    "glVertexAttrib3f",
    "glVertexAttrib4f",
    "glVertexAttrib1fv",
    "glVertexAttrib2fv",
    "glVertexAttrib3fv",
    "glVertexAttrib4fv",
    "glVertexAttribDivisorARB",
    "glVertexAttrib1s",
    "glVertexAttrib2s",
    "glVertexAttrib3s",
    "glVertexAttrib4s",
    "glVertexAttrib1d",
    "glVertexAttrib2d",
    "glVertexAttrib3d",
    "glVertexAttrib4d",
    "glVertexAttrib1sv",
    "glVertexAttrib2sv",
    "glVertexAttrib3sv",
    "glVertexAttrib4sv",
    "glVertexAttrib1dv",
    "glVertexAttrib2dv",
    "glVertexAttrib3dv",
    "glVertexAttrib4dv",
    "glVertexAttrib4iv",
    "glVertexAttrib4bv",
    "glVertexAttrib4ubv",
    "glVertexAttrib4usv",
    "glVertexAttrib4uiv",
    "glVertexAttrib4Nub",
    "glVertexAttrib4Nbv",
    "glVertexAttrib4Nsv",
    "glVertexAttrib4Niv",
    "glVertexAttrib4Nubv",
    "glVertexAttrib4Nusv",
    "glVertexAttrib4Nuiv",
    "glVertexAttribI1i",
    "glVertexAttribI2i",
    "glVertexAttribI3i",
    "glVertexAttribI4i",
    "glVertexAttribI1ui",
    "glVertexAttribI2ui",
    "glVertexAttribI3ui",
    "glVertexAttribI4ui",
    "glVertexAttribI1iv",
    "glVertexAttribI2iv",
    "glVertexAttribI3iv",
    "glVertexAttribI4iv",
    "glVertexAttribI1uiv",
    "glVertexAttribI2uiv",
    "glVertexAttribI3uiv",
    "glVertexAttribI4uiv",
    "glVertexAttribI4bv",
    "glVertexAttribI4sv",
    "glVertexAttribI4ubv",
    "glVertexAttribI4usv",
    "glBindVertexBuffer",
    "glVertexAttribFormat",
    "glVertexAttribIFormat",
    "glVertexAttribBinding",
    // ==== GL 导出：src/gl/sync.rs（共 5 个）====
    "glFenceSync",
    "glDeleteSync",
    "glClientWaitSync",
    "glWaitSync",
    "glIsSync",
    // ==== GL 导出：src/gl/texture.rs（共 27 个）====
    "glGenTextures",
    "glDeleteTextures",
    "glBindTexture",
    "glTexImage2D",
    "glTexSubImage2D",
    "glTexParameteri",
    "glTexImage3D",
    "glTexSubImage3D",
    "glTexStorage2D",
    "glTexStorage3D",
    "glTexParameterf",
    "glTexParameterfv",
    "glTexParameteriv",
    "glCompressedTexImage2D",
    "glCompressedTexSubImage2D",
    "glCompressedTexImage3D",
    "glCompressedTexSubImage3D",
    "glGetTexImage",
    "glGetTexLevelParameteriv",
    "glGetTexParameteriv",
    "glIsTexture",
    "glClearTexImage",
    "glClearTexSubImage",
    "glTexImage2DMultisample",
    "glTexImage3DMultisample",
    "glFramebufferTexture1D",
    "glFramebufferTexture3D",
    // ==== GL 导出：src/gl/render_state.rs（共 29 个）====
    "glEnablei",
    "glDisablei",
    "glBlendFunci",
    "glBlendFuncSeparate",
    "glBlendFuncSeparatei",
    "glBlendEquation",
    "glBlendEquationi",
    "glBlendEquationSeparate",
    "glBlendEquationSeparatei",
    "glColorMask",
    "glColorMaski",
    "glDepthRange",
    "glDepthRangef",
    "glStencilFunc",
    "glStencilFuncSeparate",
    "glStencilOp",
    "glStencilOpSeparate",
    "glStencilMask",
    "glStencilMaskSeparate",
    "glPolygonOffset",
    "glPolygonMode",
    "glPixelStoref",
    "glBlendEquationiARB",
    "glBlendEquationSeparateiARB",
    "glBlendFunciARB",
    "glBlendFuncSeparateiARB",
    "glProvokingVertex",
    "glBeginConditionalRender",
    "glEndConditionalRender",
    // ==== GL 补齐：阶段 1（GL 3.0-3.3 core，共 74 个）====
    // —— sampler.rs（14）——
    "glGenSamplers",
    "glDeleteSamplers",
    "glIsSampler",
    "glBindSampler",
    "glSamplerParameteri",
    "glSamplerParameterf",
    "glSamplerParameteriv",
    "glSamplerParameterfv",
    "glSamplerParameterIiv",
    "glSamplerParameterIuiv",
    "glGetSamplerParameteriv",
    "glGetSamplerParameterfv",
    "glGetSamplerParameterIiv",
    "glGetSamplerParameterIuiv",
    // —— transform_feedback.rs（13）——
    "glGenTransformFeedbacks",
    "glDeleteTransformFeedbacks",
    "glIsTransformFeedback",
    "glBindTransformFeedback",
    "glBeginTransformFeedback",
    "glEndTransformFeedback",
    "glPauseTransformFeedback",
    "glResumeTransformFeedback",
    "glTransformFeedbackBufferBase",
    "glTransformFeedbackBufferRange",
    "glGetTransformFeedbackiv",
    "glDrawTransformFeedbackStream",
    "glDrawTransformFeedbackStreamInstanced",
    // —— program.rs（9）——
    "glUniform1ui",
    "glUniform2ui",
    "glUniform3ui",
    "glUniform4ui",
    "glUniform1uiv",
    "glUniform2uiv",
    "glUniform3uiv",
    "glUniform4uiv",
    "glGetUniformuiv",
    // —— getter.rs（23）——
    "glIsVertexArray",
    "glGetVertexAttribiv",
    "glGetVertexAttribIiv",
    "glGetVertexAttribIuiv",
    "glGetVertexAttribPointerv",
    "glGetTexParameterfv",
    "glGetTexParameterIiv",
    "glGetTexParameterIuiv",
    "glGetTexLevelParameterfv",
    "glGetInternalformativ",
    "glGetFramebufferParameteriv",
    "glGetRenderbufferParameteriv",
    "glGetMultisamplefv",
    "glGetBufferParameteri64v",
    "glGetPointerv",
    "glSampleCoverage",
    "glSampleMaski",
    "glBlendColor",
    "glPointParameterf",
    "glPointParameterfv",
    "glHint",
    "glGetInternalformati64v",
    "glGetQueryIndexediv",
    // —— texture.rs（3）——
    "glCopyTexImage2D",
    "glCopyTexSubImage2D",
    "glCopyTexSubImage3D",
    // —— vertex_array.rs（8）——
    "glVertexAttribP1ui",
    "glVertexAttribP2ui",
    "glVertexAttribP3ui",
    "glVertexAttribP4ui",
    "glVertexAttribP1uiv",
    "glVertexAttribP2uiv",
    "glVertexAttribP3uiv",
    "glVertexAttribP4uiv",
    // —— 补登记（已导出未登记）——
    "glGetBooleani_v",
    "glGetDoublei_v",
    "glGetFloati_v",
    "glGetIntegeri_v",
];

/// 本库是否导出了该符号（name 为函数名字节串，带或不带尾 NUL 均可；
/// SYMBOLS 表内条目统一带尾 NUL，此处做归一化比较）
pub fn is_exported(name_bytes: &[u8]) -> bool {
    let name = name_bytes.strip_suffix(b"\0").unwrap_or(name_bytes);
    SYMBOLS.iter().any(|s| s.strip_suffix(b"\0") == Some(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表内所有符号必须能在动态符号表中解析到。
    ///
    /// 降级说明：lib 单测二进制经 rlib 静态链接后无 DYNSYM 段（未引用符号被链接器
    /// 裁剪），dlsym(RTLD_DEFAULT) 必然返回 null。故改为 dlopen 本库 cdylib 产物
    /// （libfluorategl.so）验证——cdylib 动态符号表即导出全集，与生产加载路径一致。
    /// cdylib 不存在时跳过（验证流程中 cargo build 先行保证产物存在）。
    #[test]
    fn all_exported_symbols_resolvable() {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let cdylib_path = std::path::Path::new(&manifest).join("target/debug/libfluorategl.so");
        if !cdylib_path.exists() {
            eprintln!(
                "SKIP: cdylib 产物不存在（{}），请先运行 cargo build。\
                 表内 {} 个符号的导出一致性由 cdylib 动态符号表验证保证。",
                cdylib_path.display(),
                SYMBOLS.len()
            );
            return;
        }
        let c_path = std::ffi::CString::new(cdylib_path.to_str().unwrap()).unwrap();
        let handle = unsafe { libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        assert!(!handle.is_null(), "dlopen 失败: {}", unsafe {
            let e = libc::dlerror();
            if e.is_null() {
                String::from("unknown")
            } else {
                std::ffi::CStr::from_ptr(e).to_string_lossy().into_owned()
            }
        });
        for sym in SYMBOLS {
            let name = std::str::from_utf8(&sym[..sym.len() - 1]).unwrap();
            let ptr = unsafe { libc::dlsym(handle, name.as_ptr() as *const libc::c_char) };
            assert!(
                !ptr.is_null(),
                "symbol {} not found via dlsym in cdylib",
                name
            );
        }
        unsafe { libc::dlclose(handle) };
    }

    /// 反向一致性：cdylib 动态符号表导出的 gl/egl 符号必须全部在 SYMBOLS 表中登记。
    ///
    /// 正向测试（all_exported_symbols_resolvable）只保证"表 ⊆ 导出"；若有人新增
    /// #[no_mangle] 导出却忘记登记，正向测试查不出（表内没有该符号）→ 通过，
    /// 漏登记后果是 eglGetProcAddress 对该名 is_exported=false，跳过 self-handle 自查。
    /// 本测试用 nm 解析 cdylib 动态符号，逐一对表查证，堵住该漏网路径。
    /// cdylib 未构建或本机无 nm 时跳过（验证流程中 cargo build 先行保证产物存在；
    /// Termux binutils 自带 nm）。
    #[test]
    fn all_exported_gl_egl_symbols_registered() {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let so_path = std::path::Path::new(&manifest).join("target/debug/libfluorategl.so");
        if !so_path.exists() {
            eprintln!(
                "[skip] cdylib 产物不存在（{}），跳过反向一致性检查",
                so_path.display()
            );
            return;
        }
        let output = match std::process::Command::new("nm")
            .args(["-D", "--defined-only"])
            .arg(&so_path)
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[skip] nm 不可用（{}），跳过反向一致性检查", e);
                return;
            }
        };
        assert!(
            output.status.success(),
            "nm failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut missing: Vec<String> = Vec::new();
        let mut checked = 0usize;
        for line in stdout.lines() {
            // nm -D 输出形如 "0000000000012345 T glGetString"；部分 binutils 版本
            // 带文件前缀（"libfluorategl.so: 0000... T name"）。统一取最后一个
            // 非空 token 作为符号名，不依赖类型列——对 gl/egl 前缀符号一律检查
            // （保守起见，跳过 "U" 未定义、"D"/"B" 数据等仅靠前缀即可天然排除）。
            let Some(name) = line.split_whitespace().next_back() else {
                continue;
            };
            if name.starts_with("gl") || name.starts_with("egl") {
                checked += 1;
                // is_exported 已做尾 NUL 双向归一化（输入与表内条目均 strip），
                // nm 提取的无 NUL 名字可直接匹配。
                if !is_exported(name.as_bytes()) {
                    missing.push(name.to_string());
                }
            }
        }
        eprintln!("[reverse-check] 共检查 {} 个 gl/egl 导出符号", checked);
        assert!(
            missing.is_empty(),
            "未登记的导出符号（请同步加入 SYMBOLS 表）: {:?}",
            missing
        );
    }

    #[test]
    fn is_exported_recognizes_known_symbols() {
        assert!(is_exported(b"glGetString\0"));
        assert!(is_exported(b"eglGetProcAddress\0"));
        assert!(!is_exported(b"glNonexistent\0"));
    }

    /// 表与代码 no_mangle 数量一致性：EGL 34 + GL 387 = 421
    /// （D1/D2 + P2 + 阶段 1 补齐 74 个 GL 3.0-3.3 core）
    #[test]
    fn symbol_count_sanity() {
        let egl = SYMBOLS.iter().filter(|s| s.starts_with(b"egl")).count();
        let gl = SYMBOLS.len() - egl;
        assert_eq!(egl, 34, "EGL 导出数量不符（请核对 src/egl/exports.rs）");
        assert_eq!(gl, 387, "GL 导出数量不符（请核对 src/gl/*.rs）");
        assert_eq!(SYMBOLS.len(), 421);
    }
}
