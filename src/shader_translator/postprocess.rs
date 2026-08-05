//! GLSL ES 后处理模块
//!
//! 按目标 GLES 版本条件执行（OpenGL target 重构后，spirv-cross 输出原生带
//! location/binding，不同 ES 版本的语法支持不同）：
//!
//! | 版本 | in/out location | uniform location | binding |
//! |------|-----------------|------------------|---------|
//! | 320  | 保留（ES 3.2 合法） | 保留（ES 3.1+ 合法） | 保留（ES 3.1+ 合法） |
//! | 310  | strip（跨 stage 保守） | strip（保守） | 保留（ES 3.1 合法） |
//! | 300  | strip（ES 3.0 varying 不支持 location） | strip（ES 3.0 不支持 uniform location，spike_g 实测） | 移除（ES 3.0 不支持 binding） |
//!
//! 无条件执行：
//! - strip_uniform_binding：glslang OpenGL target 无条件给 standalone uniform
//!   分配 binding decoration（spirv-cross 输出 `layout(location = 0, binding = 0)
//!   uniform mat4 MVP;`），但 GLES 中 binding 仅对 block/sampler/image/atomic
//!   合法（实测报 "binding requires block, or sampler/image, or atomic-counter
//!   type"），必须从 standalone uniform 上剥离（所有 ES 版本）
//! - strip_ubo_instance_name：spirv-cross 输出 `} _20;` 实例名（spike_b 实测必留）
//! - fix_atomic_counter_binding：GLES 要求 atomic counter 用 binding（offset 无效）
//! - inject_image_format：GLES 要求 image 必须有 format 和 binding
//! - outColorN location 重注：MC framebuffer 约定 outColorN → color attachment N

use regex::Regex;
use std::sync::OnceLock;

/// GLSL ES 后处理主入口
///
/// `version` 为目标的 GLES 版本（320/310/300），决定条件 strip 策略
/// （见模块文档表格）。
pub fn post_process(src: &str, version: u16) -> String {
    let mut result = src.to_string();

    // 0. 移除 in/out varying 的 layout(location=N)（310/300 回退时）
    //    320 保留（ES 3.2 中 in/out location 合法，spike 实测 320 产物原生带
    //    location）。310/300 保守 strip：ES 3.0 的 varying 不支持 location，
    //    且 strip 后 GLES linker 按变量名匹配，规避跨 stage 计数不一致风险。
    //    VS attribute 的 location 也被移除——MC 通过 glGetAttribLocation
    //    动态获取 attribute 位置，不依赖硬编码 location。
    //    outColorN 的 location 由后续 fix_out_color_locations 重新添加。
    if version < 320 {
        result = strip_varying_locations(&result);

        // 0.5 移除 standalone uniform 的 layout(location=N)（310/300 回退时）
        //     GLSL ES 3.00 不支持 uniform location（spike_g 实测 300 es 输出
        //     带 location 会编译失败）；310 保守 strip（MC 通过
        //     glGetUniformLocation 动态查询，无需显式 location）。
        //     uniform block（含 `{`，用 binding 不用 location）不受影响。
        result = strip_uniform_locations(&result);
    }

    // 1. 移除 layout(binding=X)（仅 300 es：ES 3.0 不支持 binding 限定符；
    //    ES 3.1+ 合法，320/310 保留）。注意：image 的 binding 不能移除！
    //    image 必须通过 layout(binding=N) 与 glBindImageTexture(unit,...) 对应。
    if version < 310 {
        result = strip_bindings(&result);
    }

    // 1.3 无条件移除 standalone uniform 上的 binding（所有 ES 版本）
    //     glslang OpenGL target 给 standalone uniform 分配 binding decoration，
    //     spirv-cross 输出 `layout(location = 0, binding = 0) uniform mat4 MVP;`，
    //     GLES 中 binding 仅对 block/sampler/image/atomic 合法 → 必须剥离。
    //     sampler/image/block 行的 binding 不受影响（按版本条件在上方处理）。
    result = strip_uniform_binding(&result);

    // 1.5 移除 UBO/SSBO 实例名（关键修复：解决 uniform 查询失败）
    //    spirv-cross 默认为 UBO/SSBO 添加实例名（如 `} _20;`），导致成员访问
    //    需要通过 `实例名.成员名`（如 `_20.ModelViewMat`）。MC 通过
    //    glGetUniformLocation(program, "ModelViewMat") 按成员名查询时返回 -1，
    //    因为带实例名的 UBO 成员必须用 "块名.成员名" 查询，直接按成员名查不到。
    //    移除实例名后，函数体变为直接成员访问。
    result = strip_ubo_instance_name(&result);

    // 2. 修复 atomic counter binding
    //    spirv-cross 输出 `layout(offset = N) uniform atomic_uint`，
    //    但 GLES 要求 atomic counter 必须用 `layout(binding = N)` 指定绑定点。
    //    步骤1 的 binding 移除不影响 offset 限定符，这里单独修复。
    result = fix_atomic_counter_binding(&result);

    // 3. 注入 image format 限定符
    //    GLES 要求 image uniform 必须同时有 binding 和 format。
    //    - 无 layout 前缀的 image：注入 binding + format
    //    - 有 layout(binding=N) 但无 format 的 image：补充 format
    //    writeonly image 默认 r32f，可读写 image 默认 r32ui。
    result = inject_image_format(&result);

    // 4. 处理 outColorN 的 location（对齐 MobileGlues processOutColorLocations）
    //    先剥离 spirv-cross 可能输出的 layout(...) 前缀，再注入 location=N
    //    （320 路径保留 location 时同样需要重注：MC framebuffer 约定
    //    outColorN 必须在 color attachment N）。
    result = fix_out_color_locations(&result);

    result
}

