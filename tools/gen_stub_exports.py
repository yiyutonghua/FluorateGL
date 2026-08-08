#!/usr/bin/env python3
"""FluorateGL GL stub 导出生成器（阶段 2/3：GL 4.x + 扩展后缀函数批量 stub）

签名源：
  1. glcorearb.h（Khronos GL core + 扩展全集，1257 个）——优先
  2. MobileGlues gl_stub.cpp/gl_native.cpp 的 STUB/NATIVE 宏——补漏

输出：
  src/gl/stub_exports.rs —— stub_fn! 宏 + 批量 stub 函数 + 透传别名 + STUB_COUNT
  src/symbols.rs —— 生成器段（标记之间）追加 stub 符号名

用法：python3 tools/gen_stub_exports.py
幂等：symbols.rs 生成器段按标记替换；stub_exports.rs 全量重写。
"""
import re, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GLCOREARB = "/data/data/com.termux/files/home/dev/MobileGlues/MobileGlues-cpp/include/GL/glcorearb.h"
MG_STUB = "/data/data/com.termux/files/home/dev/MobileGlues/MobileGlues-cpp/gl/gl_stub.cpp"
MG_NATIVE = "/data/data/com.termux/files/home/dev/MobileGlues/MobileGlues-cpp/gl/gl_native.cpp"
OUT_RS = os.path.join(ROOT, "src/gl/stub_exports.rs")
SYMBOLS_RS = os.path.join(ROOT, "src/symbols.rs")

# ============ 1. 签名解析 ============
def parse_glcorearb(path):
    src = open(path, encoding="utf-8", errors="replace").read()
    re_fn = re.compile(
        r"GLAPI\s+([A-Za-z_][A-Za-z0-9_ *]*?)\s+APIENTRY\s+(gl[A-Za-z0-9]+)\s*\(([^)]*)\)\s*;"
    )
    sigs = {}
    for m in re_fn.finditer(src):
        ret, name, args = m.group(1).strip(), m.group(2), m.group(3).strip()
        sigs[name] = (ret, args)
    return sigs

GLES_H = "/data/data/com.termux/files/home/dev/MobileGlues/MobileGlues-cpp/gles/gles.h"

def parse_gles_h(path):
    """MG gles.h 的 GL_FUNC_TYPEDEF —— GLES 函数签名（含 OES/EXT 后缀名）。
    re.M 逐行匹配（268 个）；跨行 typedef 极少，由 EXTRA_SIGS 手动补齐。"""
    src = open(path, encoding="utf-8", errors="replace").read()
    sigs = {}
    re_fn = re.compile(r"GL_FUNC_TYPEDEF\(\s*(\w+)\s*,\s*(gl[A-Za-z0-9]+)\s*,(.*?)\)\s*$", re.M)
    for m in re_fn.finditer(src):
        sigs[m.group(2)] = (m.group(1), m.group(3).strip())
    # 跨行 typedef 手动补齐（gles.h 中签名跨行的少数函数）
    EXTRA_SIGS = {
        "glMultiDrawArraysIndirectEXT": ("void", "GLenum mode, const void* indirect, GLsizei drawcount, GLsizei stride"),
        "glMultiDrawElementsIndirectEXT": ("void", "GLenum mode, GLenum type, const void* indirect, GLsizei drawcount, GLsizei stride"),
        "glMultiDrawElementsBaseVertexEXT": ("void", "GLenum mode, const GLsizei* count, GLenum type, const void* const* indices, GLsizei drawcount, const GLint* basevertex"),
    }
    for k, v in EXTRA_SIGS.items():
        if k not in sigs:
            sigs[k] = v
    return sigs

