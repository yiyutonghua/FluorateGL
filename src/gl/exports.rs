use crate::backend;
use crate::gl::buffer::sync_persistent_buffer_if_needed;
use crate::gl::getter;
use crate::state;
use libc::c_char;
use std::ffi::CString;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// GL_ARRAY_BUFFER target
const GL_ARRAY_BUFFER: u32 = 0x8892;
/// GL_ELEMENT_ARRAY_BUFFER target
const GL_ELEMENT_ARRAY_BUFFER: u32 = 0x8893;

/// glGetIntegerv 绑定查询时 GLES ID 未在 IdMap 中找到首次告警标志
static BINDING_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：glGetIntegerv 绑定查询 GLES ID 未在 IdMap 中找到。
fn warn_binding_id_miss(pname: u32, gles_id: u32) {
    if !BINDING_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glGetIntegerv(0x{:04X}): GLES ID {} not found in IdMap, returning raw GLES ID (跨线程或资源已释放，后续将静默返回原始 GLES ID)",
            pname,
            gles_id
        );
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClear(mask: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear)(mask);
    });
}

// glAlphaFunc 是桌面 GL 固定功能，GLES 2.0+ 不支持，alpha test 在 shader 中通过 discard 实现
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glAlphaFunc(_func: u32, _ref: f32) {}

// Capabilities that exist in desktop GL but are unsupported (or always on)
// in OpenGL ES. Passing them to GLES produces `GL_INVALID_ENUM`.
pub(crate) fn is_unsupported_gles_cap(cap: u32) -> bool {
    matches!(
        cap,
        0x884F | // GL_TEXTURE_CUBE_MAP_SEAMLESS
        0x8642 | // GL_PROGRAM_POINT_SIZE
        0x0B10 | // GL_POINT_SMOOTH
        0x0B20 | // GL_LINE_SMOOTH
        0x0B41 | // GL_POLYGON_SMOOTH
        0x809D | // GL_MULTISAMPLE
        0x0B21 | // GL_LINE_STIPPLE
        0x0BC0 | // GL_ALPHA_TEST
        0x8DB9 | // GL_FRAMEBUFFER_SRGB（GLES sRGB 由 internal format 控制，无此 cap）
        0x809F | // GL_SAMPLE_ALPHA_TO_ONE（GLES 无）
        0x2A02 | // GL_POLYGON_OFFSET_LINE（GLES 仅支持 GL_POLYGON_OFFSET_FILL）
        0x2A01 // GL_POLYGON_OFFSET_POINT（GLES 仅支持 GL_POLYGON_OFFSET_FILL）
    )
}

/// 将桌面 GL enable cap 翻译为 GLES 对应 cap。
///
/// GL_PRIMITIVE_RESTART（GL 3.1 core cap，0x8F9D）在 GLES 中不存在；
/// GLES 3.0+ 使用 GL_PRIMITIVE_RESTART_FIXED_INDEX（0x8D63，固定索引
/// 0xFFFF/0xFFFFFFFF，语义等价：两者均为"启用 primitive restart"，
/// 区别仅在于 GLES 固定索引值且无法用 glPrimitiveRestartIndex 更改）。
/// 同时兜底 0x8F3D（GL_PRIMITIVE_RESTART_INDEX 的枚举值，部分宿主误将其
/// 当作 cap 传递）。其余 cap 原样返回。
pub(crate) fn translate_enable_cap(cap: u32) -> u32 {
    match cap {
        0x8F9D | // GL_PRIMITIVE_RESTART
        0x8F3D // GL_PRIMITIVE_RESTART_INDEX（宿主误传为 cap 时兜底翻译）
        => 0x8D63, // GL_PRIMITIVE_RESTART_FIXED_INDEX
        _ => cap,
    }
}

// GL_DEPTH_CLAMP：GLES 3.2 core 才引入此 cap，3.1 及以下无（直通会 INVALID_ENUM）。
// 版本感知：3.2+ 直通（MC 第三人称深度钳制依赖此 cap），3.1 过滤并首次告警。
const GL_DEPTH_CLAMP: u32 = 0x864F;
static DEPTH_CLAMP_UNSUPPORTED_WARNED: AtomicBool = AtomicBool::new(false);

