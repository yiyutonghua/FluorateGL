//! Run the glslang test suite through FluorateGL's shader translator.
//!
//! This example does not require a GLES context. It initialises the library so
//! that capability probing is done once, then translates every `.vert`/`.frag`
//!/`.comp`/`.geom`/`.tesc`/`.tese` file under `tests/glsl/glslang/Test` and
//! reports how many succeeded, were passed through, or failed.
//!
//! Many glslang tests are intentionally negative or use features we cannot
//! translate, so the harness exits successfully even when individual shaders
//! fail translation. To keep a single bad shader from crashing the whole run,
//! each shader is processed in a forked worker process.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

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

fn translate_single(path: &Path, stage: u32) -> i32 {
    let bytes = fs::read(path).unwrap();
    let source = String::from_utf8_lossy(&bytes);

    let result = fluorategl::shader_translator::spirv_pass::translate(&source, stage);
    match result {
        fluorategl::shader_translator::spirv_pass::TranslationResult::Translated(_) => 0,
        fluorategl::shader_translator::spirv_pass::TranslationResult::PassThrough => 2,
        fluorategl::shader_translator::spirv_pass::TranslationResult::Failed => 1,
    }
}

fn worker(path: &str, stage: u32) {
    if fluorategl::fluorategl_init() != 0 {
        eprintln!("fluorategl_init failed");
        std::process::exit(2);
    }
    std::process::exit(translate_single(Path::new(path), stage));
}

fn run_worker(exe: &Path, path: &Path, stage: u32) -> Option<i32> {
    let status = Command::new(exe)
        .arg("--worker")
        .arg(path)
        .arg(stage.to_string())
        .status();

    match status {
        Ok(s) => s.code(),
        Err(e) => {
            eprintln!("[glslang] failed to spawn worker for {}: {}", path.display(), e);
            Some(-1)
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 4 && args[1] == "--worker" {
        let path = &args[2];
        let stage: u32 = args[3].parse().expect("invalid stage");
        worker(path, stage);
        return;
    }

    let suite_dir = Path::new("tests/glsl/glslang/Test");
    if !suite_dir.is_dir() {
        eprintln!("glslang test suite not found at {:?}", suite_dir);
        std::process::exit(1);
    }

    let exe = env::current_exe().expect("could not get current executable path");

    let mut translated = 0usize;
    let mut passed_through = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut crashed = 0usize;

    for entry in fs::read_dir(suite_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some((stage, stage_name)) = stage_for_path(&path) else {
            skipped += 1;
            continue;
        };

        let code = run_worker(&exe, &path, stage);

        match code {
            Some(0) => {
                translated += 1;
            }
            Some(2) => {
                passed_through += 1;
            }
            Some(1) => {
                failed += 1;
                eprintln!(
                    "[glslang] translate failed: {} (stage {})",
                    path.display(),
                    stage_name
                );
            }
            Some(c) => {
                failed += 1;
                eprintln!(
                    "[glslang] worker exited with code {}: {} (stage {})",
                    c, path.display(), stage_name
                );
            }
            None => {
                crashed += 1;
                eprintln!(
                    "[glslang] worker crashed (SIGSEGV etc.): {} (stage {})",
                    path.display(),
                    stage_name
                );
            }
        }
    }

    let total = translated + passed_through + failed + skipped;
    if total == 0 {
        eprintln!("no glslang test files were processed");
        std::process::exit(1);
    }

    println!(
        "[glslang] total={} translated={} passed_through={} failed={} crashed={} skipped={}",
        total, translated, passed_through, failed, crashed, skipped
    );
}
