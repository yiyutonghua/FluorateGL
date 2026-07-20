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

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glBeginQuery(target: u32, id: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        let gles_id = state::with_state(|s| s.queries.get_gles(id).unwrap_or(0));
        (dispatch.begin_query)(target, gles_id);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEndQuery(target: u32) {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.end_query)(target);
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glGetQueryiv(target: u32, pname: u32, params: *mut i32) {
    if params.is_null() {
        return;
    }
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.get_query_iv)(target, pname, params);
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
