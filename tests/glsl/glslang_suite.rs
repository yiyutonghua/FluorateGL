//! Run the glslang test suite through FluorateGL's shader translator.
//!
//! 遍历 `tests/glsl/glslang/Test` 下的所有 `.vert`/`.frag`/`.comp`/`.geom`/`.tesc`/`.tese`
//! 文件，统计翻译成功/失败，并在 GLES 后端可用时额外验证翻译后的 GLSL ES 能否编译。
//!
//! 架构：
//!   - worker 进程（fork）：只做翻译，将翻译后源码写到 stdout，退出码标识结果。
//!     用 fork 隔离是因为 glslang 的 C++ assertion failure 会 abort()，
//!     catch_unwind 无法捕获，必须进程级隔离。
//!   - 主进程：收集 worker 结果，对翻译成功的 shader 用 GLES 后端做编译检查。
//!
//! 退出码（worker）：
//!   0 = 翻译成功（stdout 含翻译后 GLSL ES 源码）
//!   1 = 翻译失败（glslang SPIR-V 编译或 spirv-cross 失败）
//!   2 = 翻译透传
//!
//! 环境变量：
//!   FLUORATEGL_BACKEND=llvmpipe  使用 Mesa llvmpipe 软件后端
//!   EGL_PLATFORM=surfaceless     无显示器环境（CI）
//!   MESA_LOADER_DRIVER_OVERRIDE=llvmpipe  强制 llvmpipe 驱动
//!
//! 用法：
//!   cargo run --example glslang_suite
//!   bash tests/run_glslang_suite.sh

use fluorategl::shader_translator::spirv_pass::{TranslationResult, translate};
use std::fs;
use std::io::Write;
use std::path::Path;

const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_GEOMETRY_SHADER: u32 = 0x8DD9;
const GL_TESS_CONTROL_SHADER: u32 = 0x8E88;
const GL_TESS_EVALUATION_SHADER: u32 = 0x8E87;
const GL_COMPUTE_SHADER: u32 = 0x91B9;

