#!/usr/bin/env python3
"""diff_compare.py — 差分测试日志对比器 v1

用法:
  python3 diff_compare.py --ref <日志1> --test <日志2> \
      [--expected expected_diffs.txt] [--report report.md]

逻辑:
  - 按用例 ID 对齐两日志（STEP 行: "STEP <case>.<n> | <op> | ..."）
  - 逐 STEP 对比（操作名 + 值 + err 序列）
  - 按分类判定（日志 STEP 行内嵌 [cls=N]，或 --expected 白名单）:
      [must] 不一致 → FAIL
      [cap]  不一致 → 记录（不 FAIL）
      [exp]  不一致 → 查白名单（用例ID+操作名），命中跳过，未命中 WARN
      [tbd]  不一致 → 待判清单
  - 输出汇总报告（PASS/FAIL/CAP/EXP/TBD 统计 + FAIL 明细）
"""
import argparse
import re
import sys

STEP_RE = re.compile(r"^STEP (\S+)\.(\d+) \| ([^|]+) \| (.*)$")
CLS_RE = re.compile(r"\[cls=(\d)\]")

# op → 分类映射（与 diff_gl_behavior.c 函数表一致；未收录默认 must=0 严格对比）
# 0=must 1=cap 2=exp 3=tbd
CLS_MAP = {
    "glGetString": 2, "glGetStringi": 2,           # 伪造版本/扩展（exp）
    "glGetIntegerv": 0, "glGetBooleanv": 0, "glGetFloatv": 0,
    "glGetError": 0, "check_glGetError": 0,        # 错误队列严格对比
    "glGenBuffers": 0, "glDeleteBuffers": 0, "glBindBuffer": 0,
    "glBufferData": 0, "glBufferSubData": 0, "glGetBufferParameteriv": 0,
    "glGetBufferSubData": 3, "glGetBufferPointerv": 3,
    "glMapBufferRange": 3, "glUnmapBuffer": 3, "glFlushMappedBufferRange": 3,
    "glBufferStorage": 3,
    "glGenTextures": 0, "glDeleteTextures": 0, "glBindTexture": 0,
    "glTexImage2D": 0, "glTexSubImage2D": 0, "glTexParameteri": 0,
    "glCompressedTexImage2D": 3, "glGetTexImage": 3,
    "glGenVertexArrays": 0, "glBindVertexArray": 0, "glDeleteVertexArrays": 0,
    "glEnableVertexAttribArray": 0, "glVertexAttribPointer": 0,
    "glCreateShader": 0, "glShaderSource": 0, "glCompileShader": 0,
    "glGetShaderiv": 0, "glGetShaderInfoLog": 0,
    "glCreateProgram": 0, "glAttachShader": 0, "glLinkProgram": 0,
    "glGetProgramiv": 0, "glGetProgramInfoLog": 0, "glUseProgram": 0,
    "glGetUniformLocation": 0, "glUniform4f": 0, "glUniform1i": 0,
    "glGetShaderSource": 2,
    "glEnable": 0, "glDisable": 0, "glIsEnabled": 0,
    "glViewport": 0, "glClearColor": 0, "glClear": 0, "glScissor": 0,
    "glGenFramebuffers": 0, "glBindFramebuffer": 0, "glDeleteFramebuffers": 0,
    "glFramebufferTexture2D": 0, "glGenRenderbuffers": 0,
    "glBindRenderbuffer": 0, "glRenderbufferStorage": 0,
    "glCheckFramebufferStatus": 0,
    "glDrawArrays": 0, "glDrawElements": 0, "glDrawArraysInstanced": 0,
    "glDrawElementsInstanced": 0, "glDrawRangeElements": 0,
    "glMultiDrawArrays": 3, "glMultiDrawElements": 3,
    "glMultiDrawElementsBaseVertex": 3,
    "glMultiDrawArraysIndirect": 3, "glMultiDrawElementsIndirect": 3,
    "glDrawElementsBaseVertex": 3, "glDrawArraysIndirect": 3,
    "glPrimitiveRestartIndex": 3, "glPolygonMode": 2, "glPointSize": 2,
    "glFenceSync": 0, "glDeleteSync": 0, "glClientWaitSync": 0,
    "glWaitSync": 0, "glIsSync": 0,
    "glGenQueries": 0, "glDeleteQueries": 0,
    "glBeginQuery": 3, "glEndQuery": 3, "glGetQueryiv": 3,
    "glReadPixels": 0, "readPixels_hash": 0,
    "build_program": 0, "state": 0, "h01s": 0,
    "IDMAP": 2, "SYM": 2, "glAlphaFunc": 2, "glClearDepth": 2, "glDrawBuffer": 2,
}



