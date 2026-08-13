//! GLES 后端能力检测
//!
//! 通过 `glGetString(GL_VERSION)` 和 `glGetStringi(GL_EXTENSIONS, i)` 查询**真实** GLES
//! 版本与扩展（绕过拦截层对 MC 伪造的 FAKE_EXTENSIONS），构建权威能力表。
//!
//! 拦截层（drawing.rs / multi_draw.rs）基于此表决定：
//! - 原生转发（扩展支持）
//! - 模拟降级（扩展不支持但可用其他函数组合实现）
//! - 告警跳过（无法模拟）
//!
//! `is_stub`（函数指针层面）作为兜底：即使扩展声明支持，若 `load_opt_suffixes!`
//! 未加载到符号（驱动声明扩展但未导出函数），仍走模拟。

use crate::backend::dispatch::GlesDispatch;
use std::sync::Mutex;
use std::sync::OnceLock;

/// GLES 扩展列表缓存（query() 时填充）。
///
/// 供 enable 表（gl/exports.rs enable_state）的 ext_backing_present 等按需查询：
/// 判断驱动是否真实支持某扩展（GL_EXT_depth_clamp / GL_EXT_sRGB_write_control /
/// GL_EXT_multisample_compatibility / GL_NV_polygon_mode / GL_OES_sample_shading /
/// GL_EXT_clip_cull_distance），决定 enable 类 cap 是转发驱动还是仅记录。
/// 对齐 MobileGlues getter.cpp 的硬件 caps 语义（MG 在 init 时遍历扩展字符串）。
static EXTENSION_CACHE: OnceLock<Mutex<Option<Vec<String>>>> = OnceLock::new();

/// 缓存扩展列表（query() 构建能力表时调用，OnceLock 幂等）。
fn cache_extensions(exts: &[String]) {
    let cache = EXTENSION_CACHE.get_or_init(|| Mutex::new(None));
    *cache.lock().unwrap_or_else(|p| p.into_inner()) = Some(exts.to_vec());
}

/// 驱动是否声明了指定扩展（扩展缓存未就绪时返回 false）。
///
/// 未就绪场景：GLES_DISPATCH 未设置（纯 stub / 离线测试），或首次 GL 调用
/// 前 enable 类查询早到。此时按"无扩展"处理（只记录不转发），与 MG 在
/// caps 填充前查不到扩展的行为一致；首次 with_gles_dispatch 触发能力查询后
/// 缓存定型，后续调用得到真实答案。
pub(crate) fn has_extension(name: &str) -> bool {
    let Some(cache) = EXTENSION_CACHE.get() else {
        return false;
    };
    cache
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .map(|exts| exts.iter().any(|e| e == name))
        .unwrap_or(false)
}

/// GLES 版本号（major * 10 + minor），如 3.2 → 32
#[derive(Clone, Copy, Debug)]
pub struct GlesVersion(pub u16);

impl GlesVersion {
    /// 是否 >= 指定版本
    pub fn at_least(self, major: u8, minor: u8) -> bool {
        self.0 >= (major as u16 * 10 + minor as u16)
    }
}

/// GLES 真实能力表
///
/// 各字段对应拦截层用到的扩展/版本特性，基于扩展字符串查询。
#[derive(Clone, Debug)]
pub struct GlesCapabilities {
    /// GLES 版本（如 30/31/32），主要用于诊断日志
    #[allow(dead_code)]
    pub version: GlesVersion,
    /// GL_OES_draw_elements_base_vertex 或 GLES 3.2
    /// 覆盖：glDrawElementsBaseVertex, glDrawRangeElementsBaseVertex,
    ///       glDrawElementsInstancedBaseVertex
    pub draw_elements_base_vertex: bool,
    /// GL_EXT_base_instance 或 GLES 3.2
    /// 覆盖：glDrawArraysInstancedBaseInstance, glDrawElementsInstancedBaseInstance,
    ///       glDrawElementsInstancedBaseVertexBaseInstance（还需 base_vertex）
    pub base_instance: bool,
    /// GL_EXT_multi_draw_elements_base_vertex 或 GLES 3.2
    /// 覆盖：glMultiDrawElementsBaseVertex
    pub multi_draw_elements_base_vertex: bool,
    /// GL_EXT_multi_draw_indirect 或 GLES 3.2
    /// 覆盖：glMultiDrawArraysIndirect, glMultiDrawElementsIndirect
    pub multi_draw_indirect: bool,
    /// GLES 3.1+（core，无扩展）
    /// 覆盖：glMultiDrawArrays, glMultiDrawElements
    pub multi_draw: bool,
    /// GLES 3.1+（core，无扩展）
    /// 覆盖：glDrawArraysIndirect, glDrawElementsIndirect
    pub indirect_draw: bool,
    /// GL_ARB_indirect_compute / GL 4.6（GLES 几乎无支持）
    /// 覆盖：glMultiDrawArraysIndirectCount, glMultiDrawElementsIndirectCount
    pub indirect_count: bool,
    /// GL_EXT_texture_query_lod（GLES 扩展）
    /// 覆盖：textureQueryLod 的硬件支持——支持时 shader 翻译可跳过 polyfill 注入
    /// （注意：Mesa llvmpipe 声明该扩展但 GLSL 编译器未实现——扩展字符串
    /// 声明不可靠，preprocess 有 FLUORATEGL_FORCE_TQL_POLYFILL 逃生门）
    pub texture_query_lod: bool,
    /// GL_EXT_buffer_storage（GLES 扩展，对应桌面 GL_ARB_buffer_storage）
    /// 覆盖：glBufferStorage 的驱动原生支持——支持时 buffer 域（MG 式）透传
    /// 持久存储语义；不支持时走降级（glBufferData 模拟）。FAKE_EXTENSIONS
    /// 的 GL_ARB_buffer_storage 声明以本字段为准（caps=true 才声明，与
    /// build_fake_extensions 的既有 behavior_dependent 模式一致）
    pub buffer_storage: bool,
}