def parse_mg_macros(paths):
    sigs = {}
    re_fn = re.compile(
        r"(?:STUB_FUNCTION_HEAD|NATIVE_FUNCTION_HEAD)\(\s*(\w+)\s*,\s*(gl[A-Za-z0-9]+)\s*,(.*?)\)\s*(?:STUB_FUNCTION_END|NATIVE_FUNCTION_END)",
        re.S,
    )
    for p in paths:
        src = open(p, encoding="utf-8", errors="replace").read()
        # 仅匹配 HEAD 宏调用（单行；END 配对复杂且非必需——args 截断仅导致
        # stub 忽略更多参数，extern "C" 下安全）
        re_head = re.compile(r"(?:STUB_FUNCTION_HEAD|NATIVE_FUNCTION_HEAD)\(\s*(\w+)\s*,\s*(gl[A-Za-z0-9]+)\s*,(.*?)\)", re.M)
        for m in re_head.finditer(src):
            ret, name, args = m.group(1).strip(), m.group(2), m.group(3).strip()
            if name not in sigs:
                sigs[name] = (ret, args)
    return sigs

# ============ 2. 类型映射 ============
RUST_KEYWORDS = {
    "type", "ref", "match", "loop", "fn", "box", "move", "static", "in", "as",
    "break", "continue", "return", "struct", "enum", "trait", "impl", "use",
    "mod", "pub", "crate", "self", "super", "where", "async", "await", "dyn",
    "abstract", "become", "do", "final", "macro", "override", "priv", "typeof",
    "unsized", "virtual", "yield",
}

SCALAR_MAP = {
    "void": "()",
    "GLenum": "u32", "GLbitfield": "u32", "GLuint": "u32", "GLhandleARB": "u32",
    "GLsizei": "i32", "GLint": "i32", "GLfixed": "i32", "GLclampx": "i32",
    "GLfloat": "f32", "GLclampf": "f32", "GLdouble": "f64", "GLclampd": "f64",
    "GLint64": "i64", "GLuint64": "u64", "GLint64EXT": "i64", "GLuint64EXT": "u64",
    "GLboolean": "u8", "GLubyte": "u8", "GLbyte": "i8", "GLchar": "i8",
    "GLshort": "i16", "GLushort": "u16",
    "GLintptr": "isize", "GLsizeiptr": "isize",
    "GLintptrARB": "isize", "GLsizeiptrARB": "isize",
    "GLsync": "*mut std::ffi::c_void",
    "GLeglImageOES": "*mut std::ffi::c_void",
    "GLeglClientBufferEXT": "*mut std::ffi::c_void",
    "GLuint64EXT": "u64",
    "GLhalfNV": "u16", "GLvdpauSurfaceNV": "isize",
    "GLcharARB": "i8", "GLhandleARB": "u32",
}

def map_scalar(t):
    t = t.strip()
    if t in SCALAR_MAP:
        return SCALAR_MAP[t]
    return None  # 未知

def map_c_type(t, is_param=True):
    """C 类型 → Rust 类型。返回 (rust_ty, ok)"""
    t = t.strip()
    # 函数指针 / 未知复杂类型 → *mut c_void
    if "PROC" in t or "(" in t or ")" in t:
        return "*mut std::ffi::c_void", True
    if t == "void":
        return "()", True
    # 指针
    if "*" in t:
        depth = t.count("*")
        # 逐层分析 const（const 修饰的是 * 之前的元素类型）
        parts = []
        rest = t
        ok = True
        for i in range(depth):
            rest = rest.strip()
            star = rest.find("*")
            if star < 0:
                ok = False
                break
            prefix = rest[:star].strip()
            if "const" in prefix:
                parts.append("*const")
            else:
                parts.append("*mut")
            # 去掉一个 *，其余（* 前元素类型 + * 后内容）保留
            rest = (rest[:star] + " " + rest[star + 1:]).strip()
        if not ok:
            return "*mut std::ffi::c_void", True
        elem = rest.replace("const", " ").strip()
        if elem in ("", "void"):
            base = "std::ffi::c_void"
        elif elem == "GLchar":
            base = "std::ffi::c_char"
        elif elem == "GLDEBUGPROC":
            base = "std::ffi::c_void"
        else:
            s = map_scalar(elem)
            if s is None:
                return "*mut std::ffi::c_void", True
            base = s
        return " ".join(parts) + " " + base, True
    # 标量
    s = map_scalar(t)
    if s is not None:
        return s, True
    return "*mut std::ffi::c_void", True  # 兜底

