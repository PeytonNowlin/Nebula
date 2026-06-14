use nebula_syntax::parse;
use nebula_types::{typecheck, TypeError};

#[test]
fn str_plus_str_concatenation_typechecks() {
    let src = r#"
mission main {
  let greeting: Str = "Hello" plus " world";
  print(greeting);
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("Str plus Str should typecheck");
}

#[test]
fn mission_requires_qualified_sector_calls() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int {
    return n times 2;
  }
}

mission main {
  print(int_to_str(double(1)));
}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("unqualified call from mission should fail");
    assert!(errors.iter().any(|e| matches!(e, TypeError::UndefinedFn { .. })));
}

#[test]
fn sector_fn_allows_unqualified_same_sector_calls() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int {
    return n times 2;
  }

  fn quadruple(n: Int) -> Int {
    return double(double(n));
  }
}

mission main {
  print(int_to_str(math.quadruple(2)));
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("same-sector unqualified calls should typecheck");
}