impl GlesCapabilities {
    /// 全 false 兜底（当前仅测试使用；生产兜底见 backend/mod.rs 的 FALLBACK_CAPS）
    #[allow(dead_code)]
    pub fn none() -> Self {
        Self {
            version: GlesVersion(0),
            draw_elements_base_vertex: false,
            base_instance: false,
            multi_draw_elements_base_vertex: false,
            multi_draw_indirect: false,
            multi_draw: false,
            indirect_draw: false,
            indirect_count: false,
            texture_query_lod: false,
            buffer_storage: false,
        }
    }

    /// 查询真实 GLES 版本与扩展，构建能力表。
    ///
    /// 必须在 EGL 上下文已创建后调用（首次 GL 调用时由 backend/mod.rs 触发）。
    /// 通过 dispatch 直接访问 GLES，绕过拦截层的伪造扩展。
    pub fn query(dispatch: &GlesDispatch) -> Self {
        let version = parse_gles_version(dispatch);
        let extensions = query_extensions(dispatch);
        cache_extensions(&extensions);

        let is_32 = version.at_least(3, 2);

        // 项目仅支持 GLES 3.1+，indirect draw（glDrawArraysIndirect / glDrawElementsIndirect）
        // 是 3.1 core 特性，恒可用，无需查询。若检测到 < 3.1 则该设备不在支持范围内。
        if !version.at_least(3, 1) {
            log::error!(
                "[FluorateGL] 检测到 GLES {}，本项目需要 GLES 3.1+，部分功能将无法正常工作",
                version.0
            );
        }

        let caps = Self {
            version,
            draw_elements_base_vertex: is_32
                || extensions
                    .iter()
                    .any(|e| e == "GL_OES_draw_elements_base_vertex"),
            base_instance: is_32 || extensions.iter().any(|e| e == "GL_EXT_base_instance"),
            multi_draw_elements_base_vertex: is_32
                || extensions
                    .iter()
                    .any(|e| e == "GL_EXT_multi_draw_elements_base_vertex"),
            multi_draw_indirect: is_32
                || extensions.iter().any(|e| e == "GL_EXT_multi_draw_indirect"),
            // GLES 3.1 core 特性，项目前提，恒为 true
            multi_draw: true,
            // GLES 3.1 core 特性，项目前提，恒为 true
            indirect_draw: true,
            // GLES 无标准 indirect count 扩展
            indirect_count: false,
            texture_query_lod: extensions.iter().any(|e| e == "GL_EXT_texture_query_lod"),
            // GL_EXT_buffer_storage（GLES 3.1+ 扩展，桌面对应 GL_ARB_buffer_storage）
            buffer_storage: extensions.iter().any(|e| e == "GL_EXT_buffer_storage"),
        };

        log::info!(
            "[FluorateGL] GLES 能力检测: version={} base_vertex={} base_instance={} multi_base_vertex={} multi_indirect={} multi_draw={} indirect_draw={} indirect_count={} texture_query_lod={} buffer_storage={}",
            version.0,
            caps.draw_elements_base_vertex,
            caps.base_instance,
            caps.multi_draw_elements_base_vertex,
            caps.multi_draw_indirect,
            caps.multi_draw,
            caps.indirect_draw,
            caps.indirect_count,
            caps.texture_query_lod,
            caps.buffer_storage
        );
        if !caps.draw_elements_base_vertex {
            log::warn!(
                "[FluorateGL] GLES 不支持 GL_OES_draw_elements_base_vertex，BaseVertex 系列将降级模拟（索引偏移丢失）"
            );
        }
        if !caps.base_instance {
            log::warn!(
                "[FluorateGL] GLES 不支持 GL_EXT_base_instance，BaseInstance 系列将降级模拟（baseinstance 丢失）"
            );
        }

        caps
    }
}