def parse_args(args_str):
    """解析 C 参数列表 → [(rust_name, rust_type)]"""
    if not args_str.strip() or args_str.strip() == "void":
        return []
    out = []
    # 按顶层逗号分割（忽略括号内逗号——极少见，简化按逗号切）
    for i, seg in enumerate(args_str.split(",")):
        seg = seg.strip()
        if not seg:
            continue
        # 指针粘连拆分：`*name`/`const*name` → `* name`/`const * name`
        # （C 源码风格 `const void* const*indices` 名字粘在 * 后）
        seg = re.sub(r"\*(\w)", r"* \1", seg)
        # 移除数组后缀 [N]
        seg = re.sub(r"\[\s*\d*\s*\]", "", seg)
        # 类型 + 名字（名字可能是最后 token）
        toks = seg.split()
        if len(toks) == 0:
            continue
        # 尝试：最后 token 是名字（非类型、非指针、标识符合法）
        name = toks[-1]
        if (name in SCALAR_MAP or name == "void" or "*" in name
                or name.startswith("const") or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name)):
            name = f"_arg{i}"
        ty = " ".join(toks[:-1]) if name != f"_arg{i}" else " ".join(toks)
        rty, _ = map_c_type(ty)
        if name in RUST_KEYWORDS:
            name = name + "_"
        out.append((name, rty))
    return out

# ============ 3. 排除/别名配置 ============
# 固定管线前缀（用户记录待办，不生成）
FIXED_PIPELINE_PREFIXES = (
    "glBegin", "glEnd", "glVertex", "glNormal", "glColor", "glIndex", "glTexCoord",
    "glMultiTexCoord", "glFog", "glLight", "glLightModel", "glMaterial", "glMatrixMode",
    "glLoadMatrix", "glLoadIdentity", "glMultMatrix", "glOrtho", "glFrustum",
    "glPushMatrix", "glPopMatrix", "glRotate", "glScale", "glTranslate", "glClipPlane",
    "glGetClipPlane", "glCallList", "glDeleteLists", "glGenLists", "glNewList",
    "glEndList", "glListBase", "glIsList", "glTexGen", "glGetTexGen", "glLineStipple",
    "glPolygonStipple", "glGetPolygonStipple", "glPixelTransfer", "glPixelMap",
    "glGetPixelMap", "glRasterPos", "glWindowPos", "glRect", "glDrawPixels",
    "glCopyPixels", "glBitmap", "glAccum", "glClearAccum", "glClearIndex",
    "glEdgeFlag", "glMap1", "glMap2", "glEvalCoord", "glEvalMesh", "glEvalPoint",
    "glGetMap", "glFeedbackBuffer", "glPassThrough", "glSelectBuffer", "glInitNames",
    "glLoadName", "glPushName", "glPopName", "glArrayElement", "glLockArrays",
    "glUnlockArrays", "glPushAttrib", "glPopAttrib", "glPushClientAttrib",
    "glPopClientAttrib", "glEnableClientState", "glDisableClientState",
    "glColorTable", "glColorSubTable", "glColorPointer", "glEdgeFlagPointer",
    "glFogCoord", "glIndexPointer", "glNormalPointer", "glSecondaryColor",
    "glTexCoordPointer", "glVertexPointer", "glInterleavedArrays",
    "glClientActiveTexture", "glGetPointerIndexedvEXT", "glMatrixLoad", "glMatrixMult",
    "glMatrixOrtho", "glMatrixPop", "glMatrixPush", "glMatrixRotate", "glMatrixScale",
    "glMatrixTranslate", "glMatrixFrustum", "glMatrixLoadIdentity", "glMatrixLoadTranspose",
    "glColorFormat", "glNormalFormat", "glTexCoordFormat", "glVertexFormat",
    "glSecondaryColorFormat", "glIndexFormat", "glEdgeFlagFormat", "glFogCoordFormat",
    "glVertexAttribFormatNV", "glVertexAttribIFormatNV", "glVertexAttribLFormatNV",
    "glVertexArrayColorOffset", "glVertexArrayEdgeFlagOffset", "glVertexArrayFogCoordOffset",
    "glVertexArrayIndexOffset", "glVertexArrayMultiTexCoordOffset",
    "glVertexArrayNormalOffset", "glVertexArraySecondaryColorOffset",
    "glVertexArrayTexCoordOffset", "glVertexArrayVertexOffset", "glVertexArrayVertexAttribOffset",
)

