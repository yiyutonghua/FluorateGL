//! GLSL转译器宏定义模块
//!
//! 包含统一的错误处理、日志记录和GL实现方式优化宏
//!
//! 说明：warn_once/gl_check_error/check_id_mapping/is_stub/gles_dispatch/
//! optimize_spirv_compile 等宏为后续 GL 层预留，暂无调用点，
//! 用 #[allow(unused_macros)] 抑制警告（保留定义供后续使用）。

#![allow(unused_macros)]

/// 警告一次宏
///
/// 用于在首次出现特定情况时发出警告，后续不再重复警告
macro_rules! warn_once {
    ($msg:expr) => {
        static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if WARNED.get().is_none() {
            log::warn!($msg);
            let _ = WARNED.set(());
        }
    };
    ($msg:expr, $($arg:tt)*) => {
        static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        if WARNED.get().is_none() {
            log::warn!($msg, $($arg)*);
            let _ = WARNED.set(());
        }
    };
}

/// GL错误处理宏
///
/// 统一处理glGetError，避免静默返回
macro_rules! gl_check_error {
    () => {
        gl_check_error!("GL error detected")
    };
    ($context:expr) => {
        let err = unsafe { gl::GetError() };
        if err != gl::NO_ERROR {
            log::error!("[ShaderTranslator] {} - GL Error: {}", $context, err);
            // 在调试模式下可以添加更多错误处理
            #[cfg(debug_assertions)]
            {
                // 这里可以添加断言或更详细的错误处理
                assert!(false, "GL error: {} - {}", $context, err);
            }
        }
    };
}

/// 检查ID映射失败并返回错误
macro_rules! check_id_mapping {
    ($id:expr, $context:expr) => {
        if $id == 0 {
            log::error!(
                "[ShaderTranslator] {} - ID mapping failed, returned 0",
                $context
            );
            return None;
        }
    };
}

/// 检查GL实现是否为stub实现
macro_rules! is_stub {
    ($gl_impl:expr) => {
        $gl_impl.is_stub()
    };
}

/// GlesDispatch方法提取宏
macro_rules! gles_dispatch {
    ($gl_impl:expr, $method:ident, $($args:expr),*) => {
        if is_stub!($gl_impl) {
            $gl_impl.$method($($args),*)
        } else {
            $gl_impl.$method($($args),*)
        }
    };
}

/// 简化Vulkan workarounds的宏
#[macro_export]
macro_rules! simplify_vulkan_workarounds {
    ($code:expr) => {
        // 简化Vulkan workarounds逻辑
        // 这里可以添加更具体的简化逻辑
        $code
    };
}

/// 优化SPIR-V编译逻辑的宏
macro_rules! optimize_spirv_compile {
    ($options:expr) => {
        // 优化SPIR-V编译选项（shaderc 0.10.1 API）
        $options.set_target_env(
            shaderc::TargetEnv::Vulkan,
            shaderc::EnvVersion::Vulkan1_2 as u32,
        );
        $options.set_optimization_level(shaderc::OptimizationLevel::Performance);
        $options.set_generate_debug_info();
        $options.set_auto_bind_uniforms(true);
        $options.set_target_spirv(shaderc::SpirvVersion::V1_5);
    };
}

/// 简化UBO处理的宏
#[macro_export]
macro_rules! simplify_ubo_processing {
    ($code:expr) => {
        // 简化UBO处理逻辑
        $code
    };
}