/// 解析 GLES 版本号
///
/// GLES 的 GL_VERSION 字符串格式如 "OpenGL ES 3.2 V@0415.0..."，
/// 解析 "3.2" 部分得到 320。
///
/// 对照 MG getter.cpp set_es_version()：MG 取前三个空格前的 token 后
/// sscanf "OpenGL ES %d.%d"（失败兜底 300）；本实现按 "OpenGL ES" 关键字
/// 分割后取首个 "主.次" token，语义等价且对 vendor 后缀更宽容。
///
/// C1 兜底：部分后端（如 ANGLE）在能力查询时机上下文未完全 current 时
/// glGetString(GL_VERSION) 返回 null，字符串解析失败 → 用
/// glGetIntegerv(GL_MAJOR_VERSION/GL_MINOR_VERSION)（GLES 3.0+ core 查询）
/// 兜底。
///
/// P3：两者都失败时兜底返回 310（项目前提 GLES 3.1+，对齐 MobileGlues
/// getter.cpp:219-232 的兜底策略——它兜底 300，我们 310 更贴前提）。
/// 拦截层以 dispatch 符号存在性为主导（C1），兜底版本仅影响
/// version.at_least 类判定（如 GL_DEPTH_CLAMP 的 3.2 感知过滤），
/// 310 兜底保证 GLES 3.1+ 设备不被误判为 3.0。
fn parse_gles_version(dispatch: &GlesDispatch) -> GlesVersion {
    // GL_VERSION = 0x1F02
    let version_ptr = unsafe { (dispatch.get_string)(0x1F02) };
    let v = if !version_ptr.is_null() {
        let version_str = unsafe {
            std::ffi::CStr::from_ptr(version_ptr)
                .to_string_lossy()
                .into_owned()
        };

        // 查找 "OpenGL ES N.M" 模式
        version_str
            .split("OpenGL ES")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| {
                let mut parts = s.split('.');
                let major = parts.next()?.parse::<u16>().ok()?;
                let minor = parts
                    .next()
                    .and_then(|m| m.parse::<u16>().ok())
                    .unwrap_or(0);
                Some(major * 10 + minor)
            })
            .unwrap_or(0)
    } else {
        0
    };

    if v != 0 {
        return GlesVersion(v);
    }

    // 兜底：glGetIntegerv(GL_MAJOR_VERSION/GL_MINOR_VERSION)
    // GL_MAJOR_VERSION = 0x821B, GL_MINOR_VERSION = 0x821C
    let mut major = 0i32;
    let mut minor = 0i32;
    unsafe {
        (dispatch.get_integerv)(0x821B, &mut major);
        (dispatch.get_integerv)(0x821C, &mut minor);
    }
    if major > 0 && minor >= 0 {
        log::debug!(
            "[FluorateGL] GL_VERSION 字符串不可用，glGetIntegerv 兜底: {}.{}",
            major,
            minor
        );
        return GlesVersion(major as u16 * 10 + minor as u16);
    }

    // P3：全部失败 → 兜底 310（项目前提 GLES 3.1+，避免误判为 3.0 导致
    // GL_DEPTH_CLAMP 等 3.2 特性判定与透传行为不一致）
    log::warn!("[FluorateGL] GL_VERSION 与 glGetIntegerv 均不可用，version 兜底为 3.1（项目前提）");
    GlesVersion(31)
}

/// 通过 glGetStringi 遍历 GLES 扩展列表
///
/// GLES 3.0+ 必须用 indexed 查询（glGetString(GL_EXTENSIONS) 可能返回超长字符串或 null）。
fn query_extensions(dispatch: &GlesDispatch) -> Vec<String> {
    // GL_NUM_EXTENSIONS = 0x821D
    let mut num = 0i32;
    unsafe {
        (dispatch.get_integerv)(0x821D, &mut num);
    }
    if num <= 0 {
        return Vec::new();
    }

    // GL_EXTENSIONS = 0x1F03
    let mut exts = Vec::with_capacity(num as usize);
    for i in 0..num as u32 {
        let ptr = unsafe { (dispatch.get_string_i)(0x1F03, i) };
        if !ptr.is_null()
            && let Ok(s) = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str()
        {
            exts.push(s.to_string());
        }
    }
    exts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_at_least() {
        let v = GlesVersion(32); // 3.2
        assert!(v.at_least(3, 2));
        assert!(v.at_least(3, 1));
        assert!(v.at_least(3, 0));
        assert!(!v.at_least(4, 0));

        let v = GlesVersion(31); // 3.1
        assert!(!v.at_least(3, 2));
        assert!(v.at_least(3, 1));
    }

    #[test]
    fn none_caps_all_false() {
        let c = GlesCapabilities::none();
        assert!(!c.draw_elements_base_vertex);
        assert!(!c.base_instance);
        assert!(!c.indirect_draw);
        assert!(!c.indirect_count);
        assert!(!c.buffer_storage);
    }
}