# 透传别名：GLES 有真实实现/我们已有 base 逻辑的后缀版本 → 转发到已有函数
# (name → 目标函数)
PASSTHROUGH_ALIASES = {
    # base vertex 系列（GLES 3.2 core / OES / EXT）
    "glDrawElementsBaseVertexOES": "glDrawElementsBaseVertex",
    "glDrawElementsBaseVertexEXT": "glDrawElementsBaseVertex",
    "glDrawRangeElementsBaseVertexOES": "glDrawRangeElementsBaseVertex",
    "glDrawRangeElementsBaseVertexEXT": "glDrawRangeElementsBaseVertex",
    "glDrawElementsInstancedBaseVertexOES": "glDrawElementsInstancedBaseVertex",
    "glDrawElementsInstancedBaseVertexEXT": "glDrawElementsInstancedBaseVertex",
    # base instance 系列（EXT/ANGLE）
    "glDrawArraysInstancedBaseInstanceEXT": "glDrawArraysInstancedBaseInstance",
    "glDrawArraysInstancedBaseInstanceANGLE": "glDrawArraysInstancedBaseInstance",
    "glDrawElementsInstancedBaseInstanceEXT": "glDrawElementsInstancedBaseInstance",
    "glDrawElementsInstancedBaseInstanceANGLE": "glDrawElementsInstancedBaseInstance",
    "glDrawElementsInstancedBaseVertexBaseInstanceEXT": "glDrawElementsInstancedBaseVertexBaseInstance",
    "glDrawElementsInstancedBaseVertexBaseInstanceANGLE": "glDrawElementsInstancedBaseVertexBaseInstance",
    # multi draw / indirect（EXT）
    "glMultiDrawArraysEXT": "glMultiDrawArrays",
    "glMultiDrawElementsEXT": "glMultiDrawElements",
    "glMultiDrawElementsBaseVertexEXT": "glMultiDrawElementsBaseVertex",
    "glMultiDrawArraysIndirectEXT": "glMultiDrawArraysIndirect",
    "glMultiDrawElementsIndirectEXT": "glMultiDrawElementsIndirect",
    # 单版本 indirect（ARB 名）
    "glDrawArraysIndirectARB": "glDrawArraysIndirect",
    "glDrawElementsIndirectARB": "glDrawElementsIndirect",
    # buffer storage / texture storage（GLES 原生）
    "glBufferStorageEXT": "glBufferStorage",
    "glTexStorage2DEXT": "glTexStorage2D",
    "glTexStorage3DEXT": "glTexStorage3D",
    "glTexBufferEXT": "glTexBuffer",
    "glTexBufferRangeEXT": "glTexBufferRange",
    # primitive restart
    "glPrimitiveRestartIndexNV": "glPrimitiveRestartIndex",
}

# 已知既有导出（排除）——动态从 nm 读
def load_ours():
    import subprocess
    so = os.path.join(ROOT, "target/debug/libfluorategl.so")
    out = subprocess.run(["nm", "-D", so], capture_output=True, text=True).stdout
    names = set()
    for line in out.splitlines():
        parts = line.split()
        if len(parts) >= 3 and parts[1] == "T" and parts[2].startswith("gl"):
            names.add(parts[2])
    return names

