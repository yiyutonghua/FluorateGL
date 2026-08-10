use crate::backend;
use crate::gl::getter;
use crate::gl::pixel;
use crate::state;
use libc::c_char;
use std::collections::VecDeque;
use std::ffi::CString;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

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

// 注：ANGLE depth-clear workaround 已按用户指令 m00313 决定不做
// （原基础版：FLUORATEGL_ANGLE_DEPTH_CLEAR_FIX 环境变量开关 + glClearBufferfv
// 重放，已移除；对照 MG gl.cpp:158-185）。glClear 保持纯透传。

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
//
// 注意：本域改造后由 enable_state 虚拟 enable 表取代此函数（表按 backing 属性
// 决定转发/仅记录），保留定义仅为兼容并行域（buffer/drawing 等）可能的引用；
// 新代码应走 enable_state。
#[allow(dead_code)]
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
/// GLES 3.0+ 使用 GL_PRIMITIVE_RESTART_FIXED_INDEX（0x8D69，固定索引
/// 0xFFFF/0xFFFFFFFF，语义等价：两者均为"启用 primitive restart"，
/// 区别仅在于 GLES 固定索引值且无法用 glPrimitiveRestartIndex 更改）。
/// 同时兜底 0x8F3D（GL_PRIMITIVE_RESTART_INDEX 的枚举值，部分宿主误将其
/// 当作 cap 传递）。其余 cap 原样返回。
///
/// 注意：本域改造后仅 enable_state 的 GL_PRIMITIVE_RESTART 联动路径使用
/// 该语义（见模块头注释 2），此独立函数保留仅为兼容并行域可能的引用。
#[allow(dead_code)]
pub(crate) fn translate_enable_cap(cap: u32) -> u32 {
    match cap {
        0x8F9D | // GL_PRIMITIVE_RESTART
        0x8F3D // GL_PRIMITIVE_RESTART_INDEX（宿主误传为 cap 时兜底翻译）
        => 0x8D69, // GL_PRIMITIVE_RESTART_FIXED_INDEX（Khronos gl3.h，GLES 3.0+ 合法 cap）
        _ => cap,
    }
}

// GL_DEPTH_CLAMP 与 GL_DEBUG_OUTPUT 的处理已并入 enable_state 虚拟 enable 表
// （见文件尾部 enable_state 模块）：
// - GL_DEPTH_CLAMP：BK_EXT，ext_backing_present 保留版本感知（GLES 3.2 core
//   引入此 cap，3.2+ 直通；3.1 或扩展缺失时仅记录 + 不转发）。
// - GL_DEBUG_OUTPUT：标为 VIRTUAL（吞掉 MC 的重新启用，保持 Adreno 驱动
//   debug 噪声抑制设计，见 backend/mod.rs suppress_debug_noise）。

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