/// GL_DEPTH_CLAMP 版本感知过滤：3.2+ 返回 false（可直通），否则返回 true 并首次告警。
fn depth_clamp_unsupported() -> bool {
    if crate::backend::capabilities().version.at_least(3, 2) {
        return false;
    }
    if !DEPTH_CLAMP_UNSUPPORTED_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glEnable/glDisable(GL_DEPTH_CLAMP) ignored: GLES 3.1 无此 cap（需 3.2+），深度钳制将失效（后续调用静默跳过）"
        );
    }
    true
}

// GL_DEBUG_OUTPUT：MC(blaze3d) 会启用 KHR_debug 回调抓驱动消息，但 Adreno 驱动会刷出
// 大量 PERFORMANCE 噪声（glDebugMessageControl 对 HIGH 级 PERFORMANCE 过滤无效）。
// suppress_debug_noise 已 glDisable(GL_DEBUG_OUTPUT)，这里吞掉 MC 的重新启用，保持彻底关闭。
const GL_DEBUG_OUTPUT: u32 = 0x9146;

/// glDebugMessageCallback stub — 吞掉 MC/LWJGL 注册的 KHR_debug 回调。
///
/// Adreno 驱动无视 glDisable(GL_DEBUG_OUTPUT) 和 glDebugMessageControl 过滤，
/// 在回调注册后仍持续发送 PERFORMANCE 消息（"Packing allocations" 等），
/// 每条触发 LWJGL 的 Java 堆栈转储，导致 OptiFine 纹理图集上传阶段极端缓慢。
///
/// 不注册任何回调（直接吞掉），驱动即使生成消息也无回调可调用，彻底阻断噪声。
/// 同时避免 MC 绕过拦截层通过 dlsym 找到 GLES 驱动的真实 glDebugMessageCallback。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDebugMessageCallback(
    _callback: *const std::ffi::c_void,
    _user_param: *const std::ffi::c_void,
) {
    log::debug!(
        "[FluorateGL] glDebugMessageCallback swallowed (callback not registered, driver debug noise blocked)"
    );
}

/// glDebugMessageCallbackKHR stub — 与 glDebugMessageCallback 等价的 KHR 扩展入口。
///
/// 部分 LWJGL 版本优先查询 KHR 后缀版本，提供此入口确保两条查询路径都被拦截。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDebugMessageCallbackKHR(
    _callback: *const std::ffi::c_void,
    _user_param: *const std::ffi::c_void,
) {
    log::debug!(
        "[FluorateGL] glDebugMessageCallbackKHR swallowed (callback not registered, driver debug noise blocked)"
    );
}

/// glObjectLabel stub — GL_KHR_debug 对象调试标签。
///
/// 仅用于驱动侧 debug 标注，不影响渲染管线状态或输出。已声明 GL_KHR_debug 扩展，
/// 必须导出此符号供宿主查询；GLES 标签能力非渲染必需，直接 no-op 吞掉。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glObjectLabel(_identifier: u32, _name: u32, _length: i32, _label: *const c_char) {
    log::debug!("[FluorateGL] glObjectLabel swallowed (debug label ignored, no rendering impact)");
}