fn stage_for_path(path: &Path) -> Option<(u32, &'static str)> {
    match path.extension()?.to_str()? {
        "vert" => Some((GL_VERTEX_SHADER, "vertex")),
        "frag" => Some((GL_FRAGMENT_SHADER, "fragment")),
        "geom" => Some((GL_GEOMETRY_SHADER, "geometry")),
        "tesc" => Some((GL_TESS_CONTROL_SHADER, "tess_control")),
        "tese" => Some((GL_TESS_EVALUATION_SHADER, "tess_eval")),
        "comp" => Some((GL_COMPUTE_SHADER, "compute")),
        _ => None,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // worker 模式：翻译单个 shader，源码写到 stdout
    if args.len() == 4 && args[1] == "--worker" {
        let path = &args[2];
        let stage: u32 = args[3].parse().expect("invalid stage");
        // worker 不需要 GLES 上下文，只做翻译
        let _ = fluorategl::fluorategl_init();
        let bytes = fs::read(path).unwrap_or_default();
        let source = String::from_utf8_lossy(&bytes);
        match translate(&source, stage) {
            TranslationResult::Translated(src) => {
                // stdout 写翻译后源码
                let _ = std::io::stdout().write_all(src.as_bytes());
                std::process::exit(0);
            }
            TranslationResult::PassThrough => std::process::exit(2),
            TranslationResult::Failed => std::process::exit(1),
        }
    }

    let suite_dir = Path::new("tests/glsl/glslang/Test");
    if !suite_dir.is_dir() {
        eprintln!("glslang test suite not found at {:?}", suite_dir);
        std::process::exit(1);
    }

    // 主进程初始化（用于 GLES 编译检查）
    if fluorategl::fluorategl_init() != 0 {
        eprintln!("[glslang] fluorategl_init failed");
        std::process::exit(1);
    }
    let gles_available = fluorategl::ensure_gles_context();
    eprintln!(
        "[glslang] GLES backend for compile test: {}",
        if gles_available {
            "available"
        } else {
            "unavailable"
        }
    );

    let exe = std::env::current_exe().expect("could not get current executable path");

    let mut translated_ok = 0usize;
    let mut compile_fail = 0usize;
    let mut no_gles = 0usize;
    let mut passed_through = 0usize;
    let mut translate_failed = 0usize;
    let mut crashed = 0usize;
    let mut skipped = 0usize;
    let mut compile_fail_samples: Vec<String> = Vec::new();
    let mut translate_fail_samples: Vec<String> = Vec::new();
    let mut crash_samples: Vec<String> = Vec::new();
    // 全量清单 dump（环境变量 GLSLANG_DUMP_FAILURES=<dir> 时，把完整失败清单写入 dir 下文件，
    // 用于回归对比；suite 默认只打印前 30 个样本）
    let dump_dir = std::env::var("GLSLANG_DUMP_FAILURES").ok();

    // 收集所有测试文件并排序，保证输出顺序稳定
    let mut files: Vec<_> = fs::read_dir(suite_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let total_files = files.len();
    let mut processed = 0usize;

    for path in &files {
        let Some((stage, stage_name)) = stage_for_path(path) else {
            skipped += 1;
            continue;
        };

        processed += 1;
        if processed % 200 == 0 {
            eprintln!("[glslang] progress: {}/{}", processed, total_files);
        }

        // fork worker 做翻译，防止单个 shader 的 C++ assertion/crash 影响整体
        // worker 只做翻译（纯 CPU），不需要 EGL/GLES，跳过后端加载以加速启动
        let output = std::process::Command::new(&exe)
            .arg("--worker")
            .arg(path)
            .arg(stage.to_string())
            .env("FLUORATEGL_SKIP_BACKEND", "1")
            .env("FLUORATEGL_LOG", "error")
            .output();

        let output = match output {
            Ok(o) => o,
            Err(e) => {
                eprintln!(
                    "[glslang] spawn worker failed for {}: {}",
                    path.display(),
                    e
                );
                crashed += 1;
                continue;
            }
        };

        match output.status.code() {
            Some(0) => {
                // 翻译成功，stdout 含翻译后源码
                let translated_src = String::from_utf8_lossy(&output.stdout);
                if !gles_available {
                    no_gles += 1;
                } else {
                    match fluorategl::gles_compile_check(&translated_src, stage) {
                        Ok(()) => translated_ok += 1,
                        Err(e) => {
                            compile_fail += 1;
                            if compile_fail_samples.len() < 30 {
                                compile_fail_samples.push(format!(
                                    "{} ({})  [{}]",
                                    path.display(),
                                    stage_name,
                                    e.chars().take(200).collect::<String>()
                                ));
                            }
                            if let Some(d) = &dump_dir {
                                let _ = std::fs::create_dir_all(d);
                                let mut f = std::fs::OpenOptions::new()
                                    .create(true)
                                    .append(true)
                                    .open(format!("{}/compile_fail.txt", d))
                                    .unwrap();
                                let _ = writeln!(
                                    f,
                                    "{} ({})  [{}]",
                                    path.display(),
                                    stage_name,
                                    e.trim().replace('\n', " | ")
                                );
                            }
                        }
                    }
                }
            }
            Some(1) => {
                translate_failed += 1;
                if translate_fail_samples.len() < 30 {
                    translate_fail_samples.push(format!("{} ({})", path.display(), stage_name));
                }
                if let Some(d) = &dump_dir {
                    let _ = std::fs::create_dir_all(d);
                    let mut f = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(format!("{}/translate_fail.txt", d))
                        .unwrap();
                    let _ = writeln!(f, "{} ({})", path.display(), stage_name);
                }
            }
            Some(2) => passed_through += 1,
            Some(c) => {
                eprintln!(
                    "[glslang] worker exited with code {}: {} ({})",
                    c,
                    path.display(),
                    stage_name
                );
                crashed += 1;
                if crash_samples.len() < 30 {
                    crash_samples.push(format!(
                        "{} ({}) [exit code {}]",
                        path.display(),
                        stage_name,
                        c
                    ));
                }
                if let Some(d) = &dump_dir {
                    let _ = std::fs::create_dir_all(d);
                    let mut f = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(format!("{}/crash.txt", d))
                        .unwrap();
                    let _ = writeln!(f, "{} ({}) [exit code {}]", path.display(), stage_name, c);
                }
            }
            None => {
                // SIGABRT (assertion) 或 SIGSEGV
                eprintln!(
                    "[glslang] worker CRASHED (signal): {} ({})",
                    path.display(),
                    stage_name
                );
                crashed += 1;
                if crash_samples.len() < 30 {
                    crash_samples.push(format!(
                        "{} ({}) [signal crash]",
                        path.display(),
                        stage_name
                    ));
                }
                if let Some(d) = &dump_dir {
                    let _ = std::fs::create_dir_all(d);
                    let mut f = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(format!("{}/crash.txt", d))
                        .unwrap();
                    let _ = writeln!(f, "{} ({}) [signal crash]", path.display(), stage_name);
                }
            }
        }
    }

    let translated_total = translated_ok + compile_fail + no_gles;

    println!();
    println!("═══════════════════════════════════════════════════════════");
    println!("  glslang test suite 翻译结果报告");
    println!("═══════════════════════════════════════════════════════════");
    println!("  测试文件总数:     {}", total_files);
    println!(
        "  已处理:           {} (跳过 {} 非目标扩展名)",
        processed, skipped
    );
    println!(
        "  GLES 编译后端:    {}",
        if gles_available {
            "llvmpipe/可用"
        } else {
            "不可用"
        }
    );
    println!("───────────────────────────────────────────────────────────");
    println!("  翻译成功:         {}", translated_total);
    if gles_available {
        println!("    ├─ 编译通过:    {}", translated_ok);
        println!("    └─ 编译失败:    {}", compile_fail);
    } else {
        println!("    └─ 无 GLES 后端，未做编译验证: {}", no_gles);
    }
    println!("  翻译透传:         {}", passed_through);
    println!("  翻译失败:         {}", translate_failed);
    println!("  崩溃(assert/segv): {}", crashed);
    println!("═══════════════════════════════════════════════════════════");

    if !translate_fail_samples.is_empty() {
        println!();
        println!("翻译失败样本 (前 {} 个):", translate_fail_samples.len());
        for s in &translate_fail_samples {
            println!("  ✗ {}", s);
        }
    }

    if !compile_fail_samples.is_empty() {
        println!();
        println!("GLES 编译失败样本 (前 {} 个):", compile_fail_samples.len());
        for s in &compile_fail_samples {
            println!("  ✗ {}", s);
        }
    }

    if !crash_samples.is_empty() {
        println!();
        println!("崩溃样本 (前 {} 个):", crash_samples.len());
        for s in &crash_samples {
            println!("  💥 {}", s);
        }
    }
}
