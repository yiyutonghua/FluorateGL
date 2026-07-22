//! GLSL 源码预处理模块
//!
//! 本分支（glslang-targetvk）仅做最小预处理：移除 #line 指令。
//! GLSL 源码直接交给 glslang 编译为 SPIR-V，再由 spirv-cross 转为 GLSL ES。
//! location/binding 由 glslang 的 ShaderOptions（AUTO_MAP_LOCATIONS /
//! AUTO_MAP_BINDINGS）自动分配，无需预处理注入。

use regex::Regex;

/// GLSL 预处理主入口
///
/// 仅移除 #line 指令，其余处理交给 glslang。
pub fn preprocess(source: &str) -> String {
    remove_line_directives(source)
}

/// 提取 GLSL 源码中的 #version 行
pub fn extract_version(source: &str) -> Option<&str> {
    source
        .lines()
        .find(|l| l.trim_start().starts_with("#version"))
}

/// 移除 #line 指令
fn remove_line_directives(source: &str) -> String {
    let re = Regex::new(r"(?m)^\s*#line\s+.*$(\n|$)?").unwrap();
    re.replace_all(source, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_line_directives() {
        let input = "#version 330\n#line 0 2\nvoid main() {}\n";
        let result = remove_line_directives(input);
        assert!(!result.contains("#line"));
    }

    #[test]
    fn test_preprocess_only_removes_line() {
        // preprocess 仅移除 #line，其余原样保留
        let input = "#version 150\n\
            #line 0 2\n\
            uniform mat4 ModelViewMat;\n\
            in vec3 Position;\n\
            void main() {\n\
                gl_Position = ModelViewMat * vec4(Position, 1.0);\n\
            }\n";
        let result = preprocess(input);
        assert!(!result.contains("#line"));
        // 版本和声明原样保留
        assert!(result.contains("#version 150"));
        assert!(result.contains("uniform mat4 ModelViewMat;"));
        assert!(result.contains("in vec3 Position;"));
    }
}
