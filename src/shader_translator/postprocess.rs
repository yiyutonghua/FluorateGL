//! GLSL ES 后处理模块
//!
//! 对齐 MobileGlues 的后处理逻辑：
//! - stripVaryingLocations：移除 in/out varying 的 layout(location=N)
//! - removeLayoutBinding：移除非 image 的 layout(binding=X)
//! - processOutColorLocations：为 outColorN 添加 layout(location=N)
//! - forceSupporterOutput：确保 precision highp float/int 声明

use regex::Regex;
use std::sync::OnceLock;

/// GLSL ES 后处理主入口
///
/// 执行顺序：
/// 0. 移除 in/out varying 的 layout(location=N)（解决跨 stage mismatch）
/// 1. 移除非 image 的 layout(binding=X)（image 的 binding 不能移除）
/// 1.5 移除 UBO/SSBO 实例名（spirv-cross 自动添加，导致 uniform 查询失败）
/// 2. 修复 atomic counter binding（offset → binding，GLES 要求 binding）
/// 3. 注入 image format 限定符（GLES 要求 image 必须有 format 和 binding）
/// 4. 处理 outColorN 的 location
/// 5. 确保 precision highp float/int 声明
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

    // 0.5 移除 standalone uniform 的 layout(location=N)
    //     preprocess 为 non-opaque uniform（mat4/vec3/float 等）注入 location 以满足
    //     glslang OpenGL SPIR-V 模式 parse 要求，且每个 shader 独立从 0 计数。
    //     跨 stage 链接时，VS 与 FS 中不同名 uniform 会占用同一 location（如 VS 的
    //     Color@1 与 FS 的 FogColor@1），导致 GLES linker 报 location 冲突。
    //     GLES 中 standalone uniform 无需显式 location（MC 通过 glGetUniformLocation
    //     动态查询），移除安全。uniform block（含 `{`，用 binding 不用 location）不受影响。
    result = strip_uniform_locations(&result);

    // 1. 移除非 image 的 layout(binding=X)（对齐 MobileGlues removeLayoutBinding）
    //    注意：image 的 binding 不能移除！image 必须通过 layout(binding=N) 与
    //    glBindImageTexture(unit,...) 的 unit 对应。移除后 image 无法正确绑定。
    //    MC 的 sampler/UBO binding 可以移除（MC 通过 glUniform1i/glBindBufferBase 设置）。
    //    策略：按行处理，跳过 image 声明行，对其余行移除 binding。
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

    result = result
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

    // 1.5 移除 UBO/SSBO 实例名（关键修复：解决 uniform 查询失败）
    //    spirv-cross 默认为 UBO/SSBO 添加实例名（如 `} _20;`），导致成员访问
    //    需要通过 `实例名.成员名`（如 `_20.ModelViewMat`）。MC 通过
    //    glGetUniformLocation(program, "ModelViewMat") 按成员名查询时返回 -1，
    //    因为带实例名的 UBO 成员必须用 "块名.成员名" 查询，直接按成员名查不到。
    //    移除实例名后，函数体变为直接成员访问，为下一步拆解做准备。
    result = strip_ubo_instance_name(&result);

    // 1.6 把生成的 UBO 拆解为 standalone uniform（关键：解决 MC uniform 查询失败）
    //     GLES 规范规定 UBO 成员没有 location，glGetUniformLocation 按成员名查询
    //     返回 -1。MC 的 Shader 类用 glGetUniformLocation 查询 uniform，UBO 方案
    //     导致所有 uniform 查不到，红屏。把 UniformBlockVS/FS/Other 拆解为
    //     standalone uniform（`uniform mat4 ModelViewMat;`）后，MC 可直接查询到。
    result = unwrap_generated_ubo(&result);

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
    //    正则容忍前导插值修饰符（flat/smooth/noperspective 等）
    let re_out_color = {
        static RE_OUT_COLOR: OnceLock<Regex> = OnceLock::new();
        RE_OUT_COLOR.get_or_init(|| {
            Regex::new(
                r"(?m)^(?P<prefix>(?:(?:flat|smooth|noperspective|centroid|invariant)\s+)*)out\s+(?P<type>(?:highp\s+|mediump\s+|lowp\s+)?\w+)\s+outColor(?P<num>\d+)\s*;",
            )
            .unwrap()
        })
    };
    result = re_out_color
        .replace_all(&result, |caps: &regex::Captures| {
            let prefix = caps.name("prefix").map(|m| m.as_str()).unwrap_or("");
            let typ = caps.name("type").map(|m| m.as_str()).unwrap_or("");
            let num = caps.name("num").map(|m| m.as_str()).unwrap_or("0");
            format!(
                "layout(location={}) {}out {} outColor{};",
                num, prefix, typ, num
            )
        })
        .to_string();

    // 5. 确保 precision 声明（对齐 MobileGlues forceSupporterOutput）
    result = ensure_precision(&result);

    result
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

