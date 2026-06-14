use nebula_syntax::parse;
use nebula_types::{typecheck, TypeError};

#[test]
fn reject_duplicate_function_in_same_sector() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int { return n times 2; }
  fn double(n: Int) -> Int { return n plus n; }
}

mission main {}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("duplicate function should fail");
    assert!(errors.iter().any(|e| matches!(
        e,
        TypeError::DuplicateSymbol { kind, name, .. }
            if kind == "function" && name == "math.double"
    )));
}

#[test]
fn reject_duplicate_probe_in_mission() {
    let src = r#"
mission main {
  probe log(level: Str, message: Str) -> Void;
  probe log(level: Str, message: Str) -> Void;
}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("duplicate mission probe should fail");
    assert!(errors.iter().any(|e| matches!(
        e,
        TypeError::DuplicateSymbol { kind, name, .. }
            if kind == "probe" && name == "log"
    )));
}

#[test]
fn reject_duplicate_sector_probe() {
    let src = r#"
sector agent {
  probe log(level: Str, message: Str) -> Void;
  probe log(level: Str, message: Str) -> Void;
}

mission main {}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("duplicate sector probe should fail");
    assert!(errors.iter().any(|e| matches!(
        e,
        TypeError::DuplicateSymbol { kind, name, .. }
            if kind == "probe" && name == "agent.log"
    )));
}

#[test]
fn reject_duplicate_sector_name() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int { return n times 2; }
}

sector math {
  fn triple(n: Int) -> Int { return n times 3; }
}

mission main {}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("duplicate sector should fail");
    assert!(errors.iter().any(|e| matches!(
        e,
        TypeError::DuplicateSymbol { kind, name, .. }
            if kind == "sector" && name == "math"
    )));
}

#[test]
fn allow_same_function_name_in_different_sectors() {
    let src = r#"
sector math {
  fn transform(n: Int) -> Int { return n times 2; }
}

sector geo {
  fn transform(n: Int) -> Int { return n plus 1; }
}

mission main {
  print(int_to_str(math.transform(1)));
  print(int_to_str(geo.transform(1)));
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("same short name in different sectors should be allowed");
}