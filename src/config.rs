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
    /// 跳过 EGL/GLES 库加载（用于 fork worker 等只需翻译管线的纯 CPU 场景）
    pub skip_backend: bool,
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

        let skip_backend = env::var("FLUORATEGL_SKIP_BACKEND")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        Self {
            backend,
            log_level,
            skip_backend,
        }
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

// ===== 报告给宿主的 GL/EGL 版本信息 =====
//
// 这些常量是 FluorateGL 对外伪装的桌面 OpenGL 版本。MC 会根据这些值判断可用的
// GL 特性与渲染路径。三者必须保持一致：
//   REPORTED_GL_VERSION_PREFIX 的 "主.次" 必须等于 REPORTED_GL_MAJOR.REPORTED_GL_MINOR
// 集中定义避免散落在多处因改动遗漏导致不一致。

/// 报告的 GL 版本前缀（glGetString(GL_VERSION) 返回 "3.3.0 FluorateGL v{ver}"）
///
/// 选择 3.3 而非 3.2 的原因：LWJGL 在 GL 3.3 下才会把 sampler objects、
/// blend_func_extended 等作为 core 函数加载函数指针。3.2 下这些函数即使有
/// 对应扩展声明，LWJGL 也只在版本 >= 3.3 时查找 core 指针，导致 Sodium
/// 创建 program 时调用这些函数触发 "No context is current" 错误并降级渲染。
/// MobileGL 报告 3.3 即可运行 Sodium/Voxy/Distant Horizons/光影，印证 3.3 足够。
pub const REPORTED_GL_VERSION_PREFIX: &str = "3.3.0 FluorateGL";

// 编译期断言：版本字符串前缀 "主.次" 必须与 MAJOR/MINOR 常量一致，
// 防止改动一处遗漏另一处导致 MC 版本解析异常。
const _: () = {
    let prefix = REPORTED_GL_VERSION_PREFIX.as_bytes();
    let major_digit = prefix[0];
    let minor_digit = prefix[2];
    assert!(major_digit == b'0' + REPORTED_GL_MAJOR as u8);
    assert!(minor_digit == b'0' + REPORTED_GL_MINOR as u8);
};
/// 报告的 GL 主版本号（glGetIntegerv(GL_MAJOR_VERSION)）
pub const REPORTED_GL_MAJOR: i32 = 3;
/// 报告的 GL 次版本号（glGetIntegerv(GL_MINOR_VERSION)）
pub const REPORTED_GL_MINOR: i32 = 3;
/// 报告的 GLSL 版本字符串（glGetString(GL_SHADING_LANGUAGE_VERSION)）
/// GL 3.3 对应 GLSL 3.30（桌面 GLSL 版本号从 1.40 跳到 3.30）
pub const REPORTED_GLSL_VERSION: &str = "3.30";

/// 报告的 EGL 主版本号
pub const REPORTED_EGL_MAJOR: i32 = 1;
/// 报告的 EGL 次版本号
pub const REPORTED_EGL_MINOR: i32 = 4;