/// 剥离所有非 image 行的 layout(binding=X)（仅 300 es 使用）
///
/// 按行处理，跳过 image 声明行（保留 image 的 binding）。
fn strip_bindings(src: &str) -> String {
    let re_is_image = {
        static RE_IS_IMAGE: OnceLock<Regex> = OnceLock::new();
        RE_IS_IMAGE.get_or_init(|| Regex::new(r"(?i)\bimage\w*\s+\w+\s*;").unwrap())
    };
    let re_binding = {
        static RE_BINDING: OnceLock<Regex> = OnceLock::new();
        RE_BINDING
            .get_or_init(|| Regex::new(r"(?i)layout\s*\(\s*binding\s*=\s*\d+\s*\)\s*").unwrap())
    };
    let re_binding_leading = {
        static RE_BINDING_LEADING: OnceLock<Regex> = OnceLock::new();
        RE_BINDING_LEADING
            .get_or_init(|| Regex::new(r"(?i)layout\s*\(\s*binding\s*=\s*\d+\s*,\s*").unwrap())
    };
    let re_binding_middle = {
        static RE_BINDING_MIDDLE: OnceLock<Regex> = OnceLock::new();
        RE_BINDING_MIDDLE.get_or_init(|| Regex::new(r"(?i),\s*binding\s*=\s*\d+").unwrap())
    };
    let re_empty_layout = {
        static RE_EMPTY_LAYOUT: OnceLock<Regex> = OnceLock::new();
        RE_EMPTY_LAYOUT.get_or_init(|| Regex::new(r"(?i)layout\s*\(\s*\)\s*").unwrap())
    };

    let mut result = src
        .lines()
        .map(|line| {
            // 跳过 image 声明行（保留 image 的 binding）
            if re_is_image.is_match(line) {
                return line.to_string();
            }
            // 非 image 行：移除 binding
            let l = re_binding_middle.replace_all(line, "");
            let l = re_binding_leading.replace_all(&l, "layout(");
            let l = re_binding.replace_all(&l, "");
            let l = re_empty_layout.replace_all(&l, "");
            l.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");

    // 如果原文件以换行结尾，保留它
    if src.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// 处理 outColorN 的 location（对齐 MobileGlues processOutColorLocations）
///
/// 分两步：
/// 1. 剥离 outColorN 声明行上的 layout(...) 前缀（spirv-cross 在 320 输出
///    时原生带 location，需先剥离才能注入正确值）
/// 2. 注入 layout(location=N)（MC framebuffer 约定 outColorN → attachment N）
fn fix_out_color_locations(src: &str) -> String {
    // 第一步：剥离 outColorN 行上的 layout(...) 前缀
    static RE_OUT_COLOR_LAYOUT: OnceLock<Regex> = OnceLock::new();
    let re_out_color_layout = RE_OUT_COLOR_LAYOUT.get_or_init(|| {
        Regex::new(
            r"(?m)^(\s*)layout\s*\([^)]*\)\s*((?:(?:flat|smooth|noperspective|centroid|invariant)\s+)*out\s+(?:(?:highp\s+|mediump\s+|lowp\s+)?\w+)\s+outColor\d+\s*;)",
        )
        .unwrap()
    });
    let result = re_out_color_layout.replace_all(src, "$1$2").to_string();

    // 第二步：注入 location（正则容忍前导插值修饰符）
    static RE_OUT_COLOR: OnceLock<Regex> = OnceLock::new();
    let re_out_color = RE_OUT_COLOR.get_or_init(|| {
        Regex::new(
            r"(?m)^(?P<prefix>(?:(?:flat|smooth|noperspective|centroid|invariant)\s+)*)out\s+(?P<type>(?:highp\s+|mediump\s+|lowp\s+)?\w+)\s+outColor(?P<num>\d+)\s*;",
        )
        .unwrap()
    });
    re_out_color
        .replace_all(&result, |caps: &regex::Captures| {
            let prefix = caps.name("prefix").map(|m| m.as_str()).unwrap_or("");
            let typ = caps.name("type").map(|m| m.as_str()).unwrap_or("");
            let num = caps.name("num").map(|m| m.as_str()).unwrap_or("0");
            format!(
                "layout(location={}) {}out {} outColor{};",
                num, prefix, typ, num
            )
        })
        .to_string()
}

/// 移除 in/out varying 声明前的 layout(location=N)
///
/// spirv-cross 输出的格式可能为：
/// - `layout(location = N) in/out <type> <name>;`
/// - `layout(location = N) flat out <type> <name>;`（带插值修饰符）
/// - `layout(location = N, X) in/out <type> <name>;`
/// - `layout(X, location = N) in/out <type> <name>;`
///
/// 改进策略：
/// 1. 增强正则匹配的健壮性，处理更多边缘情况
/// 2. 确保所有形式的 location 限定符都被正确移除
/// 3. 保留插值修饰符和其他布局限定符
fn strip_varying_locations(src: &str) -> String {
    // 插值修饰符前缀（可重复，如 invariant flat out）
    let interp = r"(?:(?:flat|smooth|noperspective|centroid|patch|invariant)\s+)*";

    // 情况1: layout(location = N) [修饰符] in/out → [修饰符] in/out
    static RE_LOC_ONLY: OnceLock<Regex> = OnceLock::new();
    let re_loc_only = RE_LOC_ONLY.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)layout\s*\(\s*location\s*=\s*\d+\s*\)\s*({})(in|out)\b",
            interp
        ))
        .unwrap()
    });
    let result = re_loc_only.replace_all(src, "$1$2").to_string();

    // 情况2: layout(location = N, X) [修饰符] in/out → layout(X) [修饰符] in/out
    static RE_LOC_LEADING: OnceLock<Regex> = OnceLock::new();
    let re_loc_leading = RE_LOC_LEADING.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)layout\s*\(\s*location\s*=\s*\d+\s*,\s*([^)]*)\)\s*({})(in|out)\b",
            interp
        ))
        .unwrap()
    });
    let result = re_loc_leading
        .replace_all(&result, "layout($1) $2$3")
        .to_string();

    // 情况3: layout(X, location = N) [修饰符] in/out → layout(X) [修饰符] in/out
    static RE_LOC_TRAILING: OnceLock<Regex> = OnceLock::new();
    let re_loc_trailing = RE_LOC_TRAILING.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)layout\s*\(\s*([^)]*?),\s*location\s*=\s*\d+\s*\)\s*({})(in|out)\b",
            interp
        ))
        .unwrap()
    });
    let result = re_loc_trailing
        .replace_all(&result, "layout($1) $2$3")
        .to_string();

    // 情况4: layout(location = N) [修饰符] in/out [数组声明] → [修饰符] in/out [数组声明]
    // 处理数组声明的情况，确保 location 被正确移除
    static RE_LOC_ARRAY: OnceLock<Regex> = OnceLock::new();
    let re_loc_array = RE_LOC_ARRAY.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)layout\s*\(\s*location\s*=\s*\d+\s*\)\s*({})(in|out)\s+(?P<rest>.+?)(\[.*\])\s*;",
            interp
        ))
        .unwrap()
    });
    re_loc_array.replace_all(&result, "$1$2 $3$4;").to_string()
}

