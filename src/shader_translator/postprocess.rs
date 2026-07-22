//! GLSL ES 后处理模块
//!
//! 对齐 MobileGlues 的后处理逻辑：
//! - stripVaryingLocations：移除 in/out varying 的 layout(location=N)
//! - removeLayoutBinding：移除所有 layout(binding=X)
//! - processOutColorLocations：为 outColorN 添加 layout(location=N)
//! - forceSupporterOutput：确保 precision highp float/int 声明

use regex::Regex;

/// GLSL ES 后处理主入口
///
/// 执行顺序：
/// 0. 移除 in/out varying 的 layout(location=N)（解决跨 stage mismatch）
/// 1. 移除所有 layout(binding=X)
/// 2. 处理 outColorN 的 location
/// 3. 确保 precision highp float/int 声明
pub fn post_process(src: &str) -> String {
    let mut result = src.to_string();

    // 0. 移除 in/out varying 的 layout(location=N)
    //    MC 桌面 GLSL 不声明 varying location（linker 按变量名匹配），
    //    但 preprocess 注入了 location（glslang OpenGL SPIR-V 模式 parse 要求），
    //    spirv-cross 保留它。不同 shader pair 中变量数量/顺序不同，
    //    导致同名 varying 跨 stage location 不一致（mismatch）。
    //    移除后 GLES linker 按变量名匹配，解决跨 stage 链接失败。
    //    VS attribute 的 location 也被移除——MC 通过 glGetAttribLocation
    //    动态获取 attribute 位置，不依赖硬编码 location。
    //    outColorN 的 location 由后续 processOutColorLocations 重新添加。
    result = strip_varying_locations(&result);

    // 1. 移除所有 layout(binding = X)（对齐 MobileGlues removeLayoutBinding）
    //    MobileGlues 不分类型，全部移除。需要处理以下形式：
    //    - layout(binding = X)                            → 移除整个 layout(...)
    //    - layout(binding = X, std140)                    → layout(std140)
    //    - layout(std140, binding = X)                    → layout(std140)
    //    - layout(std140, binding = X, column_major)      → layout(std140, column_major)
    //    - layout(push_constant, binding = X)             → layout(push_constant)
    //    策略：先处理 binding=X 在中间/末尾的情况，再处理 binding=X 是唯一项的情况
    let re_binding = Regex::new(r"(?i)layout\s*\(\s*binding\s*=\s*\d+\s*\)\s*").unwrap();
    let re_binding_leading = Regex::new(r"(?i)layout\s*\(\s*binding\s*=\s*\d+\s*,\s*").unwrap();
    let re_binding_middle = Regex::new(r"(?i),\s*binding\s*=\s*\d+").unwrap();

    // 先处理 binding 在中间的情况: layout(..., binding=X, ...) → layout(..., ...)
    result = re_binding_middle.replace_all(&result, "").to_string();
    // 再处理 binding 在开头的情况: layout(binding=X, ...) → layout(...)
    result = re_binding_leading
        .replace_all(&result, "layout(")
        .to_string();
    // 最后处理 binding 是唯一项的情况: layout(binding=X) → 移除
    result = re_binding.replace_all(&result, "").to_string();

    // 清理可能残留的空 layout 括号
    let re_empty_layout = Regex::new(r"(?i)layout\s*\(\s*\)\s*").unwrap();
    result = re_empty_layout.replace_all(&result, "").to_string();

    // 2. 处理 outColorN 的 location（对齐 MobileGlues processOutColorLocations）
    let re_out_color =
        Regex::new(r"(?m)^(out\s+(?:highp\s+|mediump\s+|lowp\s+)?\w+\s+outColor)(\d+)\s*;")
            .unwrap();
    result = re_out_color
        .replace_all(&result, "layout(location=$2) $1$2;")
        .to_string();

    // 3. 确保 precision 声明（对齐 MobileGlues forceSupporterOutput）
    result = ensure_precision(&result);

    result
}

/// 移除 in/out varying 声明前的 layout(location=N)
///
/// spirv-cross 输出的格式通常为 `layout(location = N) in/out <type> <name>;`，
/// location 是唯一限定符。处理三种情况：
/// - `layout(location = N) in/out` → `in/out`（location 唯一限定符）
/// - `layout(location = N, X) in/out` → `layout(X) in/out`（location 在开头）
/// - `layout(X, location = N) in/out` → `layout(X) in/out`（location 在末尾）
///
/// 不影响 uniform 的 layout(location)（正则要求 in/out 跟在 layout 后）。
fn strip_varying_locations(src: &str) -> String {
    // 情况1: layout(location = N) in/out → in/out（location 唯一限定符）
    let re_loc_only =
        Regex::new(r"(?i)layout\s*\(\s*location\s*=\s*\d+\s*\)\s+(in|out)\b").unwrap();
    let result = re_loc_only.replace_all(src, "$1").to_string();

    // 情况2: layout(location = N, X) in/out → layout(X) in/out
    let re_loc_leading =
        Regex::new(r"(?i)layout\s*\(\s*location\s*=\s*\d+\s*,\s*([^)]*)\)\s+(in|out)\b").unwrap();
    let result = re_loc_leading
        .replace_all(&result, "layout($1) $2")
        .to_string();

    // 情况3: layout(X, location = N) in/out → layout(X) in/out
    let re_loc_trailing =
        Regex::new(r"(?i)layout\s*\(\s*([^)]*?),\s*location\s*=\s*\d+\s*\)\s+(in|out)\b").unwrap();
    re_loc_trailing
        .replace_all(&result, "layout($1) $2")
        .to_string()
}

