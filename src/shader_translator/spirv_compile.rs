//! GLSL → SPIR-V 编译模块
//!
//! 使用 glslang 将桌面 GLSL 编译为 SPIR-V 字节码。
//!
//! 本模块绕过 glslang crate 的 `Shader::new` 封装，直接调用 glslang_sys 的 C API，
//! 以便在 parse 之前设置 `VULKAN_RULES_RELAXED`。
//!
//! 背景：glslang 0.8.1 的 `Shader::new` 内部连续调用 create → set_preamble →
//! preprocess → parse，导致 `shader.options()` 只能在 parse 之后才被调用。
//! 而 `VULKAN_RULES_RELAXED` 必须在 parse 之前设置才能放宽 non-opaque uniform
//! 检查（C++ 端 `ParseHelper.cpp` 的 `transparentOpaqueCheck` 在 parse 阶段执行）。
//!
//! 直接调用 C API 的顺序：
//! 1. glslang_shader_create — 创建空 shader
//! 2. glslang_shader_set_options(VULKAN_RULES_RELAXED | ...) — 在 parse 前设置
//! 3. glslang_shader_set_preamble
//! 4. glslang_shader_preprocess
//! 5. glslang_shader_parse
//! 6. glslang_program_create / add_shader / link / SPIRV_generate
//!
//! Vulkan target 要求：GLSL >= 140，所有 in/out 有 location，UBO/SSBO 有 binding。
//! VULKAN_RULES_RELAXED 放宽 non-opaque uniform 检查（glslang 内部自动包装进
//! `$GlobalBlock`），preprocess 负责注入 location/binding。

use glslang::limits::DEFAULT_LIMITS;
use glslang_sys as sys;
use std::ffi::{CStr, CString};

// GL shader stage 常量
pub const GL_VERTEX_SHADER: u32 = 0x8B31;
pub const GL_FRAGMENT_SHADER: u32 = 0x8B30;
pub const GL_GEOMETRY_SHADER: u32 = 0x8DD9;
pub const GL_TESS_CONTROL_SHADER: u32 = 0x8E88;
pub const GL_TESS_EVALUATION_SHADER: u32 = 0x8E87;
pub const GL_COMPUTE_SHADER: u32 = 0x91B9;

/// RAII guard：确保 shader 在任何路径下都被释放
struct ShaderHandle(*mut sys::glslang_shader_t);

impl Drop for ShaderHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { sys::glslang_shader_delete(self.0) }
        }
    }
}

/// RAII guard：确保 program 在任何路径下都被释放
struct ProgramHandle(*mut sys::glslang_program_t);

impl Drop for ProgramHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { sys::glslang_program_delete(self.0) }
        }
    }
}