def is_fixed_pipeline(name):
    if not any(name.startswith(p) for p in FIXED_PIPELINE_PREFIXES):
        return False
    # 例外：GL 2.0+ core 便捷函数不是固定管线
    if any(x in name for x in ("Attrib", "VertexArray", "Binding")):
        return False
    if re.search(r"P[1-4](ui|uiv)$", name):
        return False
    return True

# ============ 4. 主流程 ============
def main():
    core_sigs = parse_glcorearb(GLCOREARB)
    gles_sigs = parse_gles_h(GLES_H)
    mg_sigs = parse_mg_macros([MG_STUB, MG_NATIVE])
    ours = load_ours()
    ours.add("gl")  # 防止误匹配

    # 合并签名（glcorearb > gles.h > MG 宏）
    sigs = dict(core_sigs)
    for k, v in gles_sigs.items():
        if k not in sigs:
            sigs[k] = v
    for k, v in mg_sigs.items():
        if k not in sigs:
            sigs[k] = v

    # 缺失 = 全签名 - 已导出 - 固定管线
    missing = {}
    for name, sig in sigs.items():
        if name in ours:
            continue
        if is_fixed_pipeline(name):
            continue
        missing[name] = sig

    # 分类统计
    def has_suffix(n):
        return bool(re.search(r"(OES|ARB|EXT|AMD|ATI|NV|QCOM|INTEL|SGIS|SGIX|APPLE|IBM|MESA|INGR|SUN|3DFX|GREMEDY|KHR|ANGLE|DMP|S3|WIN32|REND|HP|TI|VIV|IMGSX|FJ|OVR|MALI|TIZEN|WESTON|SEC|GOOGLE|MG_)\w*$", n))

    core_missing = {n: s for n, s in missing.items() if not has_suffix(n)}
    ext_missing = {n: s for n, s in missing.items() if has_suffix(n)}
    aliases = {n: (PASSTHROUGH_ALIASES[n], missing[n]) for n in PASSTHROUGH_ALIASES if n in missing}
    for n in aliases:
        missing.pop(n, None)

    # 生成 Rust
    lines = []
    lines.append("//! GL stub 导出（生成器输出 tools/gen_stub_exports.py，勿手改）")
    lines.append("//!")
    lines.append("//! 阶段 2/3：GL 4.x + 扩展后缀函数批量 stub（GLES 无对应或超出北极星 3.3 范围）。")
    lines.append("//! stub 语义：存在不崩溃（LWJGL 绑定 null 防护），调用 no-op + debug 日志，")
    lines.append("//! 返回类型默认值（0/null/()）。对齐 MobileGlues STUB 策略。")
    lines.append("")
    lines.append("macro_rules! stub_fn {")
    lines.append("    ($name:ident, $ret:ty, ($($arg:ident: $ty:ty),*)) => {")
    lines.append("        #[unsafe(no_mangle)]")
    lines.append("        #[allow(non_snake_case, unused_variables)]")
    lines.append("        pub extern \"C\" fn $name($($arg: $ty),*) -> $ret {")
    lines.append("            log::debug!(\"[FluorateGL] stub {}\", stringify!($name));")
    lines.append("            <$ret>::default()")
    lines.append("        }")
    lines.append("    };")
    lines.append("}")
    lines.append("")
    lines.append("macro_rules! passthrough_alias {")
    lines.append("    ($name:ident, $target:path, $ret:ty, ($($arg:ident: $ty:ty),*)) => {")
    lines.append("        #[unsafe(no_mangle)]")
    lines.append("        #[allow(non_snake_case, unused_variables)]")
    lines.append("        pub extern \"C\" fn $name($($arg: $ty),*) -> $ret {")
    lines.append("            $target($($arg),*)")
    lines.append("        }")
    lines.append("    };")
    lines.append("}")
    lines.append("")

    stub_count = 0
    alias_count = 0
    for name in sorted(missing):
        ret, args_str = missing[name]
        rret, _ = map_c_type(ret)
        params = parse_args(args_str)
        args_rust = ", ".join(f"{n}: {t}" for n, t in params)
        # 空参数
        if not params:
            lines.append(f"stub_fn!({name}, {rret}, ());")
        else:
            lines.append(f"stub_fn!({name}, {rret}, ({args_rust}));")
        stub_count += 1

    # 别名目标 → Rust 路径（按模块）
    TARGET_PATH = {
        "glTexStorage2D": "crate::gl::texture::glTexStorage2D",
        "glTexStorage3D": "crate::gl::texture::glTexStorage3D",
        "glTexBuffer": "crate::gl::texture::glTexBuffer",
        "glTexBufferRange": "crate::gl::texture::glTexBufferRange",
        "glBufferStorage": "crate::gl::buffer::glBufferStorage",
        "glPrimitiveRestartIndex": "crate::gl::drawing::glPrimitiveRestartIndex",
        "glMultiDrawArrays": "crate::gl::multi_draw::glMultiDrawArrays",
        "glMultiDrawElements": "crate::gl::multi_draw::glMultiDrawElements",
        "glMultiDrawElementsBaseVertex": "crate::gl::multi_draw::glMultiDrawElementsBaseVertex",
        "glMultiDrawArraysIndirect": "crate::gl::multi_draw::glMultiDrawArraysIndirect",
        "glMultiDrawElementsIndirect": "crate::gl::multi_draw::glMultiDrawElementsIndirect",
        "glDrawArraysIndirect": "crate::gl::drawing::glDrawArraysIndirect",
        "glDrawElementsIndirect": "crate::gl::drawing::glDrawElementsIndirect",
    }

    for name in sorted(aliases):
        target, (ret, args_str) = aliases[name]
        rret, _ = map_c_type(ret)
        params = parse_args(args_str)
        tpath = TARGET_PATH.get(target, f"crate::gl::exports::{target}")
        args_rust = ", ".join(f"{n}: {t}" for n, t in params)
        if not params:
            lines.append(f"passthrough_alias!({name}, {tpath}, {rret}, ());")
        else:
            lines.append(f"passthrough_alias!({name}, {tpath}, {rret}, ({args_rust}));")
        alias_count += 1

    lines.append("")
    lines.append("#[allow(dead_code)]")
    lines.append(f"pub const STUB_COUNT: usize = {stub_count};")
    lines.append("#[allow(dead_code)]")
    lines.append(f"pub const ALIAS_COUNT: usize = {alias_count};")
    lines.append("")
    open(OUT_RS, "w").write("\n".join(lines))

    # 生成 symbols 段（幂等替换标记区间）
    sym_lines = ["    // ==== GL stub 导出（生成器输出，勿手改）===="]
    for name in sorted(missing):
        sym_lines.append(f'    "{name}",')
    for name in sorted(aliases):
        sym_lines.append(f'    "{name}",')
    sym_lines.append("    // ==== 生成器段结束 ====")
    seg_text = "\n".join(sym_lines)

    s = open(SYMBOLS_RS).read()
    start_marker = "    // ==== GL stub 导出（生成器输出，勿手改）===="
    end_marker = "    // ==== 生成器段结束 ===="
    if start_marker in s:
        s = re.sub(re.escape(start_marker) + ".*?" + re.escape(end_marker),
                   seg_text, s, flags=re.S)
    else:
        # 在 define_symbols! 列表末尾（"];" 前）插入
        idx = s.rfind("];")
        s = s[:idx] + seg_text + "\n" + s[idx:]
    open(SYMBOLS_RS, "w").write(s)

    print(f"缺失（排除固定管线后）: {len(missing)}（core {len(core_missing)} / 后缀 {len(ext_missing)}）")
    print(f"生成 stub: {stub_count}；透传别名: {alias_count}")
    print(f"排除固定管线: {len([n for n in sigs if n not in ours and is_fixed_pipeline(n)])}")

if __name__ == "__main__":
    main()
