use crate::backend;
use crate::state;
use libc::c_char;
use regex::Regex;
use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};

/// glCreateProgram 返回 0（无 EGL 上下文）首次告警标志
/// 触发场景：异步加载线程在 EGL 上下文创建前调用 GL
static PROGRAM_NO_CONTEXT_WARNED: AtomicBool = AtomicBool::new(false);
/// glGetProgramiv program 不在 IdMap 中首次告警标志
/// 触发场景：跨线程查询或 program 已被释放
static PROGRAM_ID_MISS_WARNED: AtomicBool = AtomicBool::new(false);

/// 首次告警：glCreateProgram GLES 返回 0（无 EGL 上下文）。
fn warn_program_no_context() {
    if !PROGRAM_NO_CONTEXT_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glCreateProgram() -> GLES returned 0 (no context on tid={}, 后续调用将静默返回 0)",
            state::thread_id_u64()
        );
    }
}

/// 首次告警：glGetProgramiv program 不在 IdMap 中。
fn warn_program_id_miss(program: u32) {
    if !PROGRAM_ID_MISS_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] glGetProgramiv: program {} not found in IdMap (tid={}, params untouched, caller sees GL_FALSE, 后续将静默降级)",
            program,
            state::thread_id_u64()
        );
    }
}

// ==== 常量（对齐 MG program.cpp / shader.h）====

const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_LINK_STATUS: u32 = 0x8B82;
const GL_INFO_LOG_LENGTH: u32 = 0x8B84;
const GL_COMPILE_STATUS: u32 = 0x8B81;

/// program 默认 FS 生成状态（对齐 MG ShouldGenerateFSState）
const FS_STATE_UNKNOWN: u8 = 0;
const FS_STATE_NEVER: u8 = 1;
const FS_STATE_MAYBE: u8 = 2;