/// glObjectLabelKHR stub — 与 glObjectLabel 等价的 KHR 扩展入口。
///
/// LWJGL 可能优先查询 KHR 后缀版本，提供此入口确保两条查询路径都被拦截。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glObjectLabelKHR(
    _identifier: u32,
    _name: u32,
    _length: i32,
    _label: *const c_char,
) {
    log::debug!(
        "[FluorateGL] glObjectLabelKHR swallowed (debug label ignored, no rendering impact)"
    );
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEnable(cap: u32) {
    if cap == GL_DEBUG_OUTPUT {
        log::debug!(
            "[FluorateGL] glEnable(GL_DEBUG_OUTPUT) swallowed (driver debug noise suppressed)"
        );
        return;
    }
    if is_unsupported_gles_cap(cap) {
        log::debug!(
            "[FluorateGL] glEnable(0x{:04X}) ignored (unsupported in GLES)",
            cap
        );
        return;
    }
    // M7：GL_DEPTH_CLAMP 版本感知——GLES 3.2+ 原生支持直通，3.1 过滤 + 首次告警
    if cap == GL_DEPTH_CLAMP && depth_clamp_unsupported() {
        return;
    }
    let cap = translate_enable_cap(cap);
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.enable)(cap);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDisable(cap: u32) {
    if is_unsupported_gles_cap(cap) {
        log::debug!(
            "[FluorateGL] glDisable(0x{:04X}) ignored (unsupported in GLES)",
            cap
        );
        return;
    }
    // M7：GL_DEPTH_CLAMP 版本感知——与 glEnable 对称
    if cap == GL_DEPTH_CLAMP && depth_clamp_unsupported() {
        return;
    }
    let cap = translate_enable_cap(cap);
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.disable)(cap);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDepthFunc(func: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.depth_func)(func);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDepthMask(flag: u8) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.depth_mask)(flag);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBlendFunc(sfactor: u32, dfactor: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.blend_func)(sfactor, dfactor);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearColor(r: f32, g: f32, b: f32, a: f32) {
    // 排查日志：记录 clear 色值（红屏问题定位）
    log::debug!("[FluorateGL] glClearColor({}, {}, {}, {})", r, g, b, a);
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear_color)(r, g, b, a);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearDepth(depth: f64) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear_depth)(depth as f32);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glClearStencil(s: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.clear_stencil)(s);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glViewport(x: i32, y: i32, width: i32, height: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.viewport)(x, y, width, height);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glScissor(x: i32, y: i32, width: i32, height: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.scissor)(x, y, width, height);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCullFace(mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.cull_face)(mode);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFrontFace(mode: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.front_face)(mode);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glLineWidth(width: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.line_width)(width);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glActiveTexture(texture: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.active_texture)(texture);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPixelStorei(pname: u32, param: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.pixel_store_i)(pname, param);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArrays(mode: u32, first: i32, count: i32) {
    // 同步持久映射 buffer 脏区域（若 vertex buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    let bound_vao = state::with_state(|s| s.bound_vertex_array);
    let bound_buf = state::with_state(|s| s.bound_buffer);
    log::debug!(
        "[FluorateGL] glDrawArrays(mode=0x{:04X}, first={}, count={}) bound_vao={} bound_buf={} (tid={})",
        mode,
        first,
        count,
        bound_vao,
        bound_buf,
        state::thread_id_u64()
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_arrays)(mode, first, count);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawElements(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const std::ffi::c_void,
) {
    // 同步持久映射 buffer 脏区域（若 vertex/index buffer 是持久映射的）
    sync_persistent_buffer_if_needed(GL_ARRAY_BUFFER);
    sync_persistent_buffer_if_needed(GL_ELEMENT_ARRAY_BUFFER);
    let bound_vao = state::with_state(|s| s.bound_vertex_array);
    let bound_buf = state::with_state(|s| s.bound_buffer);
    log::debug!(
        "[FluorateGL] glDrawElements(mode=0x{:04X}, count={}, type=0x{:04X}, indices={:?}) bound_vao={} bound_buf={} (tid={})",
        mode,
        count,
        type_,
        indices,
        bound_vao,
        bound_buf,
        state::thread_id_u64()
    );
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.draw_elements)(mode, count, type_, indices);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFinish() {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.finish)();
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glFlush() {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.flush)();
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGenerateMipmap(target: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.generate_mipmap)(target);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetError() -> u32 {
    let err = backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.get_error)() });
    if err != 0 {
        // MC blaze3d 每帧轮询 glGetError，驱动偶发 GL_INVALID_ENUM 会刷屏，降为 debug。
        // 真正的错误（如 shader 编译失败）会通过 fail-fast error 日志体现。
        log::debug!("[FluorateGL] glGetError() -> 0x{:04X} (GL error)", err);
    }
    err
}