/// 确保 precision highp float/int 声明存在（对齐 MobileGlues forceSupporterOutput）
/// 始终强制使用 highp，移除所有已有 precision 声明后统一插入
fn ensure_precision(source: &str) -> String {
    let mut result = source.to_string();

    // 移除所有已有的 precision 声明（注释中的不受影响，因为 #version 之后不会有注释行）
    let re_precision = Regex::new(r"(?m)^\s*precision\s+\w+\s+(?:float|int)\s*;.*$(\n)?").unwrap();
    result = re_precision.replace_all(&result, "").to_string();

    let precision_decl = "precision highp float;\nprecision highp int;\n";

    // 在 #extension 之后或 #version 之后插入
    let last_ext = result.rfind("#extension");
    if let Some(pos) = last_ext
        .map(|p| result[p..].find('\n').map(|n| p + n + 1))
        .flatten()
    {
        result.insert_str(pos, precision_decl);
    } else if let Some(version_end) = result.find('\n') {
        result.insert_str(version_end + 1, precision_decl);
    } else {
        result.insert_str(0, precision_decl);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_varying_location_in() {
        let input = "layout(location = 0) in vec2 texCoord0;";
        let result = strip_varying_locations(input);
        assert_eq!(result, "in vec2 texCoord0;");
    }

    #[test]
    fn test_strip_varying_location_out() {
        let input = "layout(location = 1) out vec4 fragColor;";
        let result = strip_varying_locations(input);
        assert_eq!(result, "out vec4 fragColor;");
    }

    #[test]
    fn test_strip_varying_location_preserves_uniform() {
        // uniform 的 layout(location) 不应被移除
        let input = "layout(location = 0) uniform mat4 MVP;";
        let result = strip_varying_locations(input);
        assert!(result.contains("layout(location = 0) uniform mat4 MVP;"));
    }

    #[test]
    fn test_strip_varying_location_multiple() {
        let input =
            "layout(location = 0) in vec3 Position;\nlayout(location = 1) out vec4 vertexColor;\n";
        let result = strip_varying_locations(input);
        assert!(result.contains("in vec3 Position;"));
        assert!(result.contains("out vec4 vertexColor;"));
        assert!(!result.contains("layout(location"));
    }

    #[test]
    fn test_strip_varying_location_with_other_qualifier() {
        // layout(location = 0, std140) 的情况（罕见但需处理）
        let input = "layout(location = 0, std140) in vec2 texCoord0;";
        let result = strip_varying_locations(input);
        assert!(result.contains("layout(std140) in vec2 texCoord0;"));
    }

    #[test]
    fn test_remove_binding_only() {
        let input = "layout(binding = 0) uniform sampler2D tex;";
        let result = post_process(input);
        assert!(!result.contains("binding"));
    }

    #[test]
    fn test_remove_binding_leading() {
        let input = "layout(binding = 0, std140) uniform Block { mat4 m; };";
        let result = post_process(input);
        assert!(!result.contains("binding"));
        assert!(result.contains("layout(std140)"));
    }

    #[test]
    fn test_remove_binding_middle() {
        let input = "layout(std140, binding = 2, column_major) uniform Block { mat4 m; };";
        let result = post_process(input);
        assert!(!result.contains("binding"));
        assert!(result.contains("std140"));
        assert!(result.contains("column_major"));
    }

    #[test]
    fn test_out_color_location() {
        // outColorN 的 location 在 strip 之后由 processOutColorLocations 重新添加
        let input = "out vec4 outColor0;";
        let result = post_process(input);
        assert!(result.contains("layout(location=0) out vec4 outColor0;"));
    }

    #[test]
    fn test_post_process_strips_varying_locations() {
        // 端到端：spirv-cross 输出 → post_process → varying location 被移除
        let input = "#version 320 es\nlayout(location = 0) in vec2 texCoord0;\nlayout(location = 0) out vec4 fragColor;\nvoid main() { fragColor = vec4(texCoord0, 0.0, 1.0); }\n";
        let result = post_process(input);
        assert!(result.contains("in vec2 texCoord0;"));
        assert!(result.contains("out vec4 fragColor;"));
        // varying 的 location 应被移除
        assert!(!result.contains("layout(location = 0) in"));
        assert!(!result.contains("layout(location = 0) out"));
    }

    #[test]
    fn test_ensure_precision() {
        let input = "#version 320 es\nvoid main() {}\n";
        let result = post_process(input);
        assert!(result.contains("precision highp float;"));
        assert!(result.contains("precision highp int;"));
    }

    #[test]
    fn test_ensure_precision_replace_existing() {
        let input = "#version 320 es\nprecision mediump float;\nvoid main() {}\n";
        let result = post_process(input);
        assert!(result.contains("precision highp float;"));
        assert!(!result.contains("precision mediump float;"));
    }
}
