use std::env;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Backend {
    /// Android 系统 GLES（libGLESv3.so + libEGL.so）
    System,
    /// ANGLE 转译层（libGLESv2_angle.so + libEGL_angle.so）
    Angle,
    /// Linux Mesa llvmpipe 软件光栅化（libGLESv2.so.2 + libEGL.so.1）
    /// 用于 CI/无显示器的 Linux 环境，需配合 EGL_PLATFORM=surfaceless
    /// + MESA_LOADER_DRIVER_OVERRIDE=llvmpipe 环境变量使用
    Llvmpipe,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub backend: Backend,
    pub log_level: LogLevel,
}

impl Config {
    pub fn from_env() -> Self {
        let backend = match env::var("FLUORATEGL_BACKEND").as_deref() {
            Ok("angle") => Backend::Angle,
            Ok("system") => Backend::System,
            Ok("llvmpipe") => Backend::Llvmpipe,
            _ => Backend::System,
        };

        let log_level = match env::var("FLUORATEGL_LOG").as_deref() {
            Ok("error") => LogLevel::Error,
            Ok("warn") => LogLevel::Warn,
            Ok("info") => LogLevel::Info,
            Ok("debug") => LogLevel::Debug,
            Ok("trace") => LogLevel::Trace,
            _ => LogLevel::Info,
        };

        Self { backend, log_level }
    }

    pub fn egl_lib_name(&self) -> &'static str {
        match self.backend {
            Backend::System => "libEGL.so",
            Backend::Angle => "libEGL_angle.so",
            // GLVND dispatch 库，通过 ICD JSON 路由到 libEGL_mesa.so.0
            Backend::Llvmpipe => "libEGL.so.1",
        }
    }

    pub fn gles_lib_name(&self) -> &'static str {
        match self.backend {
            Backend::System => "libGLESv3.so",
            Backend::Angle => "libGLESv2_angle.so",
            // Mesa GLES 2/3 库（实际提供 GLES 3.2）
            Backend::Llvmpipe => "libGLESv2.so.2",
        }
    }
}