def parse_log(path):
    """返回 {case_id: [ {n, op, text, cls}, ... ]}"""
    cases = {}
    try:
        with open(path, "r", errors="replace") as f:
            for line in f:
                m = STEP_RE.match(line.strip())
                if not m:
                    continue
                case_id = m.group(1)
                n = int(m.group(2))
                op = m.group(3).strip()
                text = m.group(4).strip()
                cm = CLS_RE.search(text)
                cls = int(cm.group(1)) if cm else -1
                cases.setdefault(case_id, []).append(
                    {"n": n, "op": op, "text": text, "cls": cls})
    except FileNotFoundError:
        print(f"错误: 找不到日志文件 {path}", file=sys.stderr)
        sys.exit(2)
    return cases


def load_expected(path):
    """expected_diffs.txt: 每行 "<case_id> <op>" 或 "# 注释" 或空行"""
    if not path:
        return set()
    entries = set()
    try:
        with open(path, "r", errors="replace") as f:
            for line in f:
                line = line.strip()
                # 剥离行内注释与整行注释
                if "#" in line:
                    line = line[: line.index("#")].strip()
                if not line:
                    continue
                entries.add(tuple(line.split(None, 1)))
    except FileNotFoundError:
        print(f"警告: 白名单文件 {path} 不存在", file=sys.stderr)
    return entries


# 对象 ID 字段（绝对值不可比，归一化为 X 比较）
# 覆盖：prog/buf/tex 对象 ID、绑定查询返回值（CURRENT_PROGRAM 等）、sync 指针
ID_RE = re.compile(r"\b(prog|buf|texture|fbo|rbo|vao|vbo|shader|tex|sync|CURRENT_PROGRAM|ARRAY_BUFFER_BINDING|ELEMENT_ARRAY_BUFFER_BINDING|VERTEX_ARRAY_BINDING|TEXTURE_BINDING_2D|DRAW_FRAMEBUFFER_BINDING|READ_FRAMEBUFFER_BINDING|RENDERBUFFER_BINDING|UNIFORM_BUFFER_BINDING)=(?:0x[0-9a-fA-F]+|-?\d+)")


def value_equal(a, b):
    """值比较：哈希/数值/字符串。对象 ID 字段归一化（非零 ID 绝对值不可比）。"""
    if a == b:
        return True
    # ID 归一化：prog=3 vs prog=4 → prog=X vs prog=X（等价）
    na = ID_RE.sub(r"\1=X", a)
    nb = ID_RE.sub(r"\1=X", b)
    if na == nb:
        return True
    # 数值容忍：整数字面量比较（前后缀无关）
    av = a.replace("ret=", "").strip()
    bv = b.replace("ret=", "").strip()
    if av.isdigit() and bv.isdigit() and int(av) == int(bv):
        return True
    return False


