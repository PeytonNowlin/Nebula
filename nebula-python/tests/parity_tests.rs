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

/// Write `src` to a temp `.neb`, run it through the interpreter and the Python
/// backend, and assert their stdout matches. This is the regression net for
/// interpreter/transpiler semantic parity.
fn assert_parity(slug: &str, src: &str) {
    let out_dir = std::env::temp_dir().join(format!("nebula-py-parity-{slug}"));
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");
    let file = out_dir.join("entry.neb");
    fs::write(&file, src).expect("write entry");
    assert_eq!(
        capture_interpreter_stdout(&file),
        compile_and_run_python(&file, &out_dir),
        "interpreter and Python backend diverged for `{slug}`"
    );
}

#[test]
fn parity_integer_div_mod_truncates_toward_zero() {
    // Negative operands: floor division would give -4 / 1, truncation gives -3 / -1.
    assert_parity(
        "div_mod_negative",
        r#"
mission main {
  let a: Int = 0 minus 7;
  print(int_to_str(a div 2));
  print(int_to_str(a mod 2));
  print(int_to_str(7 div 2));
  print(int_to_str(7 mod 2));
}
"#,
    );
}

#[test]
fn parity_structural_equality_on_composites() {
    assert_parity(
        "composite_eq",
        r#"
mission main {
  let a: List<Int> = [1, 2, 3];
  let b: List<Int> = [1, 2, 3];
  let c: List<Int> = [1, 2, 4];
  if a eq b then print("list-eq"); else print("list-ne"); end
  if a eq c then print("list-eq2"); else print("list-ne2"); end
  let m: Map<Str, Int> = {"a": 1, "b": 2};
  let n: Map<Str, Int> = {"b": 2, "a": 1};
  if m eq n then print("map-eq"); else print("map-ne"); end
}
"#,
    );
}

#[test]
fn parity_len_counts_code_points() {
    assert_parity(
        "len_unicode",
        r#"
mission main {
  print(int_to_str(len("café")));
  print(int_to_str(len("naïve résumé")));
}
"#,
    );
}

#[test]
fn parity_float_arithmetic_and_conversions() {
    assert_parity(
        "float_ops",
        r#"
mission main {
  let x: Float = 1.5;
  let y: Float = 2.5;
  print(float_to_str(x plus y));
  print(float_to_str(7.0 div 2.0));
  print(float_to_str(0.0 minus 7.5 mod 2.0));
  print(float_to_str(int_to_float(3) times 2.0));
  print(int_to_str(float_to_int(3.9)));
  print(float_to_str(str_to_float("2.25") plus 0.25));
  if x less than y then print("lt"); else print("ge"); end
}
"#,
    );
}