/// 移除 standalone uniform 声明前的 layout(location=N)
///
/// preprocess 为 non-opaque uniform 注入 location（每 shader 独立从 0 计数），
/// 跨 stage 链接时不同名 uniform 会撞 location。GLES 中 standalone uniform 无需显式
/// location（MC 通过 glGetUniformLocation 动态查询），移除安全。
///
/// 仅处理 standalone uniform（无 `{`）。uniform block（含 `{`，用 binding 不用 location）
/// 不受影响。处理三种形式：
/// - `layout(location = N) uniform ...` → `uniform ...`
/// - `layout(location = N, X) uniform ...` → `layout(X) uniform ...`
/// - `layout(X, location = N) uniform ...` → `layout(X) uniform ...`
fn strip_uniform_locations(src: &str) -> String {
    let re_loc_only = {
        static RE_LOC_ONLY: OnceLock<Regex> = OnceLock::new();
        RE_LOC_ONLY.get_or_init(|| {
            Regex::new(r"(?i)layout\s*\(\s*location\s*=\s*\d+\s*\)\s*(uniform\b)").unwrap()
        })
    };
    let re_loc_leading = {
        static RE_LOC_LEADING: OnceLock<Regex> = OnceLock::new();
        RE_LOC_LEADING
            .get_or_init(|| Regex::new(r"(?i)layout\s*\(\s*location\s*=\s*\d+\s*,\s*").unwrap())
    };
    let re_loc_trailing = {
        static RE_LOC_TRAILING: OnceLock<Regex> = OnceLock::new();
        RE_LOC_TRAILING.get_or_init(|| Regex::new(r"(?i),\s*location\s*=\s*\d+").unwrap())
    };

    src.lines()
        .map(|line| {
            // 仅处理 standalone uniform 声明行（含 uniform 且不含 block 的 `{`）
            if !line.contains("uniform") || line.contains('{') {
                return line.to_string();
            }
            let l = re_loc_only.replace_all(line, "$1").to_string();
            let l = re_loc_leading.replace_all(&l, "layout(").to_string();
            let l = re_loc_trailing.replace_all(&l, "").to_string();
            l.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 无条件移除 standalone uniform（non-opaque、非 block）上的 layout(binding=X)
///
/// glslang OpenGL target 给 standalone uniform 分配 binding decoration
/// （spirv-cross 输出 `layout(location = 0, binding = 0) uniform mat4 MVP;`），
/// 但 GLES 中 binding 仅对 block/sampler/image/atomic 合法（实测报
/// "binding requires block, or sampler/image, or atomic-counter type"）。
/// 所有 ES 版本都必须剥离。
///
/// 仅处理 standalone non-opaque uniform 行：
/// - 跳过 block（含 `{`）——block 用 binding，且 320/310 合法保留
/// - 跳过 opaque（sampler/image/atomic_uint）——binding 合法保留
/// - 移除 `binding = N`（三种位置：唯一项/前导/中间末尾），清理空 layout()
fn strip_uniform_binding(src: &str) -> String {
    let re_binding = {
        static RE_BINDING: OnceLock<Regex> = OnceLock::new();
        RE_BINDING
            .get_or_init(|| Regex::new(r"(?i)layout\s*\(\s*binding\s*=\s*\d+\s*\)\s*").unwrap())
    };
    let re_binding_leading = {
        static RE_BINDING_LEADING: OnceLock<Regex> = OnceLock::new();
        RE_BINDING_LEADING
            .get_or_init(|| Regex::new(r"(?i)layout\s*\(\s*binding\s*=\s*\d+\s*,\s*").unwrap())
    };
    let re_binding_middle = {
        static RE_BINDING_MIDDLE: OnceLock<Regex> = OnceLock::new();
        RE_BINDING_MIDDLE.get_or_init(|| Regex::new(r"(?i),\s*binding\s*=\s*\d+").unwrap())
    };
    let re_empty_layout = {
        static RE_EMPTY_LAYOUT: OnceLock<Regex> = OnceLock::new();
        RE_EMPTY_LAYOUT.get_or_init(|| Regex::new(r"(?i)layout\s*\(\s*\)\s*").unwrap())
    };

    src.lines()
        .map(|line| {
            // 只处理 standalone non-opaque uniform 行：
            // - 必须含 uniform 关键字
            // - 排除 block（含 `{` 的单行 block，或多行 block 的声明行——
            //   声明行不以 `;` 结尾，`layout(std140, binding=0) uniform Block`）
            // - 排除 opaque（sampler/image/atomic_uint——binding 合法保留）
            if !line.contains("uniform")
                || line.contains('{')
                || !line.trim_end().ends_with(';')
                || line.contains("sampler")
                || line.contains("image")
                || line.contains("atomic_uint")
            {
                return line.to_string();
            }
            let l = re_binding_middle.replace_all(line, "");
            let l = re_binding_leading.replace_all(&l, "layout(");
            let l = re_binding.replace_all(&l, "");
            let l = re_empty_layout.replace_all(&l, "");
            l.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 移除 UBO/SSBO 的实例名，使成员全局可见
///
/// spirv-cross 默认为 UBO/SSBO 添加实例名（如 `} _20;`），导致成员访问
/// 需要通过 `实例名.成员名`（如 `_20.ModelViewMat`）。MC 通过
/// `glGetUniformLocation(program, "ModelViewMat")` 按成员名查询时返回 -1，
/// 因为带实例名的 UBO 成员必须用 "块名.成员名" 查询，直接按成员名查不到。
///
/// 修复：
/// 1. 移除 UBO/SSBO 末尾的实例名：`} _20;` → `};`
/// 2. 替换函数体中的实例名引用：`_20.ModelViewMat` → `ModelViewMat`
///
/// 这样 UBO 成员变为全局可见，`glGetUniformLocation` 可直接按成员名查询。
///
/// 增强版：
/// 1. 更健壮的正则表达式，处理更多边缘情况
/// 2. 支持带初始化的实例名声明
/// 3. 避免误处理 C 风格结构体变量
/// 4. 处理多行 UBO 声明
fn strip_ubo_instance_name(src: &str) -> String {
    // 匹配 UBO/SSBO 块声明末尾的实例名
    // 格式：} _N;  或  } _N = ...;（带初始化的罕见情况）
    // 或 } _N;\n（多行声明）
    static RE_INSTANCE: OnceLock<Regex> = OnceLock::new();
    let re_instance = RE_INSTANCE
        .get_or_init(|| Regex::new(r"\}\s*(?P<inst>_\w+)(?:\s*=\s*[^;]*)?\s*;").unwrap());

    // 收集所有 UBO/SSBO 实例名
    let instance_names: Vec<String> = re_instance
        .captures_iter(src)
        .filter_map(|c| c.name("inst").map(|m| m.as_str().to_string()))
        .collect();

    if instance_names.is_empty() {
        return src.to_string();
    }

    // 1. 移除实例名声明：`} _20;` → `};`
    let result = re_instance.replace_all(src, "};").to_string();

    // 2. 替换函数体中的 `实例名.成员` → `成员`
    //    用 \b 边界确保只匹配完整的实例名（避免误伤其他变量）
    let mut result = result;
    for inst in &instance_names {
        // 动态构造的 Regex：pattern 依赖运行时解析到的实例名 inst（如 _20、_30）
        let pattern = format!(r"\b{}\.", regex::escape(inst));
        let re = Regex::new(&pattern).unwrap();
        result = re.replace_all(&result, "").to_string();
    }

    // 3. 处理 UBO 块内的成员引用（如 _20.ModelViewMat 在块内）
    //    这可能出现在 UBO 块的成员初始化中
    for inst in &instance_names {
        let pattern = format!(r"\b{}\.", regex::escape(inst));
        let re = Regex::new(&pattern).unwrap();
        result = re.replace_all(&result, "").to_string();
    }

    log::debug!(
        "[ShaderTranslator] postprocess 移除了 UBO/SSBO 实例名: {:?}",
        instance_names
    );

    result
}

/// 修复 atomic counter 的 binding 限定符
///
/// spirv-cross 翻译桌面 GLSL 的 atomic_uint 时，输出 `layout(offset = N) uniform atomic_uint`，
/// 但 GLES 要求 atomic counter 用 `layout(binding = N)` 指定绑定点（offset 无效）。
///
/// 处理两种形式：
/// - `layout(offset = N) uniform atomic_uint` → `layout(binding = N) uniform atomic_uint`
/// - `layout(offset = N, X) uniform atomic_uint` → `layout(binding = N, X) uniform atomic_uint`
fn fix_atomic_counter_binding(src: &str) -> String {
    // offset 是唯一限定符: layout(offset = N) → layout(binding = N)
    let re_offset_only = {
        static RE_OFFSET_ONLY: OnceLock<Regex> = OnceLock::new();
        RE_OFFSET_ONLY.get_or_init(|| {
            Regex::new(r"(?i)layout\s*\(\s*offset\s*=\s*(\d+)\s*\)\s*(uniform\s+atomic_uint)")
                .unwrap()
        })
    };
    let result = re_offset_only
        .replace_all(src, "layout(binding = $1) $2")
        .to_string();

    // offset 在开头: layout(offset = N, X) → layout(binding = N, X)
    let re_offset_leading = {
        static RE_OFFSET_LEADING: OnceLock<Regex> = OnceLock::new();
        RE_OFFSET_LEADING
            .get_or_init(|| Regex::new(r"(?i)layout\s*\(\s*offset\s*=\s*(\d+)\s*,\s*").unwrap())
    };
    let result = re_offset_leading
        .replace_all(&result, "layout(binding = $1, ")
        .to_string();

    // offset 在中间/末尾: layout(X, offset = N) → layout(X, binding = N)
    let re_offset_middle = {
        static RE_OFFSET_MIDDLE: OnceLock<Regex> = OnceLock::new();
        RE_OFFSET_MIDDLE.get_or_init(|| Regex::new(r"(?i),\s*offset\s*=\s*(\d+)").unwrap())
    };
    re_offset_middle
        .replace_all(&result, ", binding = $1")
        .to_string()
}

/// 为缺少 format/binding 的 image uniform 注入 layout 限定符
///
/// GLES 要求 image uniform 必须同时有 binding 和 format 限定符。
/// 修复：
/// - 无 layout 前缀的 image：注入 binding（从 0 递增）+ format
/// - 有 layout(binding=N) 但无 format 的 image：补充 format（保留原 binding）
/// - 有 layout(format) 但无 binding 的 image：补充 binding（保留原 format）
///
/// writeonly image 默认 r32f，可读写 image 默认 r32ui。
fn inject_image_format(src: &str) -> String {
    // 情况1：无 layout 前缀的裸 image 声明 → 注入 binding + format
    static RE_BARE_IMAGE: OnceLock<Regex> = OnceLock::new();
    let re_bare_image = RE_BARE_IMAGE.get_or_init(|| {
        Regex::new(
            r"(?m)^(?P<indent>\s*)uniform\s+(?P<quals>(?:writeonly\s+|readonly\s+)?(?:highp\s+|mediump\s+|lowp\s+)?)(?P<type>image\w+)\s+(?P<name>\w+)\s*;",
        )
        .unwrap()
    });

    let mut binding: u32 = 0;
    let result = re_bare_image
        .replace_all(src, |caps: &regex::Captures| {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let quals = caps.name("quals").map(|m| m.as_str()).unwrap_or("");
            let img_type = caps.name("type").map(|m| m.as_str()).unwrap_or("");
            let name = caps.name("name").map(|m| m.as_str()).unwrap_or("");

            let format = if quals.contains("writeonly") {
                "r32f"
            } else {
                "r32ui"
            };
            let b = binding;
            binding += 1;
            format!(
                "{}layout(binding = {}, {}) uniform {}{} {};",
                indent, b, format, quals, img_type, name
            )
        })
        .to_string();

    // 情况2：有 layout(binding=N) 但无 format 的 image → 补充 format
    static RE_BOUND_NO_FORMAT: OnceLock<Regex> = OnceLock::new();
    let re_bound_no_format = RE_BOUND_NO_FORMAT.get_or_init(|| {
        Regex::new(
            r"(?m)^(?P<indent>\s*)layout\s*\(\s*binding\s*=\s*(?P<binding>\d+)\s*\)\s*(?P<quals>(?:writeonly\s+|readonly\s+)?(?:highp\s+|mediump\s+|lowp\s+)?)(?P<type>image\w+)\s+(?P<name>\w+)\s*;",
        )
        .unwrap()
    });

    let result = re_bound_no_format
        .replace_all(&result, |caps: &regex::Captures| {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let b = caps.name("binding").map(|m| m.as_str()).unwrap_or("0");
            let quals = caps.name("quals").map(|m| m.as_str()).unwrap_or("");
            let img_type = caps.name("type").map(|m| m.as_str()).unwrap_or("");
            let name = caps.name("name").map(|m| m.as_str()).unwrap_or("");

            let format = if quals.contains("writeonly") {
                "r32f"
            } else {
                "r32ui"
            };
            format!(
                "{}layout(binding = {}, {}) uniform {}{} {};",
                indent, b, format, quals, img_type, name
            )
        })
        .to_string();

    // 情况3：有 layout(format) 但无 binding 的 image → 补充 binding
    static RE_FORMAT_NO_BINDING: OnceLock<Regex> = OnceLock::new();
    let re_format_no_binding = RE_FORMAT_NO_BINDING.get_or_init(|| {
        Regex::new(
            r"(?m)^(?P<indent>\s*)layout\s*\(\s*(?P<format>\w+)\s*\)\s*(?P<quals>(?:writeonly\s+|readonly\s+)?(?:highp\s+|mediump\s+|lowp\s+)?)(?P<type>image\w+)\s+(?P<name>\w+)\s*;",
        )
        .unwrap()
    });

    re_format_no_binding
        .replace_all(&result, |caps: &regex::Captures| {
            let indent = caps.name("indent").map(|m| m.as_str()).unwrap_or("");
            let format = caps.name("format").map(|m| m.as_str()).unwrap_or("r32ui");
            let quals = caps.name("quals").map(|m| m.as_str()).unwrap_or("");
            let img_type = caps.name("type").map(|m| m.as_str()).unwrap_or("");
            let name = caps.name("name").map(|m| m.as_str()).unwrap_or("");

            let b = binding;
            binding += 1;
            format!(
                "{}layout(binding = {}, {}) uniform {}{} {};",
                indent, b, format, quals, img_type, name
            )
        })
        .to_string()
}

/// 确保 precision highp float/int 声明存在（对齐 MobileGlues forceSupporterOutput）
///
/// **已移除调用**：spike 实测 spirv-cross 在 es_default_float/int_precision_highp=true
/// 下自动输出 precision（FS 有、VS 无且合法——VS 的 float/int 默认精度即 highp）。
/// 保留此函数仅为 310/300 回退路径的防御性备用，若未来出现缺 precision 编译失败
/// 可恢复调用。
#[allow(dead_code)]
fn ensure_precision(source: &str) -> String {
    let mut result = source.to_string();

    // 移除所有已有的 precision 声明
    static RE_PRECISION: OnceLock<Regex> = OnceLock::new();
    let re_precision = RE_PRECISION.get_or_init(|| {
        Regex::new(r"(?m)^\s*precision\s+\w+\s+(?:float|int)\s*;.*$(\n)?").unwrap()
    });
    result = re_precision.replace_all(&result, "").to_string();

    let precision_decl = "precision highp float;\nprecision highp int;\n";

    // 在 #extension 之后或 #version 之后插入
    let last_ext = result.rfind("#extension");
    if let Some(pos) = last_ext.and_then(|p| result[p..].find('\n').map(|n| p + n + 1)) {
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
    fn test_strip_uniform_location_standalone() {
        // standalone uniform 的 location 应被移除（GLES 由 glGetUniformLocation 动态查询）
        let input = "layout(location = 0) uniform mat4 MVP;";
        let result = strip_uniform_locations(input);
        assert_eq!(result, "uniform mat4 MVP;");
    }

    #[test]
    fn test_strip_uniform_location_preserves_block() {
        // uniform block（含 `{`）用 binding 不用 location，不应被处理
        let input = "layout(std140) uniform Block { mat4 m; };";
        let result = strip_uniform_locations(input);
        assert_eq!(result, "layout(std140) uniform Block { mat4 m; };");
    }

    #[test]
    fn test_strip_uniform_location_with_other_qualifier() {
        // layout(location = N, std140) → layout(std140)（仅移除 location，保留其他限定符）
        let input = "layout(location = 3, column_major) uniform mat4 M;";
        let result = strip_uniform_locations(input);
        assert_eq!(result, "layout(column_major) uniform mat4 M;");
    }

    #[test]
    fn test_strip_varying_location_out() {
        let input = "layout(location = 1) out vec4 fragColor;";
        let result = strip_varying_locations(input);
        assert_eq!(result, "out vec4 fragColor;");
    }

    #[test]
    fn test_strip_varying_location_flat_out() {
        // 带插值修饰符的 out 声明（MC 光照/法线传递常用）
        let input = "layout(location = 2) flat out highp vec3 vNormal;";
        let result = strip_varying_locations(input);
        assert_eq!(result, "flat out highp vec3 vNormal;");
    }

    #[test]
    fn test_strip_varying_location_smooth_in() {
        let input = "layout(location = 0) smooth in vec4 color;";
        let result = strip_varying_locations(input);
        assert_eq!(result, "smooth in vec4 color;");
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
        let input = "layout(location = 0) in vec3 Position;\nlayout(location = 1) flat out vec4 vertexColor;\n";
        let result = strip_varying_locations(input);
        assert!(result.contains("in vec3 Position;"));
        assert!(result.contains("flat out vec4 vertexColor;"));
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
        // binding 移除仅在 300 es 执行（ES 3.0 不支持 binding）
        let input = "layout(binding = 0) uniform sampler2D tex;";
        let result = post_process(input, 300);
        assert!(!result.contains("binding"));
    }

    #[test]
    fn test_remove_binding_leading() {
        let input = "layout(binding = 0, std140) uniform Block { mat4 m; };";
        let result = post_process(input, 300);
        assert!(!result.contains("binding"));
        assert!(result.contains("layout(std140)"));
    }

    #[test]
    fn test_remove_binding_middle() {
        let input = "layout(std140, binding = 2, column_major) uniform Block { mat4 m; };";
        let result = post_process(input, 300);
        assert!(!result.contains("binding"));
        assert!(result.contains("std140"));
        assert!(result.contains("column_major"));
    }

    #[test]
    fn test_image_binding_preserved() {
        // image 的 binding 不应被移除（300 移除 binding 时跳过 image 行）
        let input = "layout(binding = 0, rgba32f) uniform writeonly highp image2D img;";
        let result = post_process(input, 300);
        assert!(
            result.contains("binding = 0"),
            "image binding should be preserved, got: {}",
            result
        );
    }

    #[test]
    fn test_320_keeps_binding() {
        // 320 es 保留 binding（ES 3.1+ 支持，spike_c 实测 spirv-cross 输出
        // layout(binding = 0) 且 11/11 通过）
        let input = "layout(binding = 0) uniform sampler2D tex;";
        let result = post_process(input, 320);
        assert!(result.contains("binding"), "got: {}", result);
    }

    #[test]
    fn test_out_color_location() {
        // outColorN 的 location 由 fix_out_color_locations 注入（320 同样重注）
        let input = "out vec4 outColor0;";
        let result = post_process(input, 320);
        assert!(result.contains("layout(location=0) out vec4 outColor0;"));
    }

    #[test]
    fn test_out_color_flat_location() {
        // 带插值修饰符的 outColor（如 flat out ivec4 outColor1）
        let input = "flat out ivec4 outColor1;";
        let result = post_process(input, 320);
        assert!(result.contains("layout(location=1) flat out ivec4 outColor1;"));
    }

    #[test]
    fn test_out_color_320_strips_existing_layout_first() {
        // 320 保留 location 路径：spirv-cross 输出带 layout(location = N) 的
        // outColorN，应剥离原 location 再注入正确值（MC framebuffer 约定）
        let input = "layout(location = 2) out vec4 outColor0;";
        let result = post_process(input, 320);
        assert!(
            result.contains("layout(location=0) out vec4 outColor0;"),
            "outColor0 应重注为 location=0（而非保留 spirv-cross 的 2），got: {}",
            result
        );
    }

    #[test]
    fn test_post_process_300_strips_varying_locations() {
        // 300 es 回退：spirv-cross 输出 → post_process → varying location 被移除
        let input = "#version 300 es\nlayout(location = 0) in vec2 texCoord0;\nlayout(location = 0) out vec4 fragColor;\nvoid main() { fragColor = vec4(texCoord0, 0.0, 1.0); }\n";
        let result = post_process(input, 300);
        assert!(result.contains("in vec2 texCoord0;"));
        assert!(result.contains("out vec4 fragColor;"));
        // varying 的 location 应被移除
        assert!(!result.contains("layout(location = 0) in"));
        assert!(!result.contains("layout(location = 0) out"));
    }

    #[test]
    fn test_post_process_300_strips_uniform_location() {
        // 300 es 回退：standalone uniform location 应被移除（ES 3.0 不支持）
        let input = "#version 300 es\nlayout(location = 0) uniform mat4 ProjMat;\nlayout(location = 0) in vec3 Position;\nvoid main() { gl_Position = ProjMat * vec4(Position, 1.0); }\n";
        let result = post_process(input, 300);
        assert!(
            result.contains("uniform mat4 ProjMat;"),
            "uniform 应保留但无 location，got: {}",
            result
        );
        assert!(
            !result.contains("location"),
            "300 es 不应残留任何 location，got: {}",
            result
        );
    }

    #[test]
    fn test_post_process_320_keeps_locations() {
        // 320 es：in/out 与 uniform location 全部保留（ES 3.2 合法）
        let input = "#version 320 es\nlayout(location = 0) uniform mat4 ProjMat;\nlayout(location = 0) in vec3 Position;\nlayout(location = 0) out vec4 fragColor;\nvoid main() { gl_Position = ProjMat * vec4(Position, 1.0); fragColor = vec4(1.0); }\n";
        let result = post_process(input, 320);
        assert!(
            result.contains("layout(location = 0) uniform mat4 ProjMat"),
            "320 应保留 uniform location，got: {}",
            result
        );
        assert!(
            result.contains("layout(location = 0) in vec3 Position"),
            "320 应保留 in location，got: {}",
            result
        );
        assert!(
            result.contains("layout(location = 0) out vec4 fragColor"),
            "320 应保留 out location，got: {}",
            result
        );
    }

    #[test]
    fn test_post_process_310_strips_locations_keeps_binding() {
        // 310 es：strip location（保守），保留 binding（ES 3.1 支持）
        let input = "#version 310 es\nlayout(location = 0) uniform mat4 ProjMat;\nlayout(location = 0) in vec3 Position;\nlayout(binding = 0) uniform sampler2D tex;\nvoid main() { gl_Position = ProjMat * vec4(Position, 1.0); }\n";
        let result = post_process(input, 310);
        assert!(
            !result.contains("location"),
            "310 不应残留 location（保守 strip），got: {}",
            result
        );
        assert!(
            result.contains("binding = 0"),
            "310 应保留 binding，got: {}",
            result
        );
    }

    #[test]
    fn test_strip_ubo_instance_name_basic() {
        // spirv-cross 输出带实例名的 UBO：} _20; → };
        // 函数体中的 _20.ModelViewMat → ModelViewMat
        let input = "layout(std140) uniform UniformBlockVS\n{\n    mat4 ModelViewMat;\n    mat4 ProjMat;\n} _20;\n\nvoid main()\n{\n    gl_Position = (_20.ProjMat * _20.ModelViewMat) * vec4(Position, 1.0);\n}\n";
        let result = strip_ubo_instance_name(input);
        assert!(
            !result.contains("} _20;"),
            "instance name should be removed from declaration, got: {}",
            result
        );
        assert!(
            result.contains("};"),
            "declaration should end with }};, got: {}",
            result
        );
        assert!(
            !result.contains("_20."),
            "instance name reference should be replaced, got: {}",
            result
        );
        assert!(
            result.contains("ProjMat * ModelViewMat"),
            "member access should be direct, got: {}",
            result
        );
    }

    #[test]
    fn test_strip_ubo_instance_name_multiple_blocks() {
        // 多个 UBO 各自有实例名，应全部移除并替换引用
        let input = "layout(std140) uniform BlockA\n{\n    mat4 m;\n} _10;\nlayout(std140) uniform BlockB\n{\n    vec4 v;\n} _20;\nvoid main()\n{\n    vec4 x = _10.m * _20.v;\n}\n";
        let result = strip_ubo_instance_name(input);
        assert!(!result.contains("} _10;"));
        assert!(!result.contains("} _20;"));
        assert!(!result.contains("_10."));
        assert!(!result.contains("_20."));
        assert!(result.contains("m * v"));
    }

    #[test]
    fn test_strip_ubo_instance_name_preserves_struct_var() {
        // C 风格结构体变量声明（变量名不以 _ 开头）不应被误处理
        let input = "struct S { int x; } myVar;\nvoid main() { myVar.x = 1; }\n";
        let result = strip_ubo_instance_name(input);
        assert!(
            result.contains("} myVar;"),
            "struct variable should be preserved, got: {}",
            result
        );
        assert!(
            result.contains("myVar.x"),
            "struct member access should be preserved, got: {}",
            result
        );
    }

    #[test]
    fn test_strip_ubo_instance_name_no_instance() {
        // 无实例名的 UBO（} ;）应原样返回
        let input = "layout(std140) uniform Block\n{\n    mat4 m;\n};\nvoid main() { }\n";
        let result = strip_ubo_instance_name(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_post_process_strips_ubo_instance_name() {
        // 端到端：spirv-cross 输出 → post_process → UBO 实例名被移除
        // 模拟日志中的实际 VS 输出
        let input = "#version 320 es\nprecision highp float;\nprecision highp int;\nlayout(std140) uniform DynamicTransforms\n{\n    mat4 ModelViewMat;\n    mat4 ProjMat;\n} _20;\nin vec3 Position;\nvoid main()\n{\n    gl_Position = (_20.ProjMat * _20.ModelViewMat) * vec4(Position, 1.0);\n}\n";
        let result = post_process(input, 320);
        assert!(
            !result.contains("} _20;"),
            "instance name should be removed, got: {}",
            result
        );
        assert!(
            !result.contains("_20."),
            "instance reference should be replaced, got: {}",
            result
        );
        // 成员名应保留（用于 glGetUniformLocation 查询）
        assert!(result.contains("ModelViewMat"));
        assert!(result.contains("ProjMat"));
        // 原生 UBO 块（非 UniformBlock）应保留（不再拆解）
        assert!(
            result.contains("uniform DynamicTransforms"),
            "原生 UBO 应保留，got: {}",
            result
        );
    }
}