/// 解析 outColorN 后缀（MG program.cpp glBindFragDataLocation 的跳过检测）。
///
/// 返回 Some(n) 当 name 形如 "outColor" + 纯数字后缀（如 outColor1 → 1）。
/// "outColor" 本身（无数字）返回 None（与 MG `strlen(name) > 8` 前置一致）。
fn parse_out_color_suffix(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("outColor")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// 对齐 MG updateLayoutLocation 的有效语义：对匹配的 `out <type> <name>;`
/// 声明注入 `layout (location = N) out <type> <name>;`。
///
/// 与 MG 的差异（有意为之）：
/// - MG 的前缀组 `(layout\s*$[^)]*location...)?` 因 `$` 锚点（行尾后跟字符）
///   实际无法匹配已有 layout 的行，导致"已带 layout 的声明被再次注入"——
///   产生双重 layout，GLES 编译报错。我们修正该 regex：可选前缀组
///   `(layout\s*\([^)]*\)\s*)?` 正确吞掉已有 layout 再注入单一 layout，
///   双重 layout 场景下行为正确（MG 靠跳过 outColorN 回避此问题，我们
///   靠正确的前缀组，两条路径都安全）。
/// - 不跳过 outColorN 匹配场景：MG 跳过依赖其转换管线保证 outColorN 已带
///   location；我们 SPIR-V 主路径（postprocess fix_out_color_locations）同
///   保证（剥离旧 layout 再注入相同值 → 无操作），但 string_pass 兜底不保证
///   → 统一走注入自动兼容两条路径（fail-open：注入失败保持原样）。
/// - 与 MG 一致无 `^` 锚点：`flat out T name;` 这类行也能匹配（type 组不含
///   out 前的修饰符，替换后修饰符丢失——对 fragment output 无实际影响，
///   与 MG 行为一致）。
///
/// 返回注入后的源码；若未找到匹配声明则原样返回。
fn inject_frag_data_layout(src: &str, color: u32, name: &str) -> String {
    let escaped = regex::escape(name);
    // 可选前缀组吞掉已有 layout(...)（修正 MG 的前缀组 bug：MG 的 `$` 锚点
    // 使前缀组永远匹配失败，已带 layout 的行会被二次注入造成双重 layout）
    let pattern = format!(
        r"(?m)(layout\s*\([^)]*\)\s*)?out\s+(?P<type>(?:(?:flat|smooth|noperspective|centroid|invariant|highp|mediump|lowp)\s+)*\w+)\s+{}\s*;",
        escaped
    );
    let re = match Regex::new(&pattern) {
        Ok(re) => re,
        Err(e) => {
            log::error!(
                "[FluorateGL] inject_frag_data_layout: invalid regex for name {:?}: {}",
                name,
                e
            );
            return src.to_string();
        }
    };
    re.replace_all(src, |caps: &regex::Captures| {
        let typ = caps.name("type").map(|m| m.as_str()).unwrap_or("");
        // 与 MG 替换串一致："layout (location = N) out $2 NAME;"
        format!("layout (location = {}) out {} {};", color, typ, name)
    })
    .to_string()
}

/// 生成默认 fragment shader 并返回 GLES shader id（0=失败），按 ES 版本缓存。
///
/// 对齐 MG GenerateDefaultFSSource + DefaultFSMap：无 FS 的 program 在 GLES 上
/// link 必败，用输出纯白的默认 FS 兜底。fail-open：编译失败删除并返回 0，
/// 调用方不 attach，行为回退到"无默认 FS"的原状。
fn generate_default_fs(dispatch: &crate::backend::dispatch::GlesDispatch) -> u32 {
    // GlesVersion = major*10+minor（如 3.2 → 32），#version 需 320 形式
    let es_version = backend::capabilities().version.0 as u32;
    if es_version == 0 {
        return 0;
    }
    let version_str = es_version * 10;

    if let Some(id) = state::with_state_ref(|s| s.default_fs_cache.get(&version_str).copied()) {
        return id;
    }

    let src = format!(
        "#version {} es\n\
         precision mediump float;\n\n\
         out vec4 fragColor;\n\n\
         void main() {{\n\
         \x20   fragColor = vec4(1.0, 1.0, 1.0, 1.0);\n\
         }}\n",
        version_str
    );
    let c = match CString::new(src) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let gles_fs = unsafe { (dispatch.create_shader)(GL_FRAGMENT_SHADER) };
    if gles_fs == 0 {
        return 0;
    }
    let ptr = c.as_ptr();
    let len = c.as_bytes().len() as i32;
    unsafe {
        (dispatch.shader_source)(gles_fs, 1, &ptr, &len);
        (dispatch.compile_shader)(gles_fs);
    }
    let mut status = 0i32;
    unsafe {
        (dispatch.get_shader_iv)(gles_fs, GL_COMPILE_STATUS, &mut status);
    }
    if status == 0 {
        let mut len2 = 0i32;
        unsafe {
            (dispatch.get_shader_iv)(gles_fs, GL_INFO_LOG_LENGTH, &mut len2);
        }
        if len2 > 0 {
            let mut buf = vec![0u8; len2 as usize];
            let mut written = 0i32;
            unsafe {
                (dispatch.get_shader_info_log)(
                    gles_fs,
                    len2,
                    &mut written,
                    buf.as_mut_ptr() as *mut libc::c_char,
                );
            }
            let safe_written = (written.max(0) as usize).min(buf.len());
            log::error!(
                "[FluorateGL] Default fragment shader compile error:\n{}",
                String::from_utf8_lossy(&buf[..safe_written])
            );
        } else {
            log::error!("[FluorateGL] Default fragment shader compile error (no info log)");
        }
        unsafe {
            (dispatch.delete_shader)(gles_fs);
        }
        return 0;
    }
    state::with_state(|s| {
        s.default_fs_cache.insert(version_str, gles_fs);
    });
    gles_fs
}

/// 在 glLinkProgram 中应用 program 的 pending BindFragDataLocation 绑定：
/// 找到 FS shader 的翻译后源码，注入 layout(location=color) 后重新编译。
///
/// 对齐 MG glLinkProgram 的 frag_data_changed 分支。差异：
/// - MG 修改全局 shaderInfo 对应 shader（依赖"最后 glShaderSource 的 shader"），
///   我们用 program_fs_shader 精确跟踪（attach 时记录）。
/// - MG 重新编译后 detach/attach；GLES 规范 link 使用 shader 当前编译状态，
///   我们直接重编译后 link（无需 detach/attach）。
/// - fail-open：任何一步失败（找不到源码/无匹配声明/编译失败）仅记日志，
///   不破坏原有行为。
fn apply_frag_data_bindings(dispatch: &crate::backend::dispatch::GlesDispatch, program: u32) {
    let bindings = state::with_state(|s| s.program_frag_data_bindings.remove(&program));
    let Some(bindings) = bindings else {
        return;
    };
    let fs_shader =
        state::with_state_ref(|s| s.program_fs_shader.get(&program).copied().unwrap_or(0));
    if fs_shader == 0 {
        log::debug!(
            "[FluorateGL] glLinkProgram({}): {} pending frag data binding(s) but no FS shader attached, skipping",
            program,
            bindings.len()
        );
        return;
    }
    let gles_fs = state::with_state_ref(|s| s.shaders.get_gles(fs_shader).unwrap_or(0));
    if gles_fs == 0 {
        log::debug!(
            "[FluorateGL] glLinkProgram({}): FS shader {} not in IdMap, skipping frag data patch",
            program,
            fs_shader
        );
        return;
    }
    let src = state::with_state_ref(|s| s.shader_sources.get(&fs_shader).cloned());
    let Some(mut src) = src else {
        log::debug!(
            "[FluorateGL] glLinkProgram({}): no translated source for FS shader {}, skipping frag data patch",
            program,
            fs_shader
        );
        return;
    };

    let mut modified = false;
    for (color, name) in &bindings {
        let patched = inject_frag_data_layout(&src, *color, name);
        if patched == src {
            // 无匹配的 out 声明（或注入失败）：fail-open 保持原样
            log::debug!(
                "[FluorateGL] glBindFragDataLocation({}, {}, {:?}): no matching out declaration in translated FS source, skipping",
                program,
                color,
                name
            );
            continue;
        }
        src = patched;
        modified = true;
        log::debug!(
            "[FluorateGL] glLinkProgram({}): injected layout(location={}) for {:?} into FS {}",
            program,
            color,
            name,
            fs_shader
        );
    }
    if !modified {
        return;
    }

    let c = match CString::new(src.clone()) {
        Ok(c) => c,
        Err(_) => {
            log::error!("[FluorateGL] patched FS source contains null byte, skipping patch");
            return;
        }
    };
    let ptr = c.as_ptr();
    let len = c.as_bytes().len() as i32;
    unsafe {
        (dispatch.shader_source)(gles_fs, 1, &ptr, &len);
        (dispatch.compile_shader)(gles_fs);
    }
    let mut status = 0i32;
    unsafe {
        (dispatch.get_shader_iv)(gles_fs, GL_COMPILE_STATUS, &mut status);
    }
    if status == 0 {
        let mut len2 = 0i32;
        unsafe {
            (dispatch.get_shader_iv)(gles_fs, GL_INFO_LOG_LENGTH, &mut len2);
        }
        if len2 > 0 {
            let mut buf = vec![0u8; len2 as usize];
            let mut written = 0i32;
            unsafe {
                (dispatch.get_shader_info_log)(
                    gles_fs,
                    len2,
                    &mut written,
                    buf.as_mut_ptr() as *mut libc::c_char,
                );
            }
            let safe_written = (written.max(0) as usize).min(buf.len());
            log::error!(
                "[FluorateGL] Patched FS shader {} (GLES {}) compile failed: {}\nPatched source (first 500 chars):\n{}",
                fs_shader,
                gles_fs,
                String::from_utf8_lossy(&buf[..safe_written]).trim(),
                src.chars().take(500).collect::<String>()
            );
        } else {
            log::error!(
                "[FluorateGL] Patched FS shader {} (GLES {}) compile failed (no info log)",
                fs_shader,
                gles_fs
            );
        }
        // fail-open：编译失败也更新缓存源码（与 GLES 端内容一致），
        // 不额外动作——link 结果由 glGetProgramiv(LINK_STATUS) 如实暴露。
    }
    state::with_state(|s| {
        s.shader_sources.insert(fs_shader, src);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glCreateProgram() -> u32 {
    log::debug!("[FluorateGL] glCreateProgram()");
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = (dispatch.create_program)();
        if gles_id == 0 {
            // GLES 返回 0 通常表示当前线程无 EGL 上下文（如异步加载线程）。
            // 直接返回 0 不分配 desktop_id（对齐 glCreateShader 的防御），
            // 避免宿主拿到假 id 后继续 use/query 映射到无效的 gles_id=0。
            warn_program_no_context();
            return 0;
        }
        let desktop_id = state::with_state(|s| {
            let id = s.programs.alloc(gles_id);
            // 对齐 MG glCreateProgram：初始化默认 FS 生成状态为 Unknown
            s.program_should_generate_fs.insert(id, FS_STATE_UNKNOWN);
            id
        });
        desktop_id
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteProgram(program: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| {
            let gles_id = s.programs.delete(program);
            // 清理该 program 的 uniform location 缓存，
            // 避免 program id 复用后返回过期 location
            s.uniform_location_cache.retain(|k, _| k.0 != program);
            // 清理 MG 对齐状态（program id 复用后不留残留）
            s.program_should_generate_fs.remove(&program);
            s.program_frag_data_bindings.remove(&program);
            s.program_fs_shader.remove(&program);
            // 若删除的是当前绑定 program，GLES 自动解绑（current program = 0），
            // 同步 bound_program 保证 glUseProgram 去重不产生 stale 状态
            if s.bound_program == program {
                s.bound_program = 0;
            }
            gles_id
        });
        if let Some(gles_id) = gles_id {
            (dispatch.delete_program)(gles_id);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glAttachShader(program: u32, shader: u32) {
    log::debug!("[FluorateGL] glAttachShader({}, {})", program, shader);
    backend::with_gles_dispatch(|dispatch| unsafe {
        let (gles_program, gles_shader, stage) = state::with_state_ref(|s| {
            (
                s.programs.get_gles(program).unwrap_or(0),
                s.shaders.get_gles(shader).unwrap_or(0),
                s.shader_types.get(&shader).copied().unwrap_or(0),
            )
        });
        if gles_program == 0 || gles_shader == 0 {
            log::warn!(
                "[FluorateGL] glAttachShader({}, {}): ID 映射失败 (program={}->gles={}, shader={}->gles={})",
                program,
                shader,
                program,
                gles_program,
                shader,
                gles_shader
            );
            return;
        }
        // 对齐 MG glAttachShader 的 should_generate_fs 跟踪：
        // attach FS → Never（不需要默认 FS）；仅 attach VS（且当前非 Never）→ Maybe
        // （link 时若仍无 FS 则生成默认 FS 兜底，避免 GLES link 必败）
        state::with_state(|s| {
            let e = s
                .program_should_generate_fs
                .entry(program)
                .or_insert(FS_STATE_UNKNOWN);
            if stage == GL_FRAGMENT_SHADER {
                *e = FS_STATE_NEVER;
                // 记录最近 attach 的 FS（frag_data layout 注入定位用）
                s.program_fs_shader.insert(program, shader);
            } else if stage == GL_VERTEX_SHADER && *e != FS_STATE_NEVER {
                *e = FS_STATE_MAYBE;
            }
        });
        (dispatch.attach_shader)(gles_program, gles_shader);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glLinkProgram(program: u32) {
    log::debug!("[FluorateGL] glLinkProgram({})", program);
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| {
            // link/relink 后 uniform location 可能变化，清理该 program 的缓存，
            // 避免 relink 后返回过期 location（与 glDeleteProgram 清理模式对称）。
            // 在 link 前清理：即便 link 失败，清空缓存也只是让后续查询重新走 FFI，无副作用。
            s.uniform_location_cache.retain(|k, _| k.0 != program);
            s.programs.get_gles(program).unwrap_or(0)
        });
        if gles_id == 0 {
            log::debug!(
                "[FluorateGL] glLinkProgram({}) -> unknown desktop id, skipping",
                program
            );
            return;
        }

        // 对齐 MG glLinkProgram：
        // 1) 应用 glBindFragDataLocation 的 pending 绑定（对 FS 翻译后源码注入
        //    layout(location=N) 并重新编译）——MG 的 frag_data_changed 分支
        apply_frag_data_bindings(dispatch, program);

        // 2) 需要时生成/复用默认 FS 并 attach（MG should_generate_fs == Maybe 分支）。
        //    fail-open：生成失败（无 context/编译失败）则跳过 attach，行为回退原状。
        let should_generate_fs = state::with_state_ref(|s| {
            s.program_should_generate_fs
                .get(&program)
                .copied()
                .unwrap_or(0)
        });
        if should_generate_fs == FS_STATE_MAYBE {
            let default_fs = generate_default_fs(dispatch);
            if default_fs != 0 {
                log::debug!(
                    "[FluorateGL] glLinkProgram({}): attach default FS (GLES {}) for program with no FS",
                    program,
                    default_fs
                );
                (dispatch.attach_shader)(gles_id, default_fs);
            }
        }

        (dispatch.link_program)(gles_id);

        // 检查链接状态
        let mut status = 0i32;
        (dispatch.get_program_iv)(gles_id, GL_LINK_STATUS, &mut status);
        if status == 0 {
            let mut len = 0i32;
            (dispatch.get_program_iv)(gles_id, GL_INFO_LOG_LENGTH, &mut len);
            if len > 0 {
                let mut buf = vec![0u8; len as usize];
                let mut written = 0i32;
                (dispatch.get_program_info_log)(
                    gles_id,
                    len,
                    &mut written,
                    buf.as_mut_ptr() as *mut libc::c_char,
                );
                let safe_written = (written.max(0) as usize).min(buf.len());
                let info = String::from_utf8_lossy(&buf[..safe_written]);
                log::error!(
                    "[FluorateGL] Program {} (GLES {}) link failed: {}",
                    program,
                    gles_id,
                    info
                );
            } else {
                log::error!(
                    "[FluorateGL] Program {} (GLES {}) link failed (no info log)",
                    program,
                    gles_id
                );
            }
        } else {
            log::debug!(
                "[FluorateGL] Program {} (GLES {}) link OK",
                program,
                gles_id
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUseProgram(program: u32) {
    log::debug!("[FluorateGL] glUseProgram({})", program);
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = if program == 0 {
            0
        } else {
            state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0))
        };
        if program != 0 && gles_id == 0 {
            log::warn!(
                "[FluorateGL] glUseProgram({}): desktop program not in IdMap, gles_id=0 (tid={})",
                program,
                state::thread_id_u64()
            );
        }
        // 对齐 MG glUseProgram：program 未变化时跳过 GLES 调用（热路径去重）。
        // 一致性前提：所有可能改变当前 program 的路径都同步 bound_program——
        // 唯一例外是 glDeleteProgram 删除当前 program（GLES 自动解绑），
        // 已在 glDeleteProgram 中重置 bound_program=0。
        let changed = state::with_state_ref(|s| s.bound_program != program);
        if changed {
            (dispatch.use_program)(gles_id);
            state::with_state(|s| s.bound_program = program);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetProgramiv(program: u32, pname: u32, params: *mut i32) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            // program 不在 IdMap 中：可能是跨线程查询或 GLES 创建失败。
            // 显式写 *params = 0（GL_FALSE），避免调用方读到未初始化栈值。
            warn_program_id_miss(program);
            *params = 0;
            return;
        }
        (dispatch.get_program_iv)(gles_id, pname, params);

        // fail-fast: 真实返回 link/validate 状态，不欺骗为 GL_TRUE。
        // 保留 error 级诊断日志，让失败有迹可循，便于定位 SPIR-V 翻译根因。
        // 注意：此处不采用 MG 的 ignore_error cheat（把失败伪装成 GL_TRUE）——
        // 差分测试（desktop/gles/translate 三后端 A/B）要求 LINK/VALIDATE 状态
        // 如实返回，cheat 会导致 translate 与 gles 后端不一致而 FAIL；
        // 且翻译失败属异常路径，需要每次可见以便诊断。
        // 注意：此处不采用首次告警模式。MC 正常运行时 link 状态恒为 GL_TRUE，
        // 仅在翻译失败时返回 GL_FALSE，属于异常路径，需要每次可见以便诊断。
        const GL_VALIDATE_STATUS: u32 = 0x8B8B;
        if (pname == GL_LINK_STATUS || pname == GL_VALIDATE_STATUS) && *params == 0 {
            log::error!(
                "[FluorateGL] Program {} (GLES {}) {} failed (fail-fast, returning GL_FALSE)",
                program,
                gles_id,
                if pname == GL_LINK_STATUS {
                    "link"
                } else {
                    "validate"
                }
            );
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetProgramInfoLog(
    program: u32,
    buf_size: i32,
    length: *mut i32,
    info_log: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_program_info_log)(gles_id, buf_size, length, info_log);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformLocation(program: u32, name: *const c_char) -> i32 {
    if name.is_null() {
        return -1;
    }
    // MC 渲染循环中可能反复查询同一 uniform（如 F3 重载 shader 后），
    // 用 (program, name) 缓存 location，避免重复 FFI 查询。
    let name_str = unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned();
    if let Some(loc) = state::with_state_ref(|s| {
        s.uniform_location_cache
            .get(&(program, name_str.clone()))
            .copied()
    }) {
        log::debug!(
            "[FluorateGL] glGetUniformLocation(program={}, name={:?}) = {} (cached)",
            program,
            name_str,
            loc
        );
        return loc;
    }
    let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
    if gles_id == 0 {
        log::debug!(
            "[FluorateGL] glGetUniformLocation: program {} not in IdMap, returning -1 for {}",
            program,
            name_str
        );
        return -1;
    }
    let loc = backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_uniform_location)(gles_id, name)
    });
    // 排查日志：记录每次真实查询（含成功查询）的名称与返回值
    log::debug!(
        "[FluorateGL] glGetUniformLocation(program={}, name={:?}) = {}",
        program,
        name_str,
        loc
    );
    // 仅记录查询失败的 uniform（返回 -1），帮助定位 shader 翻译问题
    if loc < 0 {
        log::warn!(
            "[FluorateGL] glGetUniformLocation: program(desktop={}) gles={} name={:?} -> -1 (NOT FOUND)",
            program,
            gles_id,
            name_str
        );
    }
    state::with_state(|s| {
        s.uniform_location_cache.insert((program, name_str), loc);
    });
    loc
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetAttribLocation(program: u32, name: *const c_char) -> i32 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return -1;
        }
        (dispatch.get_attrib_location)(gles_id, name)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform1f(location: i32, v0: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_1f)(location, v0);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform1i(location: i32, v0: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_1i)(location, v0);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniformMatrix4fv(location: i32, count: i32, transpose: u8, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_matrix_4fv)(location, count, transpose, value);
    });
}

// ==== Unsigned uniform 系列（GL 3.0 core 补齐，GLES 3.0 原生透传）====
macro_rules! uniform_ui_fn {
    ($name:ident, $field:ident, $($arg:ident: $ty:ty),*) => {
        #[unsafe(no_mangle)]
        #[allow(non_snake_case)]
        pub extern "C" fn $name(location: i32, $($arg: $ty),*) {
            backend::with_gles_dispatch(|dispatch| unsafe {
                (dispatch.$field)(location, $($arg),*);
            });
        }
    };
}

uniform_ui_fn!(glUniform1ui, uniform_1ui, v0: u32);
uniform_ui_fn!(glUniform2ui, uniform_2ui, v0: u32, v1: u32);
uniform_ui_fn!(glUniform3ui, uniform_3ui, v0: u32, v1: u32, v2: u32);
uniform_ui_fn!(glUniform4ui, uniform_4ui, v0: u32, v1: u32, v2: u32, v3: u32);
uniform_ui_fn!(glUniform1uiv, uniform_1uiv, count: i32, value: *const u32);
uniform_ui_fn!(glUniform2uiv, uniform_2uiv, count: i32, value: *const u32);
uniform_ui_fn!(glUniform3uiv, uniform_3uiv, count: i32, value: *const u32);
uniform_ui_fn!(glUniform4uiv, uniform_4uiv, count: i32, value: *const u32);

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformuiv(program: u32, location: i32, params: *mut u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_uniform_uiv)(program, location, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDetachShader(program: u32, shader: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let (gles_program, gles_shader) = state::with_state_ref(|s| {
            (
                s.programs.get_gles(program).unwrap_or(0),
                s.shaders.get_gles(shader).unwrap_or(0),
            )
        });
        if gles_program == 0 || gles_shader == 0 {
            return;
        }
        // 若 detach 的是该 program 跟踪的 FS，清理引用（防 frag_data 注入
        // 命中已 detach 的 shader）
        state::with_state(|s| {
            if s.program_fs_shader.get(&program) == Some(&shader) {
                s.program_fs_shader.remove(&program);
            }
        });
        (dispatch.detach_shader)(gles_program, gles_shader);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glValidateProgram(program: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.validate_program)(gles_id);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveUniform(
    program: u32,
    index: u32,
    buf_size: i32,
    length: *mut i32,
    size: *mut i32,
    type_: *mut u32,
    name: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_active_uniform)(gles_id, index, buf_size, length, size, type_, name);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveAttrib(
    program: u32,
    index: u32,
    buf_size: i32,
    length: *mut i32,
    size: *mut i32,
    type_: *mut u32,
    name: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_active_attrib)(gles_id, index, buf_size, length, size, type_, name);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformfv(program: u32, location: i32, params: *mut f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            if !params.is_null() {
                *params = 0.0;
            }
            return;
        }
        (dispatch.get_uniform_fv)(gles_id, location, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformiv(program: u32, location: i32, params: *mut i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            if !params.is_null() {
                *params = 0;
            }
            return;
        }
        (dispatch.get_uniform_iv)(gles_id, location, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetAttachedShaders(
    program: u32,
    max_count: i32,
    count: *mut i32,
    shaders: *mut u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }

        if max_count > 0 && !shaders.is_null() {
            // MC program 通常 attach ≤2 个 shader（VS+FS），用栈上 buffer 避免堆分配；
            // 超过 16 个（理论极限 VS+TCS+TES+GS+FS+额外）才回退堆。
            let mut stack_buf = [0u32; 16];
            let need_heap = (max_count as usize) > stack_buf.len();
            let mut heap_buf = if need_heap {
                vec![0u32; max_count as usize]
            } else {
                Vec::new()
            };
            let gles_shaders: &mut [u32] = if need_heap {
                &mut heap_buf
            } else {
                &mut stack_buf[..max_count as usize]
            };

            (dispatch.get_attached_shaders)(gles_id, max_count, count, gles_shaders.as_mut_ptr());

            let returned_count = if count.is_null() {
                max_count
            } else {
                (*count).clamp(0, max_count)
            };

            // 一次 with_state_ref 持有 borrow，批量把 GLES shader id 翻译回 desktop id，
            // 避免循环内每次迭代都访问 thread_local。
            state::with_state_ref(|s| {
                for i in 0..returned_count as isize {
                    let gles_shader = *gles_shaders.as_ptr().offset(i);
                    let desktop_shader = s.shaders.get_desktop(gles_shader).unwrap_or(gles_shader);
                    *shaders.offset(i) = desktop_shader;
                }
            });
        } else {
            (dispatch.get_attached_shaders)(gles_id, max_count, count, shaders);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindAttribLocation(program: u32, index: u32, name: *const c_char) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.bind_attrib_location)(gles_id, index, name);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glTransformFeedbackVaryings(
    program: u32,
    count: i32,
    varyings: *const *const c_char,
    buffer_mode: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.transform_feedback_varyings)(gles_id, count, varyings, buffer_mode);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetTransformFeedbackVarying(
    program: u32,
    index: u32,
    buf_size: i32,
    length: *mut i32,
    size: *mut i32,
    type_: *mut u32,
    name: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_transform_feedback_varying)(
            gles_id, index, buf_size, length, size, type_, name,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniformBlockBinding(
    program: u32,
    uniform_block_index: u32,
    uniform_block_binding: u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            log::debug!(
                "[FluorateGL] glUniformBlockBinding(program={}, block_index={}, binding={}) -> skipped (program not in IdMap)",
                program,
                uniform_block_index,
                uniform_block_binding
            );
            return;
        }
        (dispatch.uniform_block_binding)(gles_id, uniform_block_index, uniform_block_binding);
        log::debug!(
            "[FluorateGL] glUniformBlockBinding(program={}, block_index={}, binding={}) (gles={})",
            program,
            uniform_block_index,
            uniform_block_binding,
            gles_id
        );
    });
}

/// glBindFragDataLocation — 对齐 MG program.cpp 语义。
///
/// 桌面 GL 3.0 函数，GLES 无对应实现。语义：指定 fragment shader output
/// 变量绑定的 color attachment location。
///
/// 实现（对齐 MG，但注入统一走 glLinkProgram 时对翻译后 FS 源码的 patch）：
/// 记录 (color, name) 到 program 的 pending bindings，glLinkProgram 时对
/// 该 program 的 FS 翻译后源码注入 `layout (location = N) out T name;` 并
/// 重新编译（见 apply_frag_data_bindings）。
///
/// 与 MG 的差异（有意为之）：
/// - MG 对 outColorN（N==color）跳过，依赖其转换管线保证已绑定 location；
///   我们 SPIR-V 主路径（postprocess fix_out_color_locations）同保证——
///   统一注入时前缀组吞掉管线已注入的 layout 并重写为相同值（结果等价）；
///   而 string_pass 兜底未注入 location → 统一注入恰好修正。两条路径都安全。
/// - MG 修改全局 shaderInfo 单例（最后 glShaderSource 的 shader）；我们
///   per-program 跟踪 FS shader（attach 时记录），精确且线程安全。
///
/// 必须导出此函数：Sodium 的 ShaderChunkRenderer.createShader() 在
/// glAttachShader 后调用 glBindFragDataLocation 绑定 fragment output。
/// 若未导出，eglGetProcAddress 返回 null，LWJGL capabilities 字段为 null，
/// 调用时抛 "No context is current" 错误，导致 chunk shader 创建中断，
/// 方块无法渲染（实体 shader 不调用此函数，故实体正常）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindFragDataLocation(program: u32, color_number: u32, name: *const c_char) {
    let name_str = if name.is_null() {
        "<null>".to_string()
    } else {
        unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    };
    log::debug!(
        "[FluorateGL] glBindFragDataLocation(program={}, color={}, name={:?}) -> recorded, applied at glLinkProgram",
        program,
        color_number,
        name_str
    );
    if name_str == "<null>" {
        return;
    }
    if let Some(n) = parse_out_color_suffix(&name_str) {
        // MG 在此场景跳过（依赖其转换管线已绑定 location）；我们统一走注入：
        // SPIR-V 主路径已带 layout → 注入正则不匹配 → 天然无操作；
        // string_pass 兜底未带 layout → 注入修正。两条路径都安全。
        log::debug!(
            "[FluorateGL] glBindFragDataLocation: name {:?} 匹配 outColor{} 模式（color={}）",
            name_str,
            n,
            color_number
        );
    }
    state::with_state(|s| {
        s.program_frag_data_bindings
            .entry(program)
            .or_default()
            .push((color_number, name_str));
    });
}

/// glBindFragDataLocationIndexed stub — 桌面 GL 3.3 函数，GLES 无对应实现。
///
/// 与 glBindFragDataLocation 类似，但支持 indexed fragment output（dual-source blending）。
/// GLES 不支持 dual-source blending，no-op 实现安全（MC/Sodium 不使用此特性）。
/// 导出避免 LWJGL capabilities 字段为 null 导致调用时抛错。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBindFragDataLocationIndexed(
    program: u32,
    color_number: u32,
    index: u32,
    name: *const c_char,
) {
    let name_str = if name.is_null() {
        "<null>".to_string()
    } else {
        unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    };
    log::debug!(
        "[FluorateGL] glBindFragDataLocationIndexed(program={}, color={}, index={}, name={:?}) -> no-op (GLES auto-assigns, dual-source unsupported)",
        program,
        color_number,
        index,
        name_str
    );
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformBlockIndex(program: u32, uniform_block_name: *const c_char) -> u32 {
    let name_str = if uniform_block_name.is_null() {
        "<null>".to_string()
    } else {
        unsafe { CStr::from_ptr(uniform_block_name) }
            .to_string_lossy()
            .into_owned()
    };
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            log::debug!(
                "[FluorateGL] glGetUniformBlockIndex(program={}, name={:?}) -> 0xFFFFFFFF (program not in IdMap)",
                program,
                name_str
            );
            return u32::MAX;
        }
        let index = (dispatch.get_uniform_block_index)(gles_id, uniform_block_name);
        log::debug!(
            "[FluorateGL] glGetUniformBlockIndex(program={}, name={:?}) -> {} (0x{:08X})",
            program,
            name_str,
            index,
            index
        );
        index
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveUniformBlockiv(
    program: u32,
    uniform_block_index: u32,
    pname: u32,
    params: *mut i32,
) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            *params = 0;
            return;
        }
        (dispatch.get_active_uniform_block_iv)(gles_id, uniform_block_index, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveUniformBlockName(
    program: u32,
    uniform_block_index: u32,
    buf_size: i32,
    length: *mut i32,
    uniform_block_name: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_active_uniform_block_name)(
            gles_id,
            uniform_block_index,
            buf_size,
            length,
            uniform_block_name,
        );
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetUniformIndices(
    program: u32,
    uniform_count: i32,
    uniform_names: *const *const c_char,
    uniform_indices: *mut u32,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return;
        }
        (dispatch.get_uniform_indices)(gles_id, uniform_count, uniform_names, uniform_indices);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveUniformsiv(
    program: u32,
    uniform_count: i32,
    uniform_indices: *const u32,
    pname: u32,
    params: *mut i32,
) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            *params = 0;
            return;
        }
        (dispatch.get_active_uniforms_iv)(gles_id, uniform_count, uniform_indices, pname, params);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsProgram(program: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            return 0;
        }
        (dispatch.is_program)(gles_id)
    })
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform2f(location: i32, v0: f32, v1: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_2f)(location, v0, v1);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform3f(location: i32, v0: f32, v1: f32, v2: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_3f)(location, v0, v1, v2);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform4f(location: i32, v0: f32, v1: f32, v2: f32, v3: f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_4f)(location, v0, v1, v2, v3);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform2i(location: i32, v0: i32, v1: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_2i)(location, v0, v1);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform3i(location: i32, v0: i32, v1: i32, v2: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_3i)(location, v0, v1, v2);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform4i(location: i32, v0: i32, v1: i32, v2: i32, v3: i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_4i)(location, v0, v1, v2, v3);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform1fv(location: i32, count: i32, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_1fv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform2fv(location: i32, count: i32, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_2fv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform3fv(location: i32, count: i32, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_3fv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform4fv(location: i32, count: i32, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_4fv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform1iv(location: i32, count: i32, value: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_1iv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform2iv(location: i32, count: i32, value: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_2iv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform3iv(location: i32, count: i32, value: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_3iv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniform4iv(location: i32, count: i32, value: *const i32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_4iv)(location, count, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniformMatrix2fv(location: i32, count: i32, transpose: u8, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_matrix_2fv)(location, count, transpose, value);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glUniformMatrix3fv(location: i32, count: i32, transpose: u8, value: *const f32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.uniform_matrix_3fv)(location, count, transpose, value);
    });
}

/// glShaderStorageBlockBinding stub — GL_ARB_shader_storage_buffer_object 扩展函数，no-op 实现。
///
/// 语义：修改 SSBO 的绑定点。GLES 3.1 支持 SSBO 但绑定方式不同，
/// no-op 安全（MC/Sodium 不依赖动态 SSBO 重绑定）。已声明扩展，必须导出
/// 避免 LWJGL capabilities 字段为 null 导致调用时抛错。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glShaderStorageBlockBinding(
    program: u32,
    storage_block_index: u32,
    storage_block_binding: u32,
) {
    log::debug!(
        "[FluorateGL] glShaderStorageBlockBinding(program={}, block={}, binding={}) -> no-op (SSBO rebind unsupported)",
        program,
        storage_block_index,
        storage_block_binding
    );
}

/// glProgramParameteri stub — GL 4.1 函数，no-op 实现。
///
/// 语义：设置 program 的参数（如 GL_PROGRAM_SEPARABLE）。GLES 不需要这些 flag，
/// no-op 安全。必须导出避免 LWJGL capabilities 字段为 null。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glProgramParameteri(program: u32, pname: u32, value: i32) {
    log::debug!(
        "[FluorateGL] glProgramParameteri(program={}, pname={}, value={}) -> no-op (GLES ignores program params)",
        program,
        pname,
        value
    );
}

/// glGetFragDataIndex stub — GL 3.3 函数，返回 -1。
///
/// 语义：查询 fragment output 的 index（dual-source blending）。GLES 不支持
/// dual-source blending，返回 -1 表示无 indexed output。必须导出避免
/// LWJGL capabilities 字段为 null。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetFragDataIndex(program: u32, name: *const c_char) -> i32 {
    let name_str = if name.is_null() {
        "<null>".to_string()
    } else {
        unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    };
    log::debug!(
        "[FluorateGL] glGetFragDataIndex(program={}, name={:?}) -> -1 (dual-source blending unsupported)",
        program,
        name_str
    );
    -1
}

/// glGetFragDataLocation — GL 3.0 core 函数，GLES 3.0 原生支持，直通。
///
/// 语义：查询 fragment shader output 变量的 location。
/// GLES 3.0 规范原生提供 glGetFragDataLocation，故直接翻译 desktop→GLES id 后直通。
/// 旧实现未导出此符号：宿主报告 GL 3.3 时 LWJGL GL30 类会 dlsym 加载它，
/// 失败后函数地址为 0，宿主调用即崩溃（S2 修复）。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetFragDataLocation(program: u32, name: *const c_char) -> i32 {
    if name.is_null() {
        return -1;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            log::debug!(
                "[FluorateGL] glGetFragDataLocation: program {} not in IdMap, returning -1",
                program
            );
            return -1;
        }
        (dispatch.get_frag_data_location)(gles_id, name)
    })
}

/// glGetActiveUniformName — GL 2.0 core 函数，转发 glGetActiveUniform 并提取 name。
///
/// 语义：查询指定 index 的 uniform 名称。glGetActiveUniform 返回 size/type/name，
/// 本函数只关心 name，忽略 size 和 type_（传入临时变量接收后丢弃）。
///
/// 对齐 MG program.cpp glGetActiveUniformName 的边界语义：
/// - length 先置 0（调用方读到 0 而非垃圾值）
/// - bufSize <= 0 或 name 为 null 时不写任何内容直接返回
/// - bufSize < 0 时仍转发驱动（负 bufSize 触发 GL_INVALID_VALUE，调用方应得
///   到的规范错误不能吞掉——MG 注释：这是 NeoForge 早期加载窗口的修复点）
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetActiveUniformName(
    program: u32,
    index: u32,
    buf_size: i32,
    length: *mut i32,
    name: *mut c_char,
) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state_ref(|s| s.programs.get_gles(program).unwrap_or(0));
        if gles_id == 0 {
            // 我们架构必需（MG 无 IdMap 概念）：program 不在 IdMap 时无法
            // 转发，保持原样返回
            return;
        }
        if !length.is_null() {
            *length = 0;
        }
        if buf_size <= 0 || name.is_null() {
            // 无内容可写。bufSize 为负时仍转发驱动，让其产生调用方应得的
            // GL_INVALID_VALUE（name 等指针为 null 不触碰——驱动仅凭负
            // bufSize 报错，对齐 MG 行为）。
            if buf_size < 0 {
                (dispatch.get_active_uniform)(
                    gles_id,
                    index,
                    buf_size,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
            }
            return;
        }
        *name = 0;
        // glGetActiveUniform 需要 size 和 type_ 参数，本函数忽略它们，
        // 用临时变量接收后丢弃。written 单独记录（MG 语义：length 返回
        // 实际写入长度，不含结尾 null）。
        let mut size = 0i32;
        let mut type_ = 0u32;
        let mut written = 0i32;
        (dispatch.get_active_uniform)(
            gles_id,
            index,
            buf_size,
            &mut written,
            &mut size,
            &mut type_,
            name,
        );
        if !length.is_null() {
            *length = written;
        }
    });
}
