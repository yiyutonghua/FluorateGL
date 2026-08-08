//! Transform Feedback 家族导出（GL 3.0/3.3 core 补齐）
//!
//! GL 3.0 引入 transform feedback（TF），GLES 3.0 原生支持同名函数——透传。
//! glDrawTransformFeedback/glDrawTransformFeedbackInstanced 已 stub（drawing.rs）；
//! glDrawTransformFeedbackStream/Instanced 同理 stub（GLES 无对应，语义无法模拟）。
//! 此前 dispatch 已有 8 个字段但未导出符号（D2 只 stub 了 draw 版本）——
//! LWJGL 绑定 null 有崩溃风险（光影/Mod 用 TF 时触发）。

use crate::backend;
use std::sync::atomic::{AtomicBool, Ordering};

macro_rules! passthrough_void {
    ($name:ident, $field:ident, $($arg:ident: $ty:ty),*) => {
        #[unsafe(no_mangle)]
        #[allow(non_snake_case)]
        pub extern "C" fn $name($($arg: $ty),*) {
            backend::with_gles_dispatch(|dispatch| unsafe {
                (dispatch.$field)($($arg),*);
            });
        }
    };
}

passthrough_void!(glGenTransformFeedbacks, gen_transform_feedbacks, count: i32, ids: *mut u32);
passthrough_void!(glDeleteTransformFeedbacks, delete_transform_feedbacks, count: i32, ids: *const u32);
passthrough_void!(glBindTransformFeedback, bind_transform_feedback, target: u32, id: u32);
passthrough_void!(glBeginTransformFeedback, begin_transform_feedback, primitive_mode: u32);

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glEndTransformFeedback() {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.end_transform_feedback)();
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glPauseTransformFeedback() {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.pause_transform_feedback)();
    });
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glResumeTransformFeedback() {
    backend::with_gles_dispatch(|dispatch| unsafe {
        (dispatch.resume_transform_feedback)();
    });
}

passthrough_void!(glTransformFeedbackBufferBase, transform_feedback_buffer_base, xfb: u32, index: u32, buffer: u32);
passthrough_void!(glTransformFeedbackBufferRange, transform_feedback_buffer_range, xfb: u32, index: u32, buffer: u32, offset: isize, size: isize);
passthrough_void!(glGetTransformFeedbackiv, get_transform_feedback_iv, xfb: u32, pname: u32, param: *mut i32);

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glIsTransformFeedback(id: u32) -> u8 {
    backend::with_gles_dispatch(|dispatch| unsafe { (dispatch.is_transform_feedback)(id) })
}

/// glDrawTransformFeedbackStream / Instanced：GLES 无对应（transform feedback
/// 回读绘制不存在），stub no-op + 首调告警（同 glDrawTransformFeedback 模式）。
static TF_STREAM_WARNED: AtomicBool = AtomicBool::new(false);

fn warn_tf_stream_unsupported(fname: &str) {
    if !TF_STREAM_WARNED.swap(true, Ordering::Relaxed) {
        log::warn!(
            "[FluorateGL] {}: GLES 无 transform feedback 流回读绘制，已 no-op（后续调用静默跳过）",
            fname
        );
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawTransformFeedbackStream(_mode: u32, _id: u32, _stream: u32) {
    warn_tf_stream_unsupported("glDrawTransformFeedbackStream");
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub extern "C" fn glDrawTransformFeedbackStreamInstanced(
    _mode: u32,
    _id: u32,
    _stream: u32,
    _instancecount: i32,
) {
    warn_tf_stream_unsupported("glDrawTransformFeedbackStreamInstanced");
}