/// 读取 shader 的 info log
///
/// # Safety
/// `shader` 必须是有效的 glslang_shader_t 指针
unsafe fn shader_log(shader: *mut sys::glslang_shader_t) -> String {
    let ptr = unsafe { sys::glslang_shader_get_info_log(shader) };
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// 读取 program 的 info log
///
/// # Safety
/// `program` 必须是有效的 glslang_program_t 指针
unsafe fn program_log(program: *mut sys::glslang_program_t) -> String {
    let ptr = unsafe { sys::glslang_program_get_info_log(program) };
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// 同时输出 ERROR 到 log 框架和 stderr（诊断用途）
///
/// 背景：方案C上线后发现 compile() 返回 None 但 log::error! 日志缺失
/// （疑似 Android logcat 缓冲/flush 时机问题）。stderr 能绕过 log 框架
/// 直接写入 fd，确保关键失败信息可见。在 Android 上表现为 System.err tag。
/// 诊断期使用，定位到根因后可移除 eprintln!。
fn error_dual(msg: &str) {
    log::error!("{}", msg);
    eprintln!("{}", msg);
}

/// 强制同步输出到 stderr（持锁 write_all + flush），诊断用途
///
/// 背景：日志后端在打印大段 preprocessed source 后，后续小日志可能被
/// 缓冲区串写/截断（如 "GLES info log: ERRinecraft:core/terrain" 交错）。
/// 此函数持 stderr 锁完整写出，避免与其他线程交错。
fn stderr_sync(msg: &str) {
    use std::io::Write;
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let _ = handle.write_all(msg.as_bytes());
    let _ = handle.write_all(b"\n");
    let _ = handle.flush();
}

/// 将 preprocessed source 完整写入文件，绕过日志后端截断
///
/// 日志后端只打印了前 2000 chars，大 shader（5302 chars）后 3300 chars
/// 未知，无法判断 parse 崩溃是否由后续内容触发。写文件可拿到完整源码，
/// 便于本地用 glslangValidator 命令行复现。
///
/// 写入路径优先级：
/// 1. /data/local/tmp/shader_<stage>_<n>.glsl（FCL 环境 app 可写）
/// 2. 失败则跳过（不影响主流程）
fn dump_preprocessed_to_file(stage: u32, source: &str) {
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = format!("/data/local/tmp/shader_{:04X}_{}.glsl", stage, n);
    match fs::write(&path, source) {
        Ok(_) => {
            stderr_sync(&format!(
                "[ShaderTranslator] DUMPED preprocessed source for stage 0x{:04X} to {} ({} chars)",
                stage,
                path,
                source.len()
            ));
        }
        Err(e) => {
            stderr_sync(&format!(
                "[ShaderTranslator] FAILED to dump preprocessed source to {}: {} (errno {:?})",
                path,
                e,
                e.raw_os_error()
            ));
        }
    }
}

/// 将 GLSL 源码编译为 SPIR-V 字节码
///
/// 流程：预处理 → glslang create → set_options(VULKAN_RULES_RELAXED) →
///       preprocess → parse → program link → SPIRV generate
///
/// 失败时返回 None 并输出 error 级别日志（同时写 stderr 防吞）。
pub fn compile(source: &str, stage: u32) -> Option<Vec<u32>> {
    // 确保全局 glslang process 已初始化（借用 glslang crate 的全局 OnceLock）
    if glslang::Compiler::acquire().is_none() {
        error_dual(
            "[ShaderTranslator] glslang compiler not available (glslang_initialize_process failed)",
        );
        return None;
    }

    let glsl_stage = match map_gl_stage(stage) {
        Some(s) => s,
        None => {
            error_dual(&format!(
                "[ShaderTranslator] map_gl_stage returned None for stage 0x{:04X}; source (first 500 chars):\n{}",
                stage,
                source.chars().take(500).collect::<String>()
            ));
            return None;
        }
    };

    // 预处理 GLSL：移除 #line、移除 /*#version*/ 注释、规范化版本、注入 location/binding
    let preprocessed = crate::shader_translator::preprocess::preprocess(source);

    // code 必须以 null 结尾，且保持存活到 parse 完成
    let code = match CString::new(preprocessed.as_str()) {
        Ok(c) => c,
        Err(e) => {
            error_dual(&format!(
                "[ShaderTranslator] GLSL source contains null byte for stage 0x{:04X}: {:?}",
                stage, e
            ));
            return None;
        }
    };

    // resource 指针：DEFAULT_LIMITS 是 'static const，repr(transparent) 保证
    // ResourceLimits 与 sys::glslang_resource_t 内存布局一致
    let resource: *const sys::glslang_resource_t =
        &DEFAULT_LIMITS as *const _ as *const sys::glslang_resource_t;

    // 构造 input（栈上，保持存活到 parse 完成）
    let input = sys::glslang_input_t {
        language: sys::glslang_source_t::GLSL,
        stage: glsl_stage,
        client: sys::glslang_client_t::Vulkan,
        client_version: sys::glslang_target_client_version_t::Vulkan1_2,
        target_language: sys::glslang_target_language_t::SPIRV,
        target_language_version: sys::glslang_target_language_version_t::SPIRV1_5,
        code: code.as_ptr(),
        default_version: 100,
        default_profile: sys::glslang_profile_t::None,
        force_default_version_and_profile: 0,
        forward_compatible: 0,
        messages: sys::glslang_messages_t::SUPPRESS_WARNINGS,
        resource,
        callbacks: sys::glsl_include_callbacks_s {
            include_system: None,
            include_local: None,
            free_include_result: None,
        },
        callbacks_ctx: std::ptr::null_mut(),
    };

    // 步骤 1：创建 shader
    log::debug!(
        "[ShaderTranslator] step1: calling glslang_shader_create for stage 0x{:04X}",
        stage
    );
    let shader_ptr = unsafe { sys::glslang_shader_create(&input) };
    if shader_ptr.is_null() {
        error_dual(&format!(
            "[ShaderTranslator] glslang_shader_create returned null for stage 0x{:04X}",
            stage
        ));
        return None;
    }
    let _shader_guard = ShaderHandle(shader_ptr);

    // 步骤 2：在 parse 之前设置 options
    // VULKAN_RULES_RELAXED 放宽 non-opaque uniform 检查
    // AUTO_MAP_BINDINGS / AUTO_MAP_LOCATIONS 自动分配 binding/location
    let options = sys::glslang_shader_options_t::AUTO_MAP_BINDINGS
        | sys::glslang_shader_options_t::AUTO_MAP_LOCATIONS
        | sys::glslang_shader_options_t::VULKAN_RULES_RELAXED;
    unsafe { sys::glslang_shader_set_options(shader_ptr, options.0) };

    // 步骤 3：设置 preamble（空，无 #define）
    let empty_preamble = CString::new("").unwrap();
    unsafe { sys::glslang_shader_set_preamble(shader_ptr, empty_preamble.as_ptr()) };

    // 步骤 4：preprocess + parse
    // 标记进入，并打印 preprocessed 内容（前 2000 chars）用于诊断
    log::info!(
        "[ShaderTranslator] ENTERING glslang preprocess+parse for stage 0x{:04X} (source {} chars, preprocessed {} chars)",
        stage,
        source.len(),
        preprocessed.len()
    );
    log::debug!(
        "[ShaderTranslator] preprocessed source for stage 0x{:04X} (first 2000 chars):\n{}",
        stage,
        preprocessed.chars().take(2000).collect::<String>()
    );
    log::logger().flush();
    // 诊断：完整 preprocessed 写入文件，绕过日志后端截断
    // 大 shader（>5000 chars）的 parse 崩溃可能与后 3000 chars 内容有关，
    // 必须拿到完整源码才能本地复现。
    dump_preprocessed_to_file(stage, &preprocessed);
    stderr_sync(&format!(
        "[ShaderTranslator] ENTERING preprocess+parse for stage 0x{:04X} (preprocessed {} chars dumped to file)",
        stage,
        preprocessed.len()
    ));

    log::debug!(
        "[ShaderTranslator] step4: calling glslang_shader_preprocess for stage 0x{:04X}",
        stage
    );
    stderr_sync(&format!(
        "[ShaderTranslator] step4: BEFORE glslang_shader_preprocess for stage 0x{:04X}",
        stage
    ));
    let preprocess_ret = unsafe { sys::glslang_shader_preprocess(shader_ptr, &input) };
    stderr_sync(&format!(
        "[ShaderTranslator] step4: AFTER glslang_shader_preprocess returned {} for stage 0x{:04X}",
        preprocess_ret,
        stage
    ));
    log::debug!(
        "[ShaderTranslator] glslang_shader_preprocess returned {} for stage 0x{:04X}",
        preprocess_ret,
        stage
    );
    if preprocess_ret == 0 {
        let log = unsafe { shader_log(shader_ptr) };
        error_dual(&format!(
            "[ShaderTranslator] glslang preprocess FAILED for stage 0x{:04X}: {}; source (first 500 chars):\n{}",
            stage,
            log,
            source.chars().take(500).collect::<String>()
        ));
        log::logger().flush();
        return None;
    }

    // 步骤 5：parse
    // parse 是疑似崩溃点。前后加 stderr_sync 标记，便于确认 parse 是否返回。
    // parse 前主动读一次 info_log（preprocess 可能已写入警告），便于对照。
    let info_before_parse = unsafe { shader_log(shader_ptr) };
    stderr_sync(&format!(
        "[ShaderTranslator] step5: BEFORE glslang_shader_parse for stage 0x{:04X} (info_log before parse: {})",
        stage,
        if info_before_parse.is_empty() { "(empty)" } else { &info_before_parse }
    ));
    log::debug!(
        "[ShaderTranslator] step5: calling glslang_shader_parse for stage 0x{:04X}",
        stage
    );
    let parse_ret = unsafe { sys::glslang_shader_parse(shader_ptr, &input) };
    // 立即读 info_log，防止后续操作清空
    let info_after_parse = unsafe { shader_log(shader_ptr) };
    stderr_sync(&format!(
        "[ShaderTranslator] step5: AFTER glslang_shader_parse returned {} for stage 0x{:04X} (info_log after parse: {})",
        parse_ret,
        stage,
        if info_after_parse.is_empty() { "(empty)" } else { &info_after_parse }
    ));
    log::debug!(
        "[ShaderTranslator] glslang_shader_parse returned {} for stage 0x{:04X}",
        parse_ret,
        stage
    );
    if parse_ret == 0 {
        error_dual(&format!(
            "[ShaderTranslator] glslang parse FAILED for stage 0x{:04X}: {}; source (first 500 chars):\n{}",
            stage,
            info_after_parse,
            source.chars().take(500).collect::<String>()
        ));
        log::logger().flush();
        return None;
    }

    // 步骤 6：创建 program 并链接
    log::debug!(
        "[ShaderTranslator] step6: calling glslang_program_create for stage 0x{:04X}",
        stage
    );
    let program_ptr = unsafe { sys::glslang_program_create() };
    if program_ptr.is_null() {
        error_dual(&format!(
            "[ShaderTranslator] glslang_program_create returned null for stage 0x{:04X}",
            stage
        ));
        return None;
    }
    let _program_guard = ProgramHandle(program_ptr);

    unsafe { sys::glslang_program_add_shader(program_ptr, shader_ptr) };

    let messages = sys::glslang_messages_t::DEFAULT
        | sys::glslang_messages_t::VULKAN_RULES
        | sys::glslang_messages_t::SPV_RULES;
    log::debug!(
        "[ShaderTranslator] step6: calling glslang_program_link for stage 0x{:04X}",
        stage
    );
    let link_ret = unsafe { sys::glslang_program_link(program_ptr, messages.0) };
    log::debug!(
        "[ShaderTranslator] glslang_program_link returned {} for stage 0x{:04X}",
        link_ret,
        stage
    );
    if link_ret == 0 {
        let log = unsafe { program_log(program_ptr) };
        error_dual(&format!(
            "[ShaderTranslator] glslang program link FAILED for stage 0x{:04X}: {}; source (first 500 chars):\n{}",
            stage,
            log,
            source.chars().take(500).collect::<String>()
        ));
        log::logger().flush();
        return None;
    }

    // 步骤 7：生成 SPIR-V
    // glslang compile 是 FFI 调用（C++ 代码），native 崩溃无法被 catch_unwind 捕获。
    // 此处用 info 级日志 + flush 标记进入，若崩溃后日志只有 "ENTERING" 则确认崩溃在 glslang 内部。
    log::info!(
        "[ShaderTranslator] ENTERING glslang SPIRV_generate for stage 0x{:04X}",
        stage
    );
    log::logger().flush();
    unsafe { sys::glslang_program_SPIRV_generate(program_ptr, glsl_stage) };

    let size = unsafe { sys::glslang_program_SPIRV_get_size(program_ptr) };
    if size == 0 {
        let log = unsafe { program_log(program_ptr) };
        error_dual(&format!(
            "[ShaderTranslator] glslang SPIRV_generate produced 0 words for stage 0x{:04X}: {}",
            stage, log
        ));
        log::logger().flush();
        return None;
    }

    let mut spv = vec![0u32; size];
    unsafe { sys::glslang_program_SPIRV_get(program_ptr, spv.as_mut_ptr()) };

    log::info!(
        "[ShaderTranslator] EXITED glslang compile OK for stage 0x{:04X} (SPIR-V {} words)",
        stage,
        spv.len()
    );
    log::logger().flush();
    Some(spv)
}

/// GL stage 常量 → glslang_stage_t 映射
pub fn map_gl_stage(stage: u32) -> Option<sys::glslang_stage_t> {
    match stage {
        GL_VERTEX_SHADER => Some(sys::glslang_stage_t::Vertex),
        GL_FRAGMENT_SHADER => Some(sys::glslang_stage_t::Fragment),
        GL_GEOMETRY_SHADER => Some(sys::glslang_stage_t::Geometry),
        GL_TESS_CONTROL_SHADER => Some(sys::glslang_stage_t::TesselationControl),
        GL_TESS_EVALUATION_SHADER => Some(sys::glslang_stage_t::TesselationEvaluation),
        GL_COMPUTE_SHADER => Some(sys::glslang_stage_t::Compute),
        _ => None,
    }
}

/// stage 常量 → 可读名称（用于日志）
pub fn stage_name(stage: u32) -> &'static str {
    match stage {
        GL_VERTEX_SHADER => "vertex",
        GL_FRAGMENT_SHADER => "fragment",
        GL_GEOMETRY_SHADER => "geometry",
        GL_TESS_CONTROL_SHADER => "tess_control",
        GL_TESS_EVALUATION_SHADER => "tess_eval",
        GL_COMPUTE_SHADER => "compute",
        _ => "unknown",
    }
}