fn get_fake_extensions_string() -> *const c_char {
    static EXT_STRING: OnceLock<CString> = OnceLock::new();
    let s = EXT_STRING.get_or_init(|| {
        // FAKE_EXTENSIONS 惰性构建：嵌套 OnceLock（不同实例）调用合法，无递归
        let exts = FAKE_EXTENSIONS.get_or_init(build_fake_extensions);
        let joined = exts
            .iter()
            .map(|ext| std::str::from_utf8(&ext[..ext.len() - 1]).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        CString::new(joined).unwrap_or_else(|_| CString::new("").unwrap())
    });
    s.as_ptr() as *const _
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetString(name: u32) -> *const c_char {
    let result = if name == 0x1F02 {
        // GL_VERSION：从 config::REPORTED_GL_VERSION_PREFIX 拼接 FluorateGL 版本号，
        // MC F3 的 "OpenGL:" 行显示如 "3.3.0 FluorateGL v0.2.0"。
        // 版本前缀在 config.rs 统一维护，此处不再硬编码，避免两处定义不同步。
        static VERSION: OnceLock<CString> = OnceLock::new();
        let v = VERSION.get_or_init(|| {
            CString::new(format!(
                "{} v{}",
                crate::config::REPORTED_GL_VERSION_PREFIX,
                env!("CARGO_PKG_VERSION")
            ))
            .unwrap_or_else(|_| CString::new("").unwrap())
        });
        v.as_ptr() as *const c_char
    } else if name == 0x8B8C {
        // GL_SHADING_LANGUAGE_VERSION
        static GLSL: OnceLock<CString> = OnceLock::new();
        let s = GLSL.get_or_init(|| CString::new(crate::config::REPORTED_GLSL_VERSION).unwrap());
        s.as_ptr() as *const c_char
    } else if name == 0x1F03 {
        // GL_EXTENSIONS
        get_fake_extensions_string()
    } else {
        let raw = backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.get_string)(name) });
        // ANGLE 等后端可能在上下文未完全初始化时返回 null，兜底返回 "Unknown"
        // 避免 GpuDevice.getRenderer() → NullPointerException
        if raw.is_null() {
            match name {
                0x1F01 => {
                    static UNKNOWN_RENDERER: &[u8] = b"Unknown\0";
                    UNKNOWN_RENDERER.as_ptr() as *const c_char
                }
                0x1F00 => {
                    static UNKNOWN_VENDOR: &[u8] = b"Unknown\0";
                    UNKNOWN_VENDOR.as_ptr() as *const c_char
                }
                _ => raw,
            }
        } else {
            raw
        }
    };
    log::debug!("[FluorateGL] glGetString(0x{:04X}) -> {:?}", name, result);
    result
}

/// 伪造扩展表（惰性构建）：BASE_EXTENSIONS + 按 capabilities 动态校验的行为依赖型扩展。
///
/// 行为依赖型扩展（draw_indirect / multi_draw_indirect / base_vertex / base_instance 等）
/// 声明了但真实 capabilities 不支持时，宿主查询扩展后调用 stub 函数会崩溃，
/// 因此由 build_fake_extensions() 在构建时校验剔除，保证声明与真实能力对齐。
static FAKE_EXTENSIONS: OnceLock<Vec<&'static [u8]>> = OnceLock::new();

/// 基础扩展表：静态声明（桌面名 GL_ARB_* 或 GLES 通用名，均为能力无关的纯特性声明）。
///
/// 行为依赖型扩展（draw_indirect / draw_elements_base_vertex / base_instance /
/// multi_draw_elements_base_vertex 等）已从本表移除，统一由
/// build_fake_extensions() 内的 behavior_dependent 映射表按 caps 动态声明
/// （caps=true 才 push），避免 caps=false 时仍被静态声明导致 warn 日志撒谎
/// 与宿主调用 stub 崩溃（S1 修复）。
static BASE_EXTENSIONS: &[&[u8]] = &[
    b"GL_ARB_vertex_array_object\0",
    b"GL_ARB_framebuffer_object\0",
    b"GL_ARB_instanced_arrays\0",
    b"GL_ARB_uniform_buffer_object\0",
    b"GL_ARB_shader_storage_buffer_object\0",
    b"GL_ARB_shader_image_load_store\0",
    b"GL_ARB_vertex_attrib_binding\0",
    // 行为依赖型扩展（GL_ARB_draw_indirect / GL_ARB_draw_elements_base_vertex /
    // GL_OES_draw_elements_base_vertex / GL_ARB_base_instance / GL_EXT_base_instance /
    // GL_EXT_multi_draw_elements_base_vertex）已移入 build_fake_extensions() 的
    // behavior_dependent 映射表动态声明，本表不再静态包含。
    b"GL_ARB_timer_query\0",
    b"GL_ARB_buffer_storage\0",
    b"GL_ARB_clear_texture\0",
    b"GL_ARB_draw_buffers_blend\0",
    b"GL_ARB_depth_texture\0",
    b"GL_ARB_ES3_compatibility\0",
    b"GL_ARB_shading_language_100\0",
    b"GL_ARB_texture_storage\0",
    b"GL_ARB_texture_float\0",
    b"GL_ARB_texture_half_float\0",
    b"GL_ARB_texture_rg\0",
    b"GL_ARB_texture_compression_bptc\0",
    b"GL_ARB_texture_compression_rgtc\0",
    b"GL_EXT_texture_compression_s3tc\0",
    b"GL_EXT_texture_filter_anisotropic\0",
    b"GL_EXT_texture_sRGB\0",
    b"GL_EXT_color_buffer_float\0",
    b"GL_KHR_debug\0",
    b"GL_KHR_no_error\0",
    b"GL_KHR_texture_compression_astc_ldr\0",
    b"GL_KHR_texture_compression_astc_hdr\0",
    b"GL_OES_texture_float\0",
    b"GL_OES_texture_half_float\0",
    b"GL_OES_texture_half_float_linear\0",
    b"GL_EXT_geometry_shader\0",
    b"GL_EXT_tessellation_shader\0",
    b"GL_EXT_texture_cube_map_array\0",
    b"GL_EXT_gpu_shader5\0",
    b"GL_EXT_draw_buffers_indexed\0",
    b"GL_EXT_copy_image\0",
    b"GL_EXT_texture_border_clamp\0",
    b"GL_EXT_texture_buffer\0",
    b"GL_EXT_shader_framebuffer_fetch\0",
    b"GL_OES_standard_derivatives\0",
    b"GL_OES_element_index_uint\0",
    b"GL_OES_texture_npot\0",
    b"GL_OES_depth_texture\0",
    b"GL_OES_packed_depth_stencil\0",
    b"GL_OES_rgb8_rgba8\0",
];

