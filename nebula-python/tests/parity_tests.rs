use std::fs;
use std::path::PathBuf;
use std::process::Command;

use nebula_ir::lower;
use nebula_load::load_workspace;
use nebula_python::{emit_workspace, EmitOptions};
use nebula_syntax::parse;
use nebula_types::typecheck;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn capture_interpreter_stdout(file: &PathBuf) -> String {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "run"])
        .arg(file)
        .current_dir(workspace_root())
        .output()
        .expect("cargo run");
    assert!(
        output.status.success(),
        "interpreter failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn compile_and_run_python(file: &PathBuf, out_dir: &PathBuf) -> String {
    let source = fs::read_to_string(file).expect("read source");
    let program = parse(&source).expect("parse");
    let loaded = load_workspace(file, program).expect("load");
    let typed = typecheck(&loaded.merged).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    let result = emit_workspace(
        &loaded,
        &ir,
        &EmitOptions {
            out_dir: out_dir.clone(),
            entry_path: file.clone(),
            probe_manifest: None,
            telemetry_path: None,
        },
    )
    .expect("emit");

    let output = Command::new("python3")
        .arg(&result.entry_module)
        .output()
        .expect("run python");
    assert!(
        output.status.success(),
        "python run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn transpiled_hello_matches_interpreter_stdout() {
    let file = workspace_root().join("examples/hello.neb");
    let out_dir = std::env::temp_dir().join("nebula-py-parity-hello");
    let _ = fs::remove_dir_all(&out_dir);
    assert_eq!(
        capture_interpreter_stdout(&file),
        compile_and_run_python(&file, &out_dir)
    );
}

#[test]
fn transpiled_import_demo_matches_interpreter_stdout() {
    let file = workspace_root().join("examples/import_demo.neb");
    let out_dir = std::env::temp_dir().join("nebula-py-parity-import_demo");
    let _ = fs::remove_dir_all(&out_dir);
    assert_eq!(
        capture_interpreter_stdout(&file),
        compile_and_run_python(&file, &out_dir)
    );
}