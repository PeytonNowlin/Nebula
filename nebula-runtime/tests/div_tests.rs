use nebula_ir::lower;
use nebula_runtime::Runtime;
use nebula_syntax::parse;
use nebula_types::typecheck;

fn run_expect_err(src: &str) -> String {
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    let mut runtime = Runtime::new(&ir);
    runtime.run(&ir).expect_err("run should fail").to_string()
}

#[test]
fn div_by_zero_reports_neb_r004() {
    let err = run_expect_err(
        r#"
mission main {
  let x: Int = 1 div 0;
  print(int_to_str(x));
}
"#,
    );
    assert!(err.contains("NEB-R004"), "expected divide-by-zero code, got: {err}");
    assert!(err.contains("division by zero"), "expected clear message, got: {err}");
}

#[test]
fn mod_by_zero_reports_neb_r004() {
    let err = run_expect_err(
        r#"
mission main {
  let x: Int = 5 mod 0;
  print(int_to_str(x));
}
"#,
    );
    assert!(err.contains("NEB-R004"), "expected divide-by-zero code, got: {err}");
    assert!(err.contains("division by zero"), "expected clear message, got: {err}");
}

#[test]
fn float_div_by_zero_reports_neb_r004() {
    let err = run_expect_err(
        r#"
mission main {
  let x: Float = 1.0 div 0.0;
  print(float_to_str(x));
}
"#,
    );
    assert!(err.contains("NEB-R004"), "expected divide-by-zero code, got: {err}");
}

#[test]
fn float_mod_by_zero_reports_neb_r004() {
    let err = run_expect_err(
        r#"
mission main {
  let x: Float = 1.0 mod 0.0;
  print(float_to_str(x));
}
"#,
    );
    assert!(err.contains("NEB-R004"), "expected divide-by-zero code, got: {err}");
}

#[test]
fn list_index_out_of_bounds_reports_neb_r005() {
    let err = run_expect_err(
        r#"
mission main {
  let xs: List<Int> = [1, 2];
  print(int_to_str(at(xs, 5)));
}
"#,
    );
    assert!(err.contains("NEB-R005"), "expected index-out-of-bounds code, got: {err}");
}

#[test]
fn negative_list_index_reports_neb_r005() {
    let err = run_expect_err(
        r#"
mission main {
  let xs: List<Int> = [1, 2];
  print(int_to_str(at(xs, 0 minus 1)));
}
"#,
    );
    assert!(err.contains("NEB-R005"), "expected index-out-of-bounds code, got: {err}");
}

#[test]
fn missing_map_key_reports_neb_r006() {
    let err = run_expect_err(
        r#"
mission main {
  let m: Map<Str, Int> = {"a": 1};
  print(int_to_str(get(m, "missing")));
}
"#,
    );
    assert!(err.contains("NEB-R006"), "expected key-not-found code, got: {err}");
}