/// 行为依赖型扩展 → GlesCapabilities 字段的映射校验。
/// 声明了但 caps=false → 剔除 + warn（防止宿主查询后调用 stub 函数崩溃）。
///
/// S1：base 表不再静态包含行为依赖条目，全部由本表按 caps 动态声明
/// （caps=true 才 push，名字与移除前完全一致，含尾 \0）。
fn build_fake_extensions() -> Vec<&'static [u8]> {
    // S2：caps 可能未就绪（glGetString/glGetStringi 不经 with_gles_dispatch，不触发能力查询）。
    // 宿主在此刻调用说明 GL 上下文已绑定（GLES_DISPATCH 已设置）——先补一次能力查询，
    // 避免构建时拿到 FALLBACK_CAPS（multi_draw_indirect=false 等）导致行为依赖扩展被
    // 错误剔除，OnceLock 一次性定型后造成 GLES 3.2 设备性能回归。
    if backend::gles_dispatch_ready() && !backend::caps_queried() {
        backend::query_capabilities_now();
    }
    let mut result = BASE_EXTENSIONS.to_vec();
    // 纯 stub 场景（GLES_DISPATCH 未设置）：无真实 GL 上下文，无法查询 caps，
    // 返回 BASE_EXTENSIONS 原样拷贝（不剔除不添加），与历史兜底行为一致。
    if !backend::gles_dispatch_ready() {
        return result;
    }
    let caps = crate::backend::capabilities();
    let behavior_dependent: &[(
        &[u8],
        fn(&crate::backend::capabilities::GlesCapabilities) -> bool,
    )] = &[
        (b"GL_ARB_draw_indirect\0", |c| c.indirect_draw),
        // 差异 #3：multi_draw_indirect 二选一保留 GL_EXT 名，GL_ARB 名不再声明
        (b"GL_EXT_multi_draw_indirect\0", |c| c.multi_draw_indirect),
        (b"GL_ARB_draw_elements_base_vertex\0", |c| {
            c.draw_elements_base_vertex
        }),
        (b"GL_OES_draw_elements_base_vertex\0", |c| {
            c.draw_elements_base_vertex
        }),
        (b"GL_ARB_base_instance\0", |c| c.base_instance),
        (b"GL_EXT_base_instance\0", |c| c.base_instance),
        (b"GL_EXT_multi_draw_elements_base_vertex\0", |c| {
            c.multi_draw_elements_base_vertex
        }),
    ];
    // 行为依赖字段全 false 时提示：S2 已保证构建前 caps 就绪（真实查询或 stub 早退），
    // 此处仅剩"GLES 3.1 无行为依赖特性扩展"的真实剔除场景（如 3.1 设备无
    // GL_OES_draw_elements_base_vertex / GL_EXT_base_instance / multi_draw_indirect）。
    if !caps.multi_draw_indirect
        && !caps.draw_elements_base_vertex
        && !caps.base_instance
        && !caps.multi_draw_elements_base_vertex
    {
        log::debug!(
            "[FluorateGL] FAKE_EXTENSIONS 构建时无行为依赖特性支持（multi_draw_indirect/base_vertex/base_instance 全 false），使用保守剔除结果"
        );
    }
    for (ext, pred) in behavior_dependent {
        let supported = pred(caps);
        if !supported {
            log::warn!(
                "[FluorateGL] FAKE_EXTENSIONS 剔除 {}（capabilities 不支持）",
                String::from_utf8_lossy(&ext[..ext.len() - 1])
            );
        } else if !result.iter().any(|e| *e == *ext) {
            result.push(*ext);
        }
    }
    result
}