/// glEnable — 虚拟 enable 状态表驱动（移植 MobileGlues enable.cpp）。
///
/// MG 语义：写 enable 表（scalar / blend_indexed / scissor_indexed /
/// clip_distance_mask），按 per-cap 属性决定是否转发 GLES 驱动
/// （BK_NATIVE 恒转发 / BK_EXT 扩展存在才转发 / BK_VIRTUAL 仅记录）。
/// 与原透传实现的差异见 getter.rs glIsEnabled 的注释（表回答 vs 驱动透传）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEnable(cap: u32) {
    enable_state::gl_enable(cap);
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDisable(cap: u32) {
    enable_state::gl_disable(cap);
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

/// glPixelStorei — 桌面 6 个 GLES 无对应参数影子存储 + 其余透传驱动。
///
/// 移植 MG texture.cpp:2012-2023 + pixel.cpp 语义：GLES 没有
/// GL_UNPACK_SWAP_BYTES / GL_UNPACK_LSB_FIRST / GL_PACK_SWAP_BYTES /
/// GL_PACK_LSB_FIRST / GL_PACK_IMAGE_HEIGHT / GL_PACK_SKIP_IMAGES，
/// 直通会 INVALID_ENUM 且无法读回；这 6 个参数存入影子表（pixel.rs
/// pixel_store），其余（UNPACK_ALIGNMENT / UNPACK_ROW_LENGTH 等）转发驱动。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPixelStorei(pname: u32, param: i32) {
    if pixel::pixel_store::set(pname, param) {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.pixel_store_i)(pname, param);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawArrays(mode: u32, first: i32, count: i32) {
    // 跨域协调（域 1）：buffer.rs 替换为 MG 式实现后已删除
    // sync_persistent_buffer_if_needed；若域 1 提供持久映射脏区同步的新入口，
    // 需在此接回（见 TODO 协调记录）。
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
    // 跨域协调（域 1）：buffer.rs 替换为 MG 式实现后已删除
    // sync_persistent_buffer_if_needed；若域 1 提供持久映射脏区同步的新入口，
    // 需在此接回（见 TODO 协调记录）。
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
        // Primitive restart 分支（D4，对齐 MobileGlues drawing.cpp/restart.cpp）：
        // 应用侧自定义 restart index（≠ 固定哨兵）时 GLES 的 FIXED_INDEX 语义
        // 不匹配，需在 draw 前把索引流重写为固定哨兵（MG
        // mg_restart_needs_rewrite → mg_draw_elements_restart）。
        // restart_needs_rewrite 用驱动 glIsEnabled(FIXED_INDEX) 判定
        // （应用 GL_PRIMITIVE_RESTART 经 exports.rs 翻译必然反映在驱动上），
        // draw_elements_restart_rewrite 返回 true 表示已重写并重画（含
        // basevertex=0、instancecount=-1 的非 instanced 语义）；false（索引
        // 非法/不可读/count<=0）则 fallthrough 原样 draw（restart 丢失，
        // best-effort——MG 同款策略）。glDrawArrays 无索引流，无需 restart。
        if crate::gl::drawing::restart_needs_rewrite(dispatch, type_)
            && crate::gl::drawing::draw_elements_restart_rewrite(
                dispatch, mode, count, type_, indices, 0, -1,
            )
        {
            return;
        }
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

/// 待注入 GL 错误队列（C5：drawcount<0 等 GL 3.3 规范错误需在导出层产生，
/// 因为 GLES 驱动无法生成桌面特有错误场景）。FIFO 顺序：先注入先返回。
static INJECTED_GL_ERRORS: Mutex<VecDeque<u32>> = Mutex::new(VecDeque::new());

/// 注入一个 GL 错误码，将在下一次 `glGetError` 时返回（FIFO）。
///
/// 用于模拟层无法通过 GLES 驱动产生的规范错误（如 glMultiDraw* 的负 drawcount
/// → GL_INVALID_VALUE）。注入队列优先于 GLES 驱动错误队列返回，模拟层在
/// 注入前未调用任何 GLES 函数，因此不会打乱两端错误顺序。
pub(crate) fn inject_gl_error(err: u32) {
    INJECTED_GL_ERRORS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .push_back(err);
}

// 前端单槽 GL 错误（移植 MG mg.cpp/mg.h mg_set_gl_error 语义）。
//
// MG 语义：单槽、first-wins（先出现的错误优先，后来的模糊失败不覆盖
// 能解释宿主错误的第一个错误）、读取时消费并清零。用于本层自身产生的
// 规范错误（如 glPixelStorei 负 count → GL_INVALID_VALUE，见 pixel.rs）。
// 与 MG 不同：MG 是 thread_local 且 glGetError 恒吞错返回 GL_NO_ERROR；
// 我们保留返回真实错误的 fail-open 语义（差分测试依赖），槽用
// thread_local 与 GL 上下文的线程绑定一致。
thread_local! {
    static FRONTEND_GL_ERROR: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// 记录一个前端错误（first-wins：槽非空时不覆盖）。
pub(crate) fn set_gl_error(err: u32) {
    if err == 0 {
        return;
    }
    FRONTEND_GL_ERROR.with(|slot| {
        if slot.get() == 0 {
            slot.set(err);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetError() -> u32 {
    // 优先级：注入队列（我们差分测试机制）> 前端单槽（MG mg_set_gl_error 语义）
    // > 驱动错误队列（fail-open 保留真实错误，MG 为恒吞错返回 GL_NO_ERROR）。
    // 三者都会消费：注入队列 pop、前端槽清零、驱动错误队列由驱动自身消费。
    if let Some(err) = INJECTED_GL_ERRORS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .pop_front()
    {
        log::debug!("[FluorateGL] glGetError() -> 0x{:04X} (injected)", err);
        return err;
    }
    let frontend = FRONTEND_GL_ERROR.with(|slot| slot.replace(0));
    if frontend != 0 {
        log::debug!("[FluorateGL] glGetError() -> 0x{:04X} (frontend)", frontend);
        return frontend;
    }
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
    // GL_ARB_buffer_storage：**按 caps 动态声明**（用户指令 m00315「应该声明」），
    // 由 build_fake_extensions() 的 behavior_dependent 映射表校验
    // （caps.buffer_storage = 驱动支持 GL_EXT_buffer_storage 才声明），
    // 不静态包含在本表。
    // ⚠️ 历史事故风险（commit 85c6e1e）：MC 1.21.11 (via FCL) 检测到该扩展后
    // 走 BufferStorage 路径（fwy$a），GUI per-draw UBO 池（创建 flags=0x0000，
    // usage 无 MAP 位）不建立持久映射（fxa.e=null）→ 每帧
    // CommandEncoder.mapBuffer 在 Java 层抛 "Somehow trying to map an
    // unmappable buffer" 异常被吞 → 池零写入 → UI 矩阵塌缩消失。
    // 现状变化：buffer 域已替换为 MG 式实现（驱动支持 EXT_buffer_storage 时
    // glBufferStorage 透传持久语义，flags=0 不再有旧版特判），声明后 MC 将走
    // BufferStorage 路径——真机验证（Adreno 支持 EXT_buffer_storage）是最终
    // 裁决；若 UI 塌缩复现需回滚本声明。
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
        // GL_ARB_buffer_storage（用户指令 m00315）：GLES 对应扩展为
        // GL_EXT_buffer_storage——驱动真实支持才声明（与"声明与真实能力
        // 对齐"原则一致；历史事故风险见 BASE_EXTENSIONS 注释）。
        // 注意：glBufferStorage GL 函数本身始终由驱动 dispatch 提供，
        // 声明与否只影响 MC 是否走 BufferStorage 路径。
        (b"GL_ARB_buffer_storage\0", |c| c.buffer_storage),
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
            // MG getter.cpp:157-179 default 分支语义：enable 表优先
            // （enable 类 cap → 0/1；表持有的 int 状态 → 原值），再
            // pixel store 影子表，最后透传驱动。保证 glGetIntegerv 与
            // glIsEnabled / glGetBooleanv 对同一 cap 的回答永远一致。
            let mut handled = false;
            let mut ival = 0i32;
            let mut bval = 0u8;
            if enable_state::mg_enable_query(pname, &mut bval) {
                unsafe { *data = bval as i32 };
                handled = true;
            } else if enable_state::mg_enable_query_int(pname, &mut ival) {
                unsafe { *data = ival };
                handled = true;
            } else if pixel::pixel_store::query_int(pname, &mut ival) {
                unsafe { *data = ival };
                handled = true;
            }
            if !handled {
                getter::get_integerv(pname, data);
            }
            translate_binding_to_desktop(pname, data);
        }
    }
    log::debug!(
        "[FluorateGL] glGetIntegerv(0x{:04X}) -> {}",
        pname,
        unsafe { *data }
    );
}

/// 非 GL_EXTENSIONS 的 glGetStringi 拆分缓存（移植 MG getter.cpp:520-592
/// StringCache 语义）：GL_VENDOR / GL_VERSION / GL_SHADING_LANGUAGE_VERSION
/// 按分隔符拆分为 token 列表，索引越界返回空串而非 null。
///
/// MG 分隔符：GL_VENDOR 用 ", "（MG 的 vendor 含逗号）、GL_VERSION 用 " ."
/// （空格与点）、其余空格。我们版本字符串 "3.3.0 FluorateGL vX.Y.Z" 按 " ."
/// 拆分与 MG 行为一致。惰性构建一次并缓存；借用问题用扩展生命周期
/// （OnceLock 内容不被移动，&'static 安全）。
fn get_stringi_parts(name: u32) -> Option<&'static Vec<CString>> {
    static CACHES: OnceLock<Mutex<Vec<(u32, Vec<CString>)>>> = OnceLock::new();
    let mut guard = CACHES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if let Some((_, parts)) = guard.iter().find(|(n, _)| *n == name) {
        return Some(unsafe { &*(parts as *const Vec<CString>) });
    }
    // 构建缓存条目（持锁期间调用 glGetString 导出函数——其路径不触碰本锁，无死锁）
    let ptr = glGetString(name);
    if ptr.is_null() {
        return None;
    }
    let raw = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_bytes().to_vec();
    let delimiter: &[u8] = match name {
        0x1F00 /* GL_VENDOR */ => b", ",
        0x1F02 /* GL_VERSION */ => b" .",
        _ => b" ",
    };
    let parts: Vec<CString> = raw
        .split(|c| delimiter.contains(c))
        .filter(|t| !t.is_empty())
        .map(|t| CString::new(t).unwrap_or_else(|_| CString::new("").unwrap()))
        .collect();
    guard.push((name, parts));
    let (_, parts) = guard.last().unwrap();
    Some(unsafe { &*(parts as *const Vec<CString>) })
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
    } else if let Some(parts) = get_stringi_parts(name) {
        // 移植 MG StringCache：GL_VENDOR/GL_VERSION/GL_SHADING_LANGUAGE_VERSION
        // 也支持按索引查询（宿主可能对非扩展 name 使用 Stringi）
        if (index as usize) < parts.len() {
            parts[index as usize].as_ptr() as *const c_char
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

// ==== 虚拟 enable 状态表（移植自 MobileGlues gl/enable.cpp + enable.h）====
//
// MG 语义：桌面 GL 的 enable 能力全部由本表持有。glEnable/glDisable 写表，
// glIsEnabled / glGetBooleanv / glGetFloatv / glGetDoublev / glGetInteger64v /
// glGetIntegerv 读表，二者永远一致；是否转发到 GLES 驱动是每个 cap 的独立
// 属性（backing）：BK_NATIVE 恒转发、BK_EXT 扩展存在才转发、BK_VIRTUAL 仅
// 记录。GLES 不认识的 cap 不再产生 INVALID_ENUM 透传（旧实现 glIsEnabled
// 对这类 cap 返回 GL_FALSE 而 glGetBooleanv 不写 data，同一 cap 两种答案）。
//
// 我们相对 MG 的有意偏离（差分测试与历史行为保护）：
// 1. GL_DEBUG_OUTPUT：MG 为 BK_NATIVE，我们标 BK_VIRTUAL——吞掉 MC 的重新
//    启用，保持 Adreno 驱动 debug 噪声抑制设计（见 backend/mod.rs
//    suppress_debug_noise 与 glDebugMessageCallback 吞回调）。
// 2. GL_PRIMITIVE_RESTART：MG 为纯 BK_VIRTUAL（MG 的 drawing 层在 draw 前
//    自行借用驱动的 GL_PRIMITIVE_RESTART_FIXED_INDEX）；我们没有该借用机制
//    （drawing.rs 是纯透传，属域 4），故保留 translate_enable_cap 联动语义：
//    写表的同时转发 GL_PRIMITIVE_RESTART_FIXED_INDEX 到驱动，保证 MC 的
//    primitive restart 实际生效（不劣化渲染）。
// 3. GL_DEPTH_CLAMP：MG 只查 GL_EXT_depth_clamp；我们保留版本感知
//    （GLES 3.2 core 引入此 cap，3.2+ 直通，3.1 + 扩展缺失仅记录）。
// 4. GL_MAX_DRAW_BUFFERS 等表内 int 查询与 MG 一致（clamp 到表容量）。
pub(crate) mod enable_state {
    use crate::backend;
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};

    // ---- 枚举常量（desktop GL 枚举值，与 glcorearb.h 一致）----
    const GL_BLEND: u32 = 0x0BE2;
    const GL_COLOR_LOGIC_OP: u32 = 0x0BF2;
    const GL_CULL_FACE: u32 = 0x0B44;
    const GL_DEBUG_OUTPUT: u32 = 0x92E0;
    const GL_DEBUG_OUTPUT_SYNCHRONOUS: u32 = 0x8242;
    const GL_DEPTH_CLAMP: u32 = 0x864F;
    const GL_DEPTH_TEST: u32 = 0x0B71;
    const GL_DITHER: u32 = 0x0BD0;
    const GL_FRAMEBUFFER_SRGB: u32 = 0x8DB9;
    const GL_LINE_SMOOTH: u32 = 0x0B20;
    const GL_MULTISAMPLE: u32 = 0x809D;
    const GL_POLYGON_OFFSET_FILL: u32 = 0x8037;
    const GL_POLYGON_OFFSET_LINE: u32 = 0x2A02;
    const GL_POLYGON_OFFSET_POINT: u32 = 0x2A01;
    const GL_POLYGON_SMOOTH: u32 = 0x0B41;
    const GL_PRIMITIVE_RESTART: u32 = 0x8F9D;
    const GL_PRIMITIVE_RESTART_FIXED_INDEX: u32 = 0x8D69;
    const GL_PROGRAM_POINT_SIZE: u32 = 0x8642;
    const GL_RASTERIZER_DISCARD: u32 = 0x8C89;
    const GL_SAMPLE_ALPHA_TO_COVERAGE: u32 = 0x809E;
    const GL_SAMPLE_ALPHA_TO_ONE: u32 = 0x809F;
    const GL_SAMPLE_COVERAGE: u32 = 0x80A0;
    const GL_SAMPLE_MASK: u32 = 0x8E51;
    const GL_SAMPLE_SHADING: u32 = 0x8C36;
    const GL_SCISSOR_TEST: u32 = 0x0C11;
    const GL_STENCIL_TEST: u32 = 0x0B90;
    const GL_TEXTURE_CUBE_MAP_SEAMLESS: u32 = 0x884F;
    const GL_CLIP_DISTANCE0: u32 = 0x3000;
    const GL_PRIMITIVE_RESTART_INDEX: u32 = 0x8F3D;
    const GL_MAX_CLIP_DISTANCES: u32 = 0x0D32;
    const GL_MAX_VIEWPORTS: u32 = 0x825B;
    const GL_MAX_DRAW_BUFFERS: u32 = 0x8824;
    const GL_SAMPLES: u32 = 0x80A9;

    // ---- cap 索引（数组下标，顺序即 k_caps 顺序，对齐 MG mg_cap_index）----
    const MGC_BLEND: usize = 0;
    const MGC_COLOR_LOGIC_OP: usize = 1;
    const MGC_CULL_FACE: usize = 2;
    const MGC_DEBUG_OUTPUT: usize = 3;
    const MGC_DEBUG_OUTPUT_SYNCHRONOUS: usize = 4;
    const MGC_DEPTH_CLAMP: usize = 5;
    const MGC_DEPTH_TEST: usize = 6;
    const MGC_DITHER: usize = 7;
    const MGC_FRAMEBUFFER_SRGB: usize = 8;
    const MGC_LINE_SMOOTH: usize = 9;
    const MGC_MULTISAMPLE: usize = 10;
    const MGC_POLYGON_OFFSET_FILL: usize = 11;
    const MGC_POLYGON_OFFSET_LINE: usize = 12;
    const MGC_POLYGON_OFFSET_POINT: usize = 13;
    const MGC_POLYGON_SMOOTH: usize = 14;
    const MGC_PRIMITIVE_RESTART: usize = 15;
    const MGC_PRIMITIVE_RESTART_FIXED_INDEX: usize = 16;
    const MGC_PROGRAM_POINT_SIZE: usize = 17;
    const MGC_RASTERIZER_DISCARD: usize = 18;
    const MGC_SAMPLE_ALPHA_TO_COVERAGE: usize = 19;
    const MGC_SAMPLE_ALPHA_TO_ONE: usize = 20;
    const MGC_SAMPLE_COVERAGE: usize = 21;
    const MGC_SAMPLE_MASK: usize = 22;
    const MGC_SAMPLE_SHADING: usize = 23;
    const MGC_SCISSOR_TEST: usize = 24;
    const MGC_STENCIL_TEST: usize = 25;
    const MGC_TEXTURE_CUBE_MAP_SEAMLESS: usize = 26;
    const MGC_COUNT: usize = 27;

    // MG enable.h：GL 4.6 至少要求 8 个 clip distance；驱动的 draw buffers /
    // viewports 超过以下容量时 clamp（层从不承诺存不下的量）。
    const MG_MAX_CLIP_DISTANCES: u32 = 8;
    const MG_MAX_DRAW_BUFFERS: usize = 16;
    const MG_MAX_VIEWPORTS: usize = 16;

    /// cap 如何到达驱动（MG enable.cpp backing_t）。
    #[derive(Clone, Copy, PartialEq)]
    enum Backing {
        /// GLES core 认识此枚举：恒转发
        Native,
        /// 扩展用同一枚举值提供：扩展存在才转发，否则仅记录
        Ext,
        /// GLES 无对应：仅记录，永不转发
        Virtual,
    }

    struct CapDesc {
        cap: u32,
        index: usize,
        backing: Backing,
        initial: bool,
        name: &'static str,
    }

    // GL 4.6 初值来自 glEnable 规范；仅 GL_DITHER 默认 GL_TRUE
    // （GL_MULTISAMPLE 规范也为 TRUE，但由 framebuffer 决定，见
    // mg_enable_sync_driver 的播种逻辑）。
    const K_CAPS: &[CapDesc] = &[
        CapDesc {
            cap: GL_BLEND,
            index: MGC_BLEND,
            backing: Backing::Native,
            initial: false,
            name: "GL_BLEND",
        },
        CapDesc {
            cap: GL_COLOR_LOGIC_OP,
            index: MGC_COLOR_LOGIC_OP,
            backing: Backing::Virtual,
            initial: false,
            name: "GL_COLOR_LOGIC_OP",
        },
        CapDesc {
            cap: GL_CULL_FACE,
            index: MGC_CULL_FACE,
            backing: Backing::Native,
            initial: false,
            name: "GL_CULL_FACE",
        },
        // 偏离 MG（BK_NATIVE → BK_VIRTUAL）：吞掉 MC 的重新启用，保持
        // Adreno 驱动 debug 噪声抑制设计（见模块头注释 1）
        CapDesc {
            cap: GL_DEBUG_OUTPUT,
            index: MGC_DEBUG_OUTPUT,
            backing: Backing::Virtual,
            initial: false,
            name: "GL_DEBUG_OUTPUT",
        },
        CapDesc {
            cap: GL_DEBUG_OUTPUT_SYNCHRONOUS,
            index: MGC_DEBUG_OUTPUT_SYNCHRONOUS,
            backing: Backing::Native,
            initial: false,
            name: "GL_DEBUG_OUTPUT_SYNCHRONOUS",
        },
        CapDesc {
            cap: GL_DEPTH_CLAMP,
            index: MGC_DEPTH_CLAMP,
            backing: Backing::Ext,
            initial: false,
            name: "GL_DEPTH_CLAMP",
        },
        CapDesc {
            cap: GL_DEPTH_TEST,
            index: MGC_DEPTH_TEST,
            backing: Backing::Native,
            initial: false,
            name: "GL_DEPTH_TEST",
        },
        CapDesc {
            cap: GL_DITHER,
            index: MGC_DITHER,
            backing: Backing::Native,
            initial: true,
            name: "GL_DITHER",
        },
        CapDesc {
            cap: GL_FRAMEBUFFER_SRGB,
            index: MGC_FRAMEBUFFER_SRGB,
            backing: Backing::Ext,
            initial: false,
            name: "GL_FRAMEBUFFER_SRGB",
        },
        CapDesc {
            cap: GL_LINE_SMOOTH,
            index: MGC_LINE_SMOOTH,
            backing: Backing::Virtual,
            initial: false,
            name: "GL_LINE_SMOOTH",
        },
        CapDesc {
            cap: GL_MULTISAMPLE,
            index: MGC_MULTISAMPLE,
            backing: Backing::Ext,
            initial: false,
            name: "GL_MULTISAMPLE",
        },
        CapDesc {
            cap: GL_POLYGON_OFFSET_FILL,
            index: MGC_POLYGON_OFFSET_FILL,
            backing: Backing::Native,
            initial: false,
            name: "GL_POLYGON_OFFSET_FILL",
        },
        CapDesc {
            cap: GL_POLYGON_OFFSET_LINE,
            index: MGC_POLYGON_OFFSET_LINE,
            backing: Backing::Ext,
            initial: false,
            name: "GL_POLYGON_OFFSET_LINE",
        },
        CapDesc {
            cap: GL_POLYGON_OFFSET_POINT,
            index: MGC_POLYGON_OFFSET_POINT,
            backing: Backing::Ext,
            initial: false,
            name: "GL_POLYGON_OFFSET_POINT",
        },
        CapDesc {
            cap: GL_POLYGON_SMOOTH,
            index: MGC_POLYGON_SMOOTH,
            backing: Backing::Virtual,
            initial: false,
            name: "GL_POLYGON_SMOOTH",
        },
        CapDesc {
            cap: GL_PRIMITIVE_RESTART,
            index: MGC_PRIMITIVE_RESTART,
            backing: Backing::Virtual,
            initial: false,
            name: "GL_PRIMITIVE_RESTART",
        },
        CapDesc {
            cap: GL_PRIMITIVE_RESTART_FIXED_INDEX,
            index: MGC_PRIMITIVE_RESTART_FIXED_INDEX,
            backing: Backing::Native,
            initial: false,
            name: "GL_PRIMITIVE_RESTART_FIXED_INDEX",
        },
        CapDesc {
            cap: GL_PROGRAM_POINT_SIZE,
            index: MGC_PROGRAM_POINT_SIZE,
            backing: Backing::Virtual,
            initial: false,
            name: "GL_PROGRAM_POINT_SIZE",
        },
        CapDesc {
            cap: GL_RASTERIZER_DISCARD,
            index: MGC_RASTERIZER_DISCARD,
            backing: Backing::Native,
            initial: false,
            name: "GL_RASTERIZER_DISCARD",
        },
        CapDesc {
            cap: GL_SAMPLE_ALPHA_TO_COVERAGE,
            index: MGC_SAMPLE_ALPHA_TO_COVERAGE,
            backing: Backing::Native,
            initial: false,
            name: "GL_SAMPLE_ALPHA_TO_COVERAGE",
        },
        CapDesc {
            cap: GL_SAMPLE_ALPHA_TO_ONE,
            index: MGC_SAMPLE_ALPHA_TO_ONE,
            backing: Backing::Ext,
            initial: false,
            name: "GL_SAMPLE_ALPHA_TO_ONE",
        },
        CapDesc {
            cap: GL_SAMPLE_COVERAGE,
            index: MGC_SAMPLE_COVERAGE,
            backing: Backing::Native,
            initial: false,
            name: "GL_SAMPLE_COVERAGE",
        },
        CapDesc {
            cap: GL_SAMPLE_MASK,
            index: MGC_SAMPLE_MASK,
            backing: Backing::Native,
            initial: false,
            name: "GL_SAMPLE_MASK",
        },
        CapDesc {
            cap: GL_SAMPLE_SHADING,
            index: MGC_SAMPLE_SHADING,
            backing: Backing::Ext,
            initial: false,
            name: "GL_SAMPLE_SHADING",
        },
        CapDesc {
            cap: GL_SCISSOR_TEST,
            index: MGC_SCISSOR_TEST,
            backing: Backing::Native,
            initial: false,
            name: "GL_SCISSOR_TEST",
        },
        CapDesc {
            cap: GL_STENCIL_TEST,
            index: MGC_STENCIL_TEST,
            backing: Backing::Native,
            initial: false,
            name: "GL_STENCIL_TEST",
        },
        CapDesc {
            cap: GL_TEXTURE_CUBE_MAP_SEAMLESS,
            index: MGC_TEXTURE_CUBE_MAP_SEAMLESS,
            backing: Backing::Virtual,
            initial: false,
            name: "GL_TEXTURE_CUBE_MAP_SEAMLESS",
        },
    ];

    // 编译期断言：K_CAPS 条目数 = MGC_COUNT 且按索引顺序排列
    // （对齐 MG 的 static_assert：k_caps[MGC_X] 必须是能力 X 的描述符）。
    const fn caps_ordered() -> bool {
        let mut i = 0;
        while i < K_CAPS.len() {
            if K_CAPS[i].index != i {
                return false;
            }
            i += 1;
        }
        K_CAPS.len() == MGC_COUNT
    }
    const _: () = assert!(
        caps_ordered(),
        "K_CAPS 必须按 mg_cap_index 顺序排列且覆盖全部"
    );

    /// 一次性的"每站告警"宏（对齐 MG EN_WARN_ONCE：参数被拒时仅提示一次）。
    macro_rules! warn_once {
        ($w:ident, $($arg:tt)*) => {{
            static $w: AtomicBool = AtomicBool::new(false);
            if !$w.swap(true, Ordering::Relaxed) {
                log::warn!($($arg)*);
            }
        }};
    }

    /// find_cap：switch 而非遍历 K_CAPS（每个 glEnable/glDisable 与查询都经过）。
    fn find_cap(cap: u32) -> Option<&'static CapDesc> {
        match cap {
            GL_BLEND => Some(&K_CAPS[MGC_BLEND]),
            GL_COLOR_LOGIC_OP => Some(&K_CAPS[MGC_COLOR_LOGIC_OP]),
            GL_CULL_FACE => Some(&K_CAPS[MGC_CULL_FACE]),
            GL_DEBUG_OUTPUT => Some(&K_CAPS[MGC_DEBUG_OUTPUT]),
            GL_DEBUG_OUTPUT_SYNCHRONOUS => Some(&K_CAPS[MGC_DEBUG_OUTPUT_SYNCHRONOUS]),
            GL_DEPTH_CLAMP => Some(&K_CAPS[MGC_DEPTH_CLAMP]),
            GL_DEPTH_TEST => Some(&K_CAPS[MGC_DEPTH_TEST]),
            GL_DITHER => Some(&K_CAPS[MGC_DITHER]),
            GL_FRAMEBUFFER_SRGB => Some(&K_CAPS[MGC_FRAMEBUFFER_SRGB]),
            GL_LINE_SMOOTH => Some(&K_CAPS[MGC_LINE_SMOOTH]),
            GL_MULTISAMPLE => Some(&K_CAPS[MGC_MULTISAMPLE]),
            GL_POLYGON_OFFSET_FILL => Some(&K_CAPS[MGC_POLYGON_OFFSET_FILL]),
            GL_POLYGON_OFFSET_LINE => Some(&K_CAPS[MGC_POLYGON_OFFSET_LINE]),
            GL_POLYGON_OFFSET_POINT => Some(&K_CAPS[MGC_POLYGON_OFFSET_POINT]),
            GL_POLYGON_SMOOTH => Some(&K_CAPS[MGC_POLYGON_SMOOTH]),
            GL_PRIMITIVE_RESTART => Some(&K_CAPS[MGC_PRIMITIVE_RESTART]),
            GL_PRIMITIVE_RESTART_FIXED_INDEX => Some(&K_CAPS[MGC_PRIMITIVE_RESTART_FIXED_INDEX]),
            GL_PROGRAM_POINT_SIZE => Some(&K_CAPS[MGC_PROGRAM_POINT_SIZE]),
            GL_RASTERIZER_DISCARD => Some(&K_CAPS[MGC_RASTERIZER_DISCARD]),
            GL_SAMPLE_ALPHA_TO_COVERAGE => Some(&K_CAPS[MGC_SAMPLE_ALPHA_TO_COVERAGE]),
            GL_SAMPLE_ALPHA_TO_ONE => Some(&K_CAPS[MGC_SAMPLE_ALPHA_TO_ONE]),
            GL_SAMPLE_COVERAGE => Some(&K_CAPS[MGC_SAMPLE_COVERAGE]),
            GL_SAMPLE_MASK => Some(&K_CAPS[MGC_SAMPLE_MASK]),
            GL_SAMPLE_SHADING => Some(&K_CAPS[MGC_SAMPLE_SHADING]),
            GL_SCISSOR_TEST => Some(&K_CAPS[MGC_SCISSOR_TEST]),
            GL_STENCIL_TEST => Some(&K_CAPS[MGC_STENCIL_TEST]),
            GL_TEXTURE_CUBE_MAP_SEAMLESS => Some(&K_CAPS[MGC_TEXTURE_CUBE_MAP_SEAMLESS]),
            _ => None,
        }
    }

    /// GL_CLIP_DISTANCEi 是连续枚举区段，用位掩码而非表槽（MG 同）。
    fn clip_distance_slot(cap: u32) -> Option<u32> {
        if cap < GL_CLIP_DISTANCE0 {
            return None;
        }
        let n = cap - GL_CLIP_DISTANCE0;
        if n >= MG_MAX_CLIP_DISTANCES {
            return None;
        }
        Some(n)
    }

    /// 支撑 BK_EXT cap 的扩展是否真实存在（使用时刻查询而非缓存，
    /// 对齐 MG：设备有扩展就必须持续拿到真实转发）。
    ///
    /// 注意：扩展存在性来自 backend::capabilities 的扩展缓存，首次 GL
    /// 调用（with_gles_dispatch 触发能力查询）后定型；此前返回 false
    /// （按"无扩展"只记录不转发，与旧实现版本兜底行为一致）。
    fn ext_backing_present(cap: u32) -> bool {
        match cap {
            GL_DEPTH_CLAMP => {
                // 偏离 MG（仅查 GL_EXT_depth_clamp）：保留版本感知，
                // GLES 3.2 core 引入此 cap（见模块头注释 3）
                crate::backend::capabilities().version.at_least(3, 2)
                    || backend::capabilities::has_extension("GL_EXT_depth_clamp")
            }
            GL_FRAMEBUFFER_SRGB => {
                backend::capabilities::has_extension("GL_EXT_sRGB_write_control")
            }
            GL_MULTISAMPLE | GL_SAMPLE_ALPHA_TO_ONE => {
                backend::capabilities::has_extension("GL_EXT_multisample_compatibility")
            }
            GL_POLYGON_OFFSET_LINE | GL_POLYGON_OFFSET_POINT => {
                backend::capabilities::has_extension("GL_NV_polygon_mode")
            }
            GL_SAMPLE_SHADING => {
                // GLES 3.2 core，此前为扩展
                crate::backend::capabilities().version.at_least(3, 2)
                    || backend::capabilities::has_extension("GL_OES_sample_shading")
            }
            _ => false,
        }
    }

    /// 每上下文 enable 状态（本层无上下文 id 概念，thread_local 与
    /// state::State 一致——GL 上下文是线程绑定的）。
    #[derive(Clone)]
    struct EnableState {
        scalar: [bool; MGC_COUNT],
        blend_indexed: [bool; MG_MAX_DRAW_BUFFERS],
        scissor_indexed: [bool; MG_MAX_VIEWPORTS],
        clip_distance_mask: u32,
        primitive_restart_index: u32,
        initialised: bool,
        /// mg_enable_sync_driver 已运行（驱动与表对齐，之后可安全跳过冗余调用）
        driver_synced: bool,
    }

    impl Default for EnableState {
        fn default() -> Self {
            Self {
                scalar: [false; MGC_COUNT],
                blend_indexed: [false; MG_MAX_DRAW_BUFFERS],
                scissor_indexed: [false; MG_MAX_VIEWPORTS],
                clip_distance_mask: 0,
                primitive_restart_index: 0,
                initialised: false,
                driver_synced: false,
            }
        }
    }

    thread_local! {
        static ENABLE_STATE: RefCell<EnableState> = RefCell::new(EnableState::default());
    }

    fn mg_enable_reset(st: &mut EnableState) {
        st.scalar = [false; MGC_COUNT];
        for d in K_CAPS {
            st.scalar[d.index] = d.initial;
        }
        // GL 4.6 规范 GL_MULTISAMPLE 初值 GL_TRUE；扩展缺失时由
        // framebuffer 实际采样数播种（见 mg_enable_sync_driver）
        st.scalar[MGC_MULTISAMPLE] = true;
        // GL_BLEND 按 draw buffer、GL_SCISSOR_TEST 按 viewport；scalar 形式
        // 各为索引 0（MG 同）
        st.blend_indexed = [false; MG_MAX_DRAW_BUFFERS];
        st.scissor_indexed = [false; MG_MAX_VIEWPORTS];
        st.clip_distance_mask = 0;
        st.primitive_restart_index = 0;
        st.initialised = true;
        st.driver_synced = false;
    }

    /// 让驱动与表对齐（对齐 MG mg_enable_sync_driver，每次上下文生效一次）。
    ///
    /// - 无 GL_EXT_multisample_compatibility 时 GL_MULTISAMPLE 由
    ///   GL_SAMPLES 实际值播种（单采样 framebuffer 上报 TRUE 是另一方向的谎言）。
    /// - GL_FRAMEBUFFER_SRGB：桌面初值 GL_FALSE 而 GL_EXT_sRGB_write_control
    ///   初值 GL_TRUE，把表值推送到驱动使两者从第一帧起一致。
    ///
    /// 无 GLES dispatch（stub 模式）时不执行且不置 driver_synced，
    /// 后续调用继续尝试（对齐 MG：sync 在上下文 current 时运行）。
    fn mg_enable_sync_driver(st: &mut EnableState) {
        if st.driver_synced {
            return;
        }
        if !backend::gles_dispatch_ready() {
            return;
        }
        backend::with_gles_dispatch(|dispatch| unsafe {
            if !ext_backing_present(GL_MULTISAMPLE) {
                let mut samples: i32 = 0;
                (dispatch.get_integerv)(GL_SAMPLES, &mut samples);
                st.scalar[MGC_MULTISAMPLE] = samples > 0;
            }
            if ext_backing_present(GL_FRAMEBUFFER_SRGB) {
                if st.scalar[MGC_FRAMEBUFFER_SRGB] {
                    (dispatch.enable)(GL_FRAMEBUFFER_SRGB);
                } else {
                    (dispatch.disable)(GL_FRAMEBUFFER_SRGB);
                }
            }
        });
        st.driver_synced = true;
    }

    /// 当前线程 enable 表（惰性初始化）。
    fn with_enable_state_mut<F: FnOnce(&mut EnableState)>(f: F) {
        ENABLE_STATE.with(|cell| {
            let mut st = cell.borrow_mut();
            if !st.initialised {
                mg_enable_reset(&mut st);
            }
            f(&mut st);
        });
    }

    /// 读 cap 状态（index 对非 indexed cap 忽略；未知 cap 返回 GL_FALSE
    /// ——GL 规范对非法 cap 的答案，对齐 MG mg_enable_get）。
    pub(crate) fn mg_enable_get(cap: u32, index: u32) -> u8 {
        ENABLE_STATE.with(|cell| {
            let st = cell.borrow();
            if !st.initialised {
                return 0;
            }
            if let Some(slot) = clip_distance_slot(cap) {
                return if st.clip_distance_mask & (1 << slot) != 0 {
                    1
                } else {
                    0
                };
            }
            let Some(d) = find_cap(cap) else {
                return 0;
            };
            if d.cap == GL_BLEND && (index as usize) < MG_MAX_DRAW_BUFFERS {
                return st.blend_indexed[index as usize] as u8;
            }
            if d.cap == GL_SCISSOR_TEST && (index as usize) < MG_MAX_VIEWPORTS {
                return st.scissor_indexed[index as usize] as u8;
            }
            st.scalar[d.index] as u8
        })
    }

    /// pname 是否为 enable 类查询（是则 *out 收到状态并返回 true）。
    /// 供 glGetBooleanv / glGetFloatv / glGetDoublev / glGetInteger64v /
    /// glGetIntegerv 使用，保证与 glIsEnabled 的回答一致（对齐 MG
    /// mg_enable_query）。
    pub(crate) fn mg_enable_query(pname: u32, out: &mut u8) -> bool {
        if clip_distance_slot(pname).is_some() || find_cap(pname).is_some() {
            *out = mg_enable_get(pname, 0);
            return true;
        }
        false
    }

    /// pname 是否为本表持有的 int 状态（是则 *out 收到值并返回 true）。
    ///
    /// GL_PRIMITIVE_RESTART_INDEX：表记录值（glPrimitiveRestartIndex 写入）；
    /// GL_MAX_CLIP_DISTANCES / GL_MAX_VIEWPORTS：表实际可跟踪的量
    /// （GLES 无此 pname，直通 INVALID_ENUM 且不写 data——旧实现宿主读垃圾值）；
    /// GL_MAX_DRAW_BUFFERS：驱动值 clamp 到 blend_indexed 容量
    /// （glEnablei(GL_BLEND, i) 永远不会收到表承诺之外的索引）。
    pub(crate) fn mg_enable_query_int(pname: u32, out: &mut i32) -> bool {
        match pname {
            GL_PRIMITIVE_RESTART_INDEX => {
                ENABLE_STATE.with(|cell| *out = cell.borrow().primitive_restart_index as i32);
                true
            }
            GL_MAX_CLIP_DISTANCES => {
                *out = MG_MAX_CLIP_DISTANCES as i32;
                true
            }
            GL_MAX_VIEWPORTS => {
                *out = MG_MAX_VIEWPORTS as i32;
                true
            }
            GL_MAX_DRAW_BUFFERS => {
                let mut n: i32 = 0;
                backend::with_gles_dispatch(|dispatch| unsafe {
                    (dispatch.get_integerv)(GL_MAX_DRAW_BUFFERS, &mut n);
                });
                if n <= 0 {
                    n = 1;
                }
                *out = if n as usize > MG_MAX_DRAW_BUFFERS {
                    MG_MAX_DRAW_BUFFERS as i32
                } else {
                    n
                };
                true
            }
            _ => false,
        }
    }

    /// 写 glPrimitiveRestartIndex（MG mg_enable_set_primitive_restart_index）。
    ///
    /// 注意跨域协调：drawing.rs（域 4）的 glPrimitiveRestartIndex 目前直接
    /// 转发驱动、不写本表——若宿主调用后查询 GL_PRIMITIVE_RESTART_INDEX，
    /// 表回答 0（默认值）而非驱动值。域 4 若改为写表（对齐 MG）即可闭环。
    pub(crate) fn set_primitive_restart_index(index: u32) {
        with_enable_state_mut(|st| st.primitive_restart_index = index);
    }

    pub(crate) fn gl_enable(cap: u32) {
        mg_set_enabled(cap, 0, false, true);
    }

    pub(crate) fn gl_disable(cap: u32) {
        mg_set_enabled(cap, 0, false, false);
    }

    pub(crate) fn gl_enable_i(cap: u32, index: u32) {
        mg_set_enabled(cap, index, true, true);
    }

    pub(crate) fn gl_disable_i(cap: u32, index: u32) {
        mg_set_enabled(cap, index, true, false);
    }

    /// 写 enable 状态（对齐 MG mg_set_enabled：表永远更新，驱动按 backing
    /// 属性 + 冗余检测决定是否真正调用）。
    fn mg_set_enabled(cap: u32, index: u32, indexed: bool, value: bool) {
        with_enable_state_mut(|st| {
            mg_enable_sync_driver(st);

            // GL_CLIP_DISTANCEi：位掩码路径
            if let Some(slot) = clip_distance_slot(cap) {
                if indexed {
                    warn_once!(
                        CLIP_INDEXED_WARNED,
                        "[FluorateGL] glEnablei/glDisablei: GL_CLIP_DISTANCE{} 不是 indexed capability，已忽略",
                        slot
                    );
                    return;
                }
                let was_on = (st.clip_distance_mask & (1 << slot)) != 0;
                if value {
                    st.clip_distance_mask |= 1 << slot;
                } else {
                    st.clip_distance_mask &= !(1 << slot);
                }
                let redundant = was_on == value && st.driver_synced;
                if !redundant && backend::capabilities::has_extension("GL_EXT_clip_cull_distance") {
                    backend::with_gles_dispatch(|dispatch| unsafe {
                        if value {
                            (dispatch.enable)(cap);
                        } else {
                            (dispatch.disable)(cap);
                        }
                    });
                }
                return;
            }

            let Some(d) = find_cap(cap) else {
                // 表外 cap：透传 GLES——桌面 3.3 对非法 cap 报 INVALID_ENUM，
                // GLES 同样报（MG 的忽略行为与桌面语义不符，差分 g11 裁决修正）
                warn_once!(
                    UNKNOWN_CAP_WARNED,
                    "[FluorateGL] glEnable/glDisable: 0x{:04X} 不在能力表，透传 GLES（非法 cap 由驱动报 INVALID_ENUM）",
                    cap
                );
                backend::with_gles_dispatch(|dispatch| unsafe {
                    if value {
                        (dispatch.enable)(cap);
                    } else {
                        (dispatch.disable)(cap);
                    }
                });
                return;
            };

            // indexed 路径：GL 4.6 只有两个 indexed cap（BLEND 按 draw buffer、
            // SCISSOR_TEST 按 viewport；viewport 数组未实现，仅 index 0 有效）
            if indexed {
                match d.cap {
                    GL_BLEND => {
                        if index as usize >= MG_MAX_DRAW_BUFFERS {
                            warn_once!(
                                BLEND_INDEX_WARNED,
                                "[FluorateGL] glEnablei(GL_BLEND, {}): 索引超过 GL_MAX_DRAW_BUFFERS，已忽略",
                                index
                            );
                            return;
                        }
                        let was = st.blend_indexed[index as usize];
                        st.blend_indexed[index as usize] = value;
                        if index == 0 {
                            st.scalar[MGC_BLEND] = value;
                        }
                        if was == value && st.driver_synced {
                            return;
                        }
                        backend::with_gles_dispatch(|dispatch| unsafe {
                            if (dispatch.enable_i as *const ()) != (dispatch.stub as *const ()) {
                                if value {
                                    (dispatch.enable_i)(cap, index);
                                } else {
                                    (dispatch.disable_i)(cap, index);
                                }
                            } else if index == 0 {
                                if value {
                                    (dispatch.enable)(cap);
                                } else {
                                    (dispatch.disable)(cap);
                                }
                            }
                        });
                        return;
                    }
                    GL_SCISSOR_TEST => {
                        if index as usize >= MG_MAX_VIEWPORTS {
                            warn_once!(
                                SCISSOR_INDEX_WARNED,
                                "[FluorateGL] glEnablei(GL_SCISSOR_TEST, {}): 索引超过 GL_MAX_VIEWPORTS，已忽略",
                                index
                            );
                            return;
                        }
                        st.scissor_indexed[index as usize] = value;
                        if index != 0 {
                            // 仅记录（viewport 数组入口是 stub，对齐 MG）
                            warn_once!(
                                SCISSOR_VIEWPORT_WARNED,
                                "[FluorateGL] glEnablei(GL_SCISSOR_TEST, {}): viewport 数组未实现，状态已记录但无实际效果",
                                index
                            );
                            return;
                        }
                        // index == 0：继续走 scalar 路径（同步 scissor_indexed[0]）
                    }
                    _ => {
                        warn_once!(
                            NOT_INDEXED_WARNED,
                            "[FluorateGL] glEnablei/glDisablei: {} 不是 indexed capability，已忽略",
                            d.name
                        );
                        return;
                    }
                }
            }

            // scalar 主路径：先读旧值（BLEND 需所有 draw buffer 槽一致才算冗余）
            let mut already_set = st.scalar[d.index] == value;
            if already_set && d.cap == GL_BLEND {
                already_set = st.blend_indexed.iter().all(|&b| b == value);
            }

            st.scalar[d.index] = value;
            if d.cap == GL_BLEND {
                for b in st.blend_indexed.iter_mut() {
                    *b = value;
                }
            }
            if d.cap == GL_SCISSOR_TEST {
                for s in st.scissor_indexed.iter_mut() {
                    *s = value;
                }
            }

            // 转发判定：backing 属性 + 扩展存在性
            let mut forward = match d.backing {
                Backing::Native => true,
                Backing::Ext => ext_backing_present(d.cap),
                Backing::Virtual => false,
            };
            // 偏离 MG（模块头注释 2）：GL_PRIMITIVE_RESTART 保留联动语义——
            // 转发 GL_PRIMITIVE_RESTART_FIXED_INDEX 到驱动（我们 drawing 层
            // 无 MG 的临时借用机制，不转发则 MC 的 primitive restart 失效）
            if d.cap == GL_PRIMITIVE_RESTART {
                forward = true;
            }

            if !forward {
                log::debug!(
                    "[FluorateGL] {} 仅记录不转发（GLES 无对应或扩展缺失）",
                    d.name
                );
                return;
            }

            // 冗余检测：表与驱动已同步且值未变 → 跳过驱动调用
            // （渲染器每 pass 都 glEnable 已生效的状态，驱动调用不免费——
            // 对齐 MG，ANGLE 下尤其明显）
            if already_set && st.driver_synced {
                return;
            }

            backend::with_gles_dispatch(|dispatch| unsafe {
                let target = if d.cap == GL_PRIMITIVE_RESTART {
                    GL_PRIMITIVE_RESTART_FIXED_INDEX
                } else {
                    d.cap
                };
                if value {
                    (dispatch.enable)(target);
                } else {
                    (dispatch.disable)(target);
                }
            });
        });
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// 表顺序与索引断言（编译期已校验，运行期回归保护）。
        #[test]
        fn caps_table_ordered() {
            assert!(caps_ordered());
            assert_eq!(K_CAPS.len(), MGC_COUNT);
        }

        /// 未知 cap：glIsEnabled 语义返回 GL_FALSE，写操作仅告警忽略。
        #[test]
        fn unknown_cap_returns_false() {
            assert_eq!(mg_enable_get(0xDEAD, 0), 0);
        }

        /// 写读回：VIRTUAL cap 记录在表，驱动不参与。
        #[test]
        fn virtual_cap_roundtrip() {
            with_enable_state_mut(|st| {
                mg_enable_reset(st);
                st.scalar[MGC_TEXTURE_CUBE_MAP_SEAMLESS] = true;
            });
            assert_eq!(mg_enable_get(GL_TEXTURE_CUBE_MAP_SEAMLESS, 0), 1);
            let mut out = 0u8;
            assert!(mg_enable_query(GL_TEXTURE_CUBE_MAP_SEAMLESS, &mut out));
            assert_eq!(out, 1);
        }

        /// GL_PRIMITIVE_RESTART_INDEX int 查询走表。
        #[test]
        fn primitive_restart_index_query() {
            with_enable_state_mut(|st| {
                mg_enable_reset(st);
                st.primitive_restart_index = 0xFFFF;
            });
            let mut out = 0i32;
            assert!(mg_enable_query_int(GL_PRIMITIVE_RESTART_INDEX, &mut out));
            assert_eq!(out, 0xFFFF);
        }

        /// BLEND indexed 与 scalar 镜像。
        #[test]
        fn blend_indexed_mirrors_scalar() {
            with_enable_state_mut(|st| {
                mg_enable_reset(st);
                st.scalar[MGC_BLEND] = true;
                st.blend_indexed = [true; MG_MAX_DRAW_BUFFERS];
            });
            assert_eq!(mg_enable_get(GL_BLEND, 3), 1);
            assert_eq!(mg_enable_get(GL_BLEND, 0), 1);
        }
    }
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

    /// C5/D5：注入错误应在下一次 glGetError 返回（FIFO 优先于驱动错误队列）。
    #[test]
    fn injected_gl_error_returned_by_gl_get_error() {
        use super::{INJECTED_GL_ERRORS, glGetError, inject_gl_error};
        // 清理可能残留的注入（其他测试/运行时不会注入，防御性清空）
        INJECTED_GL_ERRORS.lock().unwrap().clear();

        inject_gl_error(0x0501); // GL_INVALID_VALUE
        inject_gl_error(0x0500); // GL_INVALID_ENUM
        assert_eq!(glGetError(), 0x0501, "FIFO：先注入先返回");
        assert_eq!(glGetError(), 0x0500, "FIFO：第二个注入错误");
        // 队列清空后转发驱动（测试环境无 GLES dispatch → stub 返回 0）
        assert_eq!(glGetError(), 0, "注入队列空时转发底层 glGetError");
        assert!(
            INJECTED_GL_ERRORS.lock().unwrap().is_empty(),
            "测试后注入队列应为空"
        );
    }
}