/// 把 preprocess 生成的 UBO 拆解为 standalone uniform
///
/// preprocess 把 standalone uniform 包装进 UniformBlockVS/FS/Other（Vulkan SPIR-V
/// 要求 non-opaque uniform 必须在 buffer 中）。但 GLES 规范规定 UBO 成员没有
/// location，`glGetUniformLocation(program, "成员名")` 返回 -1。MC 的 Shader 类
/// 用 `glGetUniformLocation` 查询 uniform，UBO 方案导致所有 uniform 查不到，红屏。
///
/// 此函数把 `layout(...) uniform UniformBlockVS { members };` 拆解为
/// `uniform <type> <name>;` 列表，使每个 uniform 变为 standalone，MC 可直接查询。
///
/// 仅处理 preprocess 生成的 UniformBlockVS/FS/Other，不影响 MC 原生 UBO。
/// strip_ubo_instance_name 已移除实例名，函数体已是直接成员访问，无需替换引用。
fn unwrap_generated_ubo(src: &str) -> String {
    // 匹配我们生成的 UBO 块：layout(...) uniform UniformBlock(VS|FS|Other) { members };
    // (?s) 让 . 匹配换行，成员是多行的
    static RE_UBO: OnceLock<Regex> = OnceLock::new();
    let re_ubo = RE_UBO.get_or_init(|| {
        Regex::new(
            r"(?s)layout\s*\([^)]*\)\s*uniform\s+(UniformBlock(?:VS|FS|Other))\s*\{([^}]*)\}\s*;",
        )
        .unwrap()
    });

    let mut unwrapped_count = 0;
    let result = re_ubo
        .replace_all(src, |caps: &regex::Captures| {
            let members = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            unwrapped_count += 1;

            // 把每个成员声明转为 standalone uniform
            // 成员格式：`    mat4 ModelViewMat;` 或 `    highp mat4 ModelViewMat;`
            let mut standalone = String::new();
            for line in members.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                // 移除末尾分号，加 uniform 前缀
                let line = line.trim_end_matches(';').trim();
                standalone.push_str(&format!("uniform {};\n", line));
            }
            // 去掉末尾多余换行（replace_all 的替换文本不需要尾换行）
            standalone.trim_end().to_string()
        })
        .to_string();

    if unwrapped_count > 0 {
        log::debug!(
            "[ShaderTranslator] postprocess 拆解了 {} 个 UBO 为 standalone uniform",
            unwrapped_count
        );
    }

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
/// 始终强制使用 highp，移除所有已有 precision 声明后统一插入
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
    fn test_image_binding_preserved() {
        // image 的 binding 不应被移除
        let input = "layout(binding = 0, rgba32f) uniform writeonly highp image2D img;";
        let result = post_process(input);
        assert!(
            result.contains("binding = 0"),
            "image binding should be preserved, got: {}",
            result
        );
    }

    #[test]
    fn test_out_color_location() {
        // outColorN 的 location 在 strip 之后由 processOutColorLocations 重新添加
        let input = "out vec4 outColor0;";
        let result = post_process(input);
        assert!(result.contains("layout(location=0) out vec4 outColor0;"));
    }

    #[test]
    fn test_out_color_flat_location() {
        // 带插值修饰符的 outColor（如 flat out ivec4 outColor1）
        let input = "flat out ivec4 outColor1;";
        let result = post_process(input);
        assert!(result.contains("layout(location=1) flat out ivec4 outColor1;"));
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
        let input = "#version 310 es\nprecision highp float;\nprecision highp int;\nlayout(std140) uniform UniformBlockVS\n{\n    mat4 ModelViewMat;\n    mat4 ProjMat;\n} _20;\nin vec3 Position;\nvoid main()\n{\n    gl_Position = (_20.ProjMat * _20.ModelViewMat) * vec4(Position, 1.0);\n}\n";
        let result = post_process(input);
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
    }

    #[test]
    fn test_unwrap_generated_ubo_basic() {
        // UniformBlockVS 块应被拆解为 standalone uniform
        let input = "layout(std140) uniform UniformBlockVS\n{\n    mat4 ModelViewMat;\n    mat4 ProjMat;\n};\nvoid main() { gl_Position = ProjMat * ModelViewMat * vec4(0); }\n";
        let result = unwrap_generated_ubo(input);
        assert!(
            !result.contains("uniform UniformBlockVS"),
            "UBO block should be removed, got: {}",
            result
        );
        assert!(
            result.contains("uniform mat4 ModelViewMat;"),
            "should have standalone uniform ModelViewMat, got: {}",
            result
        );
        assert!(
            result.contains("uniform mat4 ProjMat;"),
            "should have standalone uniform ProjMat, got: {}",
            result
        );
        // 函数体直接成员访问保留（strip_ubo_instance_name 已处理）
        assert!(result.contains("ProjMat * ModelViewMat"));
    }

    #[test]
    fn test_unwrap_generated_ubo_fragment() {
        // UniformBlockFS 块也应被拆解
        let input = "layout(std140) uniform UniformBlockFS\n{\n    vec4 ColorModulator;\n};\nvoid main() { fragColor = ColorModulator; }\n";
        let result = unwrap_generated_ubo(input);
        assert!(
            result.contains("uniform vec4 ColorModulator;"),
            "should have standalone uniform ColorModulator, got: {}",
            result
        );
        assert!(!result.contains("uniform UniformBlockFS"));
    }

    #[test]
    fn test_unwrap_generated_ubo_preserves_native_ubo() {
        // MC 原生 UBO（非 UniformBlockVS/FS/Other）不应被拆解
        let input =
            "layout(std140) uniform DynamicTransforms\n{\n    mat4 m;\n};\nvoid main() {}\n";
        let result = unwrap_generated_ubo(input);
        assert!(
            result.contains("uniform DynamicTransforms"),
            "native UBO should be preserved, got: {}",
            result
        );
    }

    #[test]
    fn test_unwrap_generated_ubo_with_precision() {
        // 带 precision 限定符的成员
        let input = "layout(std140) uniform UniformBlockVS\n{\n    highp mat4 ModelViewMat;\n    mediump float FogStart;\n};\nvoid main() {}\n";
        let result = unwrap_generated_ubo(input);
        assert!(
            result.contains("uniform highp mat4 ModelViewMat;"),
            "should preserve precision, got: {}",
            result
        );
        assert!(result.contains("uniform mediump float FogStart;"));
    }

    #[test]
    fn test_post_process_unwraps_ubo_to_standalone() {
        // 端到端：spirv-cross 输出（带实例名） → post_process → standalone uniform
        let input = "#version 310 es\nprecision highp float;\nprecision highp int;\nlayout(std140) uniform UniformBlockVS\n{\n    mat4 ModelViewMat;\n    mat4 ProjMat;\n} _20;\nin vec3 Position;\nvoid main()\n{\n    gl_Position = (_20.ProjMat * _20.ModelViewMat) * vec4(Position, 1.0);\n}\n";
        let result = post_process(input);
        // UBO 块应被拆解
        assert!(
            !result.contains("uniform UniformBlockVS"),
            "UBO block should be unwrapped, got: {}",
            result
        );
        // 应有 standalone uniform 声明
        assert!(
            result.contains("uniform mat4 ModelViewMat;"),
            "should have standalone uniform, got: {}",
            result
        );
        assert!(result.contains("uniform mat4 ProjMat;"));
        // 函数体应直接访问成员（无实例名前缀）
        assert!(
            result.contains("ProjMat * ModelViewMat"),
            "should access members directly, got: {}",
            result
        );
        assert!(!result.contains("_20."));
    }
}