/// 将 GLES 驱动返回的绑定查询结果中的原始 GLES ID 翻译为桌面 ID。
/// 如果 pname 不是绑定查询，或返回值为 0，则不做任何修改。
fn translate_binding_to_desktop(pname: u32, data: *mut i32) {
    let gles_id = unsafe { *data } as u32;
    if gles_id == 0 {
        return;
    }

    let desktop_id = match pname {
        // Buffer 绑定查询 → buffers IdMap
        0x8894 | // GL_ARRAY_BUFFER_BINDING
        0x8895 | // GL_ELEMENT_ARRAY_BUFFER_BINDING
        0x8A28 | // GL_UNIFORM_BUFFER_BINDING
        0x8F36 | // GL_COPY_READ_BUFFER_BINDING
        0x8F37 | // GL_COPY_WRITE_BUFFER_BINDING
        0x8C8F | // GL_TRANSFORM_FEEDBACK_BUFFER_BINDING
        0x88ED | // GL_PIXEL_PACK_BUFFER_BINDING
        0x88EF | // GL_PIXEL_UNPACK_BUFFER_BINDING
        0x8F43 | // GL_DRAW_INDIRECT_BUFFER_BINDING
        0x90D3 | // GL_SHADER_STORAGE_BUFFER_BINDING
        0x92C1 // GL_ATOMIC_COUNTER_BUFFER_BINDING
        => {
            state::with_state(|s| s.buffers.get_desktop(gles_id))
        }
        // Vertex Array 绑定 → vertex_arrays IdMap
        0x85B5 /* GL_VERTEX_ARRAY_BINDING */ => {
            state::with_state(|s| s.vertex_arrays.get_desktop(gles_id))
        }
        // Program 绑定 → programs IdMap
        0x8B8D /* GL_CURRENT_PROGRAM */ => {
            state::with_state(|s| s.programs.get_desktop(gles_id))
        }
        // Texture 绑定 → textures IdMap
        0x8069 | // GL_TEXTURE_BINDING_2D
        0x806A | // GL_TEXTURE_BINDING_3D
        0x8C1D | // GL_TEXTURE_BINDING_2D_ARRAY
        0x8514 | // GL_TEXTURE_BINDING_CUBE_MAP
        0x9104 | // GL_TEXTURE_BINDING_2D_MULTISAMPLE
        0x9105 | // GL_TEXTURE_BINDING_2D_MULTISAMPLE_ARRAY
        0x900A | // GL_TEXTURE_BINDING_CUBE_MAP_ARRAY
        0x8C2C // GL_TEXTURE_BINDING_BUFFER
        => {
            state::with_state(|s| s.textures.get_desktop(gles_id))
        }
        // Framebuffer 绑定 → framebuffers IdMap
        0x8CA6 | // GL_DRAW_FRAMEBUFFER_BINDING (= GL_FRAMEBUFFER_BINDING)
        0x8CAA // GL_READ_FRAMEBUFFER_BINDING
        => {
            state::with_state(|s| s.framebuffers.get_desktop(gles_id))
        }
        // Renderbuffer 绑定 → renderbuffers IdMap
        0x8CA7 /* GL_RENDERBUFFER_BINDING */ => {
            state::with_state(|s| s.renderbuffers.get_desktop(gles_id))
        }
        _ => return, // 不是绑定查询，无需翻译
    };

    if let Some(desktop_id) = desktop_id {
        if desktop_id != gles_id {
            unsafe { *data = desktop_id as i32 };
        }
    } else {
        warn_binding_id_miss(pname, gles_id);
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetIntegerv(pname: u32, data: *mut i32) {
    if data.is_null() {
        return;
    }
    match pname {
        0x821D => {
            // GL_NUM_EXTENSIONS：可能早于 with_gles_dispatch 触发的能力查询到达
            // （静态分支不经 getter），但 build_fake_extensions 内部已有 S2 能力补查，
            // 重复 get_or_init 调用天然幂等，安全。
            unsafe { *data = FAKE_EXTENSIONS.get_or_init(build_fake_extensions).len() as i32 };
        }
        0x821B => {
            // GL_MAJOR_VERSION
            unsafe { *data = crate::config::REPORTED_GL_MAJOR };
        }
        0x821C => {
            // GL_MINOR_VERSION
            unsafe { *data = crate::config::REPORTED_GL_MINOR };
        }
        0x9126 => {
            // GL_CONTEXT_PROFILE_MASK
            unsafe { *data = 0x00000001 }; // GL_CONTEXT_CORE_PROFILE_BIT
        }
        0x821E => {
            // GL_CONTEXT_FLAGS：GLES 无此 pname（直通会 INVALID_ENUM 且不写 data），
            // 拦截返回 0（非 debug / 非 forward-compatible context）
            unsafe { *data = 0 };
        }
        0x8E4F => {
            // GL_PROVOKING_VERTEX：GLES 无此 pname，固定为 LAST_VERTEX_PROVOKING
            // （与 glProvokingVertex no-op 的语义一致）
            unsafe { *data = 0x8E65 }; // GL_LAST_VERTEX_PROVOKING
        }
        _ => {
            getter::get_integerv(pname, data);
            translate_binding_to_desktop(pname, data);
        }
    }
    log::debug!(
        "[FluorateGL] glGetIntegerv(0x{:04X}) -> {}",
        pname,
        unsafe { *data }
    );
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetStringi(name: u32, index: u32) -> *const c_char {
    // L3：FAKE_EXTENSIONS 仅在 GL_EXTENSIONS 分支内惰性构建，
    // 其他 name 直接走越界空串逻辑，避免无关查询触发扩展表构建。
    // （构建内部已有 S2 能力补查，与 glGetIntegerv(GL_NUM_EXTENSIONS) /
    //   get_fake_extensions_string 的 get_or_init 重复调用天然幂等，安全。）
    let result = if name == 0x1F03 {
        let exts = FAKE_EXTENSIONS.get_or_init(build_fake_extensions);
        if (index as usize) < exts.len() {
            exts[index as usize].as_ptr() as *const c_char
        } else {
            // 越界返回空串而非 null：防宿主不判 null 直接解引用崩溃
            static EMPTY: &[u8] = b"\0";
            EMPTY.as_ptr() as *const c_char
        }
    } else {
        // 越界返回空串而非 null：防宿主不判 null 直接解引用崩溃
        static EMPTY: &[u8] = b"\0";
        EMPTY.as_ptr() as *const c_char
    };
    log::debug!(
        "[FluorateGL] glGetStringi(0x{:04X}, {}) -> {:?}",
        name,
        index,
        result
    );
    result
}

#[cfg(test)]
mod tests {
    use super::glGetStringi;

    /// 越界索引应返回非 null 的空串指针（首字节 '\0'），
    /// 而非 null 指针：防宿主不判 null 直接解引用崩溃。
    #[test]
    fn gl_get_stringi_out_of_range_returns_empty_string() {
        let ptr = glGetStringi(0x1F03, 9999);
        assert!(!ptr.is_null(), "越界索引应返回非 null 指针");
        unsafe {
            assert_eq!(*ptr, 0, "返回的指针应指向空串（首字节为 '\\0'）");
        }
    }

    /// 合法索引仍应返回对应扩展名（回归保护）。
    #[test]
    fn gl_get_stringi_valid_index_returns_extension() {
        let ptr = glGetStringi(0x1F03, 0);
        assert!(!ptr.is_null(), "合法索引应返回非 null 指针");
        unsafe {
            let name = std::ffi::CStr::from_ptr(ptr).to_bytes();
            assert_eq!(name, b"GL_ARB_vertex_array_object");
        }
    }
}