def compare(ref_cases, test_cases, expected):
    stats = {"PASS": 0, "FAIL": 0, "CAP": 0, "EXP_OK": 0, "EXP_WARN": 0, "TBD": 0}
    fails = []
    warns = []
    tbds = []
    caps = []

    all_ids = sorted(set(ref_cases) | set(test_cases))
    for cid in all_ids:
        ref = ref_cases.get(cid, [])
        tst = test_cases.get(cid, [])
        if not ref and not tst:
            continue
        if not ref:
            fails.append(f"{cid}: test 端有日志但 ref 端无该用例")
            continue
        if not tst:
            fails.append(f"{cid}: ref 端有日志但 test 端无该用例")
            continue
        # 任一端整个用例 missing（函数缺失）→ 该用例按 EXP 跳过逐步骤对比
        if ref and "(missing" in ref[0]["text"]:
            stats["EXP_OK"] += max(len(ref), len(tst))
            continue
        if tst and "(missing" in tst[0]["text"]:
            stats["EXP_OK"] += max(len(ref), len(tst))
            continue
        n_max = max(len(ref), len(tst))
        for i in range(n_max):
            r = ref[i] if i < len(ref) else None
            t = tst[i] if i < len(tst) else None
            if r is None:
                if (cid, t["op"]) in expected:
                    stats["EXP_OK"] += 1
                    continue
                fails.append(f"{cid}.{i}: test 端多出 STEP op={t['op']}")
                continue
            if t is None:
                if (cid, r["op"]) in expected:
                    stats["EXP_OK"] += 1
                    continue
                fails.append(f"{cid}.{i}: ref 端多出 STEP op={r['op']}")
                continue
            if r["op"] != t["op"]:
                # 任一端函数缺失（missing）→ 记 EXP（函数在 GLES 无对应）
                if "(missing" in r["text"] or "(missing" in t["text"]:
                    stats["EXP_OK"] += 1
                    continue
                # 操作名不同 = 必然差异（exp 白名单检查）
                key = (cid, r["op"])
                if key in expected or (t["op"] and (cid, t["op"]) in expected):
                    stats["EXP_OK"] += 1
                    continue
                fails.append(f"{cid}.{i}: 操作名不同 ref={r['op']} test={t['op']}")
                continue
            if value_equal(r["text"], t["text"]):
                stats["PASS"] += 1
                continue
            # 任一端函数缺失 → EXP
            if "(missing" in r["text"] or "(missing" in t["text"]:
                stats["EXP_OK"] += 1
                continue
            # 值不同 → 按分类判定（优先日志内嵌 cls，其次 CLS_MAP，默认 must）
            cls = r["cls"] if r["cls"] >= 0 else t["cls"]
            if cls < 0:
                cls = CLS_MAP.get(r["op"], 0)
            key = (cid, r["op"])
            if key in expected:
                stats["EXP_OK"] += 1
            elif cls == 0:
                stats["FAIL"] += 1
                fails.append(f"{cid}.{i} [{r['op']}] ref={r['text']} | test={t['text']}")
            elif cls == 1:
                stats["CAP"] += 1
                caps.append(f"{cid}.{i} [{r['op']}] ref={r['text']} | test={t['text']}")
            elif cls == 2:
                stats["EXP_WARN"] += 1
                warns.append(f"{cid}.{i} [{r['op']}] ref={r['text']} | test={t['text']}")
            else:
                stats["TBD"] += 1
                tbds.append(f"{cid}.{i} [{r['op']}] ref={r['text']} | test={t['text']}")

    return stats, fails, warns, tbds, caps


def main():
    ap = argparse.ArgumentParser(description="差分日志对比")
    ap.add_argument("--ref", required=True, help="参照日志（如 desktop 或 gles）")
    ap.add_argument("--test", required=True, help="被测日志（如 translate）")
    ap.add_argument("--expected", default=None, help="expected_diffs.txt 白名单")
    ap.add_argument("--report", default=None, help="输出报告文件（默认 stdout）")
    args = ap.parse_args()

    ref = parse_log(args.ref)
    tst = parse_log(args.test)
    expected = load_expected(args.expected)

    stats, fails, warns, tbds, caps = compare(ref, tst, expected)

    out = []
    out.append(f"# 差分对比报告: {args.ref} vs {args.test}")
    out.append("")
    out.append("| 类别 | 数量 |")
    out.append("|---|---|")
    out.append(f"| PASS（一致） | {stats['PASS']} |")
    out.append(f"| FAIL（must 不一致） | {stats['FAIL']} |")
    out.append(f"| CAP（cap 差异，允许） | {stats['CAP']} |")
    out.append(f"| EXP_OK（exp 白名单命中） | {stats['EXP_OK']} |")
    out.append(f"| EXP_WARN（exp 未在白名单） | {stats['EXP_WARN']} |")
    out.append(f"| TBD（待判） | {stats['TBD']} |")
    out.append("")
    out.append(f"**结论: {'通过' if stats['FAIL'] == 0 else '失败（存在 must 级差异）'}**")
    out.append("")

    if fails:
        out.append("## FAIL 明细（must 级差异，需修复）")
        for f in fails:
            out.append(f"- {f}")
        out.append("")
    if warns:
        out.append("## EXP_WARN 明细（exp 差异未在白名单）")
        for w in warns:
            out.append(f"- {w}")
        out.append("")
    if tbds:
        out.append("## TBD 明细（待判差异）")
        for t in tbds:
            out.append(f"- {t}")
        out.append("")
    if caps:
        out.append("## CAP 明细（能力差异，允许）")
        for c in caps:
            out.append(f"- {c}")
        out.append("")

    report = "\n".join(out)
    if args.report:
        with open(args.report, "w") as f:
            f.write(report + "\n")
        print(f"报告已写入 {args.report}")
    else:
        print(report)

    sys.exit(0 if stats["FAIL"] == 0 else 1)


if __name__ == "__main__":
    main()
