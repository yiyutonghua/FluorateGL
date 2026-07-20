use std::env;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Backend {
    System,
    Angle,
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
            _ => Backend::System,
        };

        let log_level = match env::var("FLUORATEGL_LOG").as_deref() {
            Ok("error") => LogLevel::Error,
            Ok("warn") => LogLevel::Warn,
            Ok("debug") => LogLevel::Debug,
            Ok("trace") => LogLevel::Trace,
            _ => LogLevel::Debug,
        };

        Self { backend, log_level }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试不设置环境变量时，默认走 System 后端。
    #[test]
    fn from_env_defaults_to_system_backend() {
        unsafe { std::env::remove_var("FLUORATEGL_BACKEND") };
        let cfg = Config::from_env();
        assert_eq!(cfg.backend, Backend::System);
    }

    /// 测试显式设置 angle 后端能被解析，未知值回退为 System。
    /// 合并到单个测试函数，避免并行运行时环境变量竞争。
    #[test]
    fn from_env_parses_backend_values() {
        unsafe {
            std::env::set_var("FLUORATEGL_BACKEND", "angle");
            assert_eq!(Config::from_env().backend, Backend::Angle);

            std::env::set_var("FLUORATEGL_BACKEND", "vulkan");
            assert_eq!(Config::from_env().backend, Backend::System);

            std::env::remove_var("FLUORATEGL_BACKEND");
            assert_eq!(Config::from_env().backend, Backend::System);
        }
    }

    /// 测试 log level 各档位解析与默认值。
    /// 合并到单个测试函数，避免并行运行时环境变量竞争。
    #[test]
    fn from_env_parses_log_levels() {
        unsafe {
            for (val, expected) in [
                ("error", LogLevel::Error),
                ("warn", LogLevel::Warn),
                ("debug", LogLevel::Debug),
                ("trace", LogLevel::Trace),
            ] {
                std::env::set_var("FLUORATEGL_LOG", val);
                assert_eq!(
                    Config::from_env().log_level,
                    expected,
                    "failed for FLUORATEGL_LOG={}",
                    val
                );
            }
            // 未知值回退为 Debug
            std::env::set_var("FLUORATEGL_LOG", "verbose");
            assert_eq!(Config::from_env().log_level, LogLevel::Debug);

            // 未设置时默认 Debug
            std::env::remove_var("FLUORATEGL_LOG");
            assert_eq!(Config::from_env().log_level, LogLevel::Debug);
        }
    }

    /// 测试 libEGL / libGLES 库名按后端正确切换。
    #[test]
    fn lib_names_match_backend() {
        let sys = Config { backend: Backend::System, log_level: LogLevel::Info };
        let angle = Config { backend: Backend::Angle, log_level: LogLevel::Info };
        assert_eq!(sys.egl_lib_name(), "libEGL.so");
        assert_eq!(sys.gles_lib_name(), "libGLESv3.so");
        assert_eq!(angle.egl_lib_name(), "libEGL_angle.so");
        assert_eq!(angle.gles_lib_name(), "libGLESv2_angle.so");
    }
}
