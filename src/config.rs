use std::env;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Backend {
    System,
    Angle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
            _ => Backend::System,
        };

        let log_level = match env::var("FLUORATEGL_LOG").as_deref() {
            Ok("error") => LogLevel::Error,
            Ok("warn") => LogLevel::Warn,
            Ok("debug") => LogLevel::Debug,
            Ok("trace") => LogLevel::Trace,
            _ => LogLevel::Info,
        };

        Self {
            backend,
            log_level,
        }
    }

    pub fn egl_lib_name(&self) -> &'static str {
        match self.backend {
            Backend::System => "libEGL.so",
            Backend::Angle => "libEGL_angle.so",
        }
    }

    pub fn gles_lib_name(&self) -> &'static str {
        match self.backend {
            Backend::System => "libGLESv3.so",
            Backend::Angle => "libGLESv2_angle.so",
        }
    }
}
