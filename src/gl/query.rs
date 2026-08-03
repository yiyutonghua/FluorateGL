use crate::backend;
use crate::state;

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGenQueries(n: i32, ids: *mut u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let mut gles_id = 0u32;
            (dispatch.gen_queries)(1, &mut gles_id);

            let desktop_id = state::with_state(|s| s.queries.alloc(gles_id));
            *ids.offset(i) = desktop_id;
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDeleteQueries(n: i32, ids: *const u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        for i in 0..n as isize {
            let desktop_id = *ids.offset(i);
            if let Some(gles_id) = state::with_state(|s| s.queries.delete(desktop_id)) {
                (dispatch.delete_queries)(1, &gles_id);
            }
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsQuery(id: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.queries.get_gles(id).unwrap_or(0));
        (dispatch.is_query)(gles_id)
    })
}

/// 桌面查询 target → GLES target（GLES 无 GL_SAMPLES_PASSED，映射到 ANY_SAMPLES_PASSED）。
///
/// 桌面 GL 的遮挡查询用 GL_SAMPLES_PASSED(0x8914)，而 GLES 3.0/3.1/3.2 只接受
/// GL_ANY_SAMPLES_PASSED(0x8C2F)、GL_ANY_SAMPLES_PASSED_CONSERVATIVE(0x8D6A) 与
/// GL_TRANSFORM_FEEDBACK_PRIMITIVES_WRITTEN(0x8B88)。不映射时 GLES 报 INVALID_ENUM，
/// 查询对象从未激活 → GL_QUERY_RESULT_AVAILABLE 恒 FALSE → MC 遮挡剔除误判"全部被遮挡"
/// → 跳过渲染（方块消失/闪烁）。
///
/// 语义兼容性：ANY_SAMPLES_PASSED 与 SAMPLES_PASSED 都表示片段通过深度/模板测试，
/// 区别仅在于 ANY 不统计通过数量（结果为 0/1）。宿主（如 MC）只查
/// RESULT_AVAILABLE 与结果非零判断，不依赖精确计数，映射安全。
fn translate_query_target(target: u32) -> u32 {
    if target == 0x8914 {
        0x8C2F // GL_SAMPLES_PASSED → GL_ANY_SAMPLES_PASSED
    } else {
        target
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBeginQuery(target: u32, id: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.queries.get_gles(id).unwrap_or(0));
        (dispatch.begin_query)(translate_query_target(target), gles_id);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEndQuery(target: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.end_query)(translate_query_target(target));
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetQueryiv(target: u32, pname: u32, params: *mut i32) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_query_iv)(translate_query_target(target), pname, params);
    });
}

fn is_stub(dispatch: &backend::dispatch::GlesDispatch, f: *const ()) -> bool {
    f == dispatch.stub as *const ()
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetQueryObjectiv(id: u32, pname: u32, params: *mut i32) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.queries.get_gles(id).unwrap_or(0));
        if gles_id == 0 {
            *params = 0;
            return;
        }
        if is_stub(dispatch, dispatch.get_query_object_iv as *const ()) {
            let mut value = 0u32;
            (dispatch.get_query_object_uiv)(gles_id, pname, &mut value);
            *params = value as i32;
        } else {
            (dispatch.get_query_object_iv)(gles_id, pname, params);
        }
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetQueryObjectuiv(id: u32, pname: u32, params: *mut u32) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.queries.get_gles(id).unwrap_or(0));
        if gles_id == 0 {
            *params = 0;
            return;
        }
        (dispatch.get_query_object_uiv)(gles_id, pname, params);
    });
}

/// glQueryCounter stub — GL_ARB_timer_query 扩展函数，no-op 实现。
///
/// 语义：记录 GPU 时间戳到 query object。GLES 3.2 有 glQueryCounterEXT 扩展版本，
/// 但为简化先 no-op。已声明 GL_ARB_timer_query 扩展，必须导出避免 LWJGL
/// capabilities 字段为 null 导致调用时抛错。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glQueryCounter(id: u32, target: u32) {
    log::debug!(
        "[FluorateGL] glQueryCounter(id={}, target={}) -> no-op (timer query stub)",
        id,
        target
    );
}

/// glGetQueryObjecti64v stub — GL_ARB_timer_query 扩展函数，返回 0。
///
/// 语义：查询 query object 的 64 位有符号整数值（如 GPU 时间戳）。
/// no-op 返回 0，调用方看到 0 时间戳。已声明扩展，必须导出。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetQueryObjecti64v(id: u32, pname: u32, params: *mut i64) {
    log::debug!(
        "[FluorateGL] glGetQueryObjecti64v(id={}, pname={}) -> 0 (timer query stub)",
        id,
        pname
    );
    if !params.is_null() {
        unsafe { *params = 0 };
    }
}

/// glGetQueryObjectui64v stub — GL_ARB_timer_query 扩展函数，返回 0。
///
/// 语义：查询 query object 的 64 位无符号整数值。no-op 返回 0。
/// 已声明扩展，必须导出。
#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetQueryObjectui64v(id: u32, pname: u32, params: *mut u64) {
    log::debug!(
        "[FluorateGL] glGetQueryObjectui64v(id={}, pname={}) -> 0 (timer query stub)",
        id,
        pname
    );
    if !params.is_null() {
        unsafe { *params = 0 };
    }
}
