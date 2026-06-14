use std::fs;
use std::path::PathBuf;
use std::process::Command;

use nebula_ir::lower;
use nebula_load::load_workspace;
use nebula_python::{emit_workspace, EmitOptions};
use nebula_syntax::parse;
use nebula_types::typecheck;

fn compile_and_run(src: &str, out_dir: &PathBuf) -> std::process::Output {
    fs::create_dir_all(out_dir).expect("create out dir");
    let entry = out_dir.join("entry.neb");
    fs::write(&entry, src).expect("write entry");
    let source = fs::read_to_string(&entry).expect("read entry");
    let program = parse(&source).expect("parse");
    let loaded = load_workspace(&entry, program).expect("load");
    let typed = typecheck(&loaded.merged).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    let result = emit_workspace(
        &loaded,
        &ir,
        &EmitOptions {
            out_dir: out_dir.clone(),
            entry_path: entry.clone(),
            probe_manifest: None,
            telemetry_path: None,
        },
    )
    .expect("emit");
    Command::new("python3")
        .arg(result.entry_module)
        .output()
        .expect("run python")
}

#[test]
fn transpiled_div_by_zero_reports_neb_r004() {
    let out_dir = std::env::temp_dir().join("nebula-py-div-zero");
    let _ = fs::remove_dir_all(&out_dir);
    let output = compile_and_run(
        r#"
mission main {
  let x: Int = 1 div 0;
  print(int_to_str(x));
}
"#,
        &out_dir,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("NEB-R004"), "stderr: {stderr}");
}
