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

fn capture_interpreter_stdout(file: &PathBuf, probe_manifest: Option<&PathBuf>) -> String {
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "--quiet", "--", "run"])
        .arg(file)
        .current_dir(workspace_root());
    if let Some(manifest) = probe_manifest {
        cmd.arg("--probes").arg(manifest);
    }
    let output = cmd.output().expect("cargo run");
    assert!(
        output.status.success(),
        "interpreter failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn compile_and_run_python(
    file: &PathBuf,
    out_dir: &PathBuf,
    probe_manifest: Option<PathBuf>,
) -> String {
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
            probe_manifest,
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
        capture_interpreter_stdout(&file, None),
        compile_and_run_python(&file, &out_dir, None)
    );
}

#[test]
fn transpiled_import_demo_matches_interpreter_stdout() {
    let file = workspace_root().join("examples/import_demo.neb");
    let out_dir = std::env::temp_dir().join("nebula-py-parity-import_demo");
    let _ = fs::remove_dir_all(&out_dir);
    assert_eq!(
        capture_interpreter_stdout(&file, None),
        compile_and_run_python(&file, &out_dir, None)
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
        capture_interpreter_stdout(&file, None),
        compile_and_run_python(&file, &out_dir, None),
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
fn parity_integer_arithmetic() {
    assert_parity(
        "int_arithmetic",
        r#"
mission main {
  print(int_to_str(2 plus 3 times 4));
  print(int_to_str(100 minus 7));
  print(int_to_str(0 minus 5));
  print(int_to_str(6 times 7 minus 2));
}
"#,
    );
}

#[test]
fn parity_collection_access() {
    assert_parity(
        "collection_access",
        r#"
mission main {
  let xs: List<Str> = ["a", "b", "c"];
  print(at(xs, 0));
  print(at(xs, 2));
  print(int_to_str(len(xs)));
  let m: Map<Str, Int> = {"x": 10, "y": 20};
  print(int_to_str(get(m, "y")));
  print(int_to_str(len(m)));
  if has(m, "x") then print("has-x"); else print("no-x"); end
  if has(m, "z") then print("has-z"); else print("no-z"); end
}
"#,
    );
}

#[test]
fn parity_numeric_helpers() {
    assert_parity(
        "numeric_helpers",
        r#"
mission main {
  print(int_to_str(abs(0 minus 42)));
  print(int_to_str(abs(7)));
  print(int_to_str(min(3, 9)));
  print(int_to_str(max(3, 9)));
  print(int_to_str(min(0 minus 5, 0 minus 2)));
  print(int_to_str(max(0 minus 5, 0 minus 2)));
}
"#,
    );
}

#[test]
fn parity_split_and_join() {
    assert_parity(
        "split_join",
        r#"
mission main {
  let parts: List<Str> = split("alice,bob,carol", ",");
  print(int_to_str(len(parts)));
  print(at(parts, 1));
  print(join(parts, " | "));
  let pairs: List<Str> = split("a=1;b=2;c=3", ";");
  let mut keys: List<Str> = [];
  let mut i: Int = 0;
  while i lt len(pairs) do
    let kv: List<Str> = split(at(pairs, i), "=");
    push(keys, to_upper(at(kv, 0)));
    set i = i plus 1;
  end
  print(join(keys, ","));
}
"#,
    );
}

#[test]
fn parity_string_operations() {
    assert_parity(
        "string_ops",
        r#"
mission main {
  let s: Str = trim("  Hello, World  ");
  print(s);
  print(to_upper(s));
  print(to_lower(s));
  print(substr(s, 0, 5));
  print(substr(s, 7, 99));
  print(int_to_str(index_of(s, "World")));
  print(int_to_str(index_of(s, "zzz")));
  if contains(s, "World") then print("c1"); else print("c0"); end
  if starts_with(s, "Hello") then print("s1"); else print("s0"); end
  if ends_with(s, "World") then print("e1"); else print("e0"); end
  print(replace(s, "World", "Nebula"));
  print(substr("café", 0, 3));
  print(int_to_str(index_of("café", "é")));
  print(to_upper("café"));
}
"#,
    );
}

#[test]
fn parity_map_insert() {
    assert_parity(
        "map_insert",
        r#"
mission main {
  let mut m: Map<Str, Int> = {"a": 1};
  insert(m, "b", 2);
  insert(m, "a", 9);
  print(int_to_str(get(m, "a")));
  print(int_to_str(get(m, "b")));
  print(int_to_str(len(m)));
  if has(m, "b") then print("has-b"); else print("no-b"); end
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
  if x lt y then print("lt"); else print("ge"); end
}
"#,
    );
}

#[test]
fn bundle_read_file_and_json_parse_match_between_backends() {
    let out_dir = std::env::temp_dir().join("nebula-py-parity-bundle");
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("create out dir");

    let data_path = out_dir.join("data.json");
    fs::write(&data_path, r#"{"status":"ok"}"#).expect("write data");

    let file = out_dir.join("entry.neb");
    let src = format!(
        r#"
mission main {{
  probe read_file(path: Str) -> Str;
  probe json_parse(text: Str) -> Map<Str, Str>;
  let raw: Str = call read_file(path: "{}");
  let cfg: Map<Str, Str> = call json_parse(text: raw);
  print(get(cfg, "status"));
}}
"#,
        data_path.display()
    );
    fs::write(&file, src).expect("write entry");

    let manifest = workspace_root().join("probes/bundle.json");
    assert_eq!(
        capture_interpreter_stdout(&file, Some(&manifest)),
        compile_and_run_python(&file, &out_dir, Some(manifest)),
        "bundle probe parity failed"
    );
}
