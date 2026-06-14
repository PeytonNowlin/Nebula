use nebula_syntax::parse;
use nebula_test_support::{assert_golden, join_errors};
use nebula_types::typecheck;

#[test]
fn golden_parse_unexpected_token() {
    let src = "mission main { if true then }";
    let err = parse(src).expect_err("should fail to parse");
    assert_golden("errors", "parse_unexpected", &err.to_string());
}

#[test]
fn golden_lex_invalid_character() {
    let src = "mission main { let x: Int = @1; }";
    let err = parse(src).expect_err("should fail to lex");
    assert_golden("errors", "lex_invalid_char", &err.to_string());
}

#[test]
fn golden_type_mismatch() {
    let src = r#"
mission main {
  let x: Int = "not an int";
}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("should fail typecheck");
    assert_golden("errors", "type_mismatch", &join_errors(errors.errors()));
}

#[test]
fn golden_duplicate_function() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int { return n times 2; }
  fn double(n: Int) -> Int { return n plus n; }
}
mission main {}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("should fail typecheck");
    assert_golden("errors", "duplicate_function", &join_errors(errors.errors()));
}

#[test]
fn golden_undefined_function() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int { return n times 2; }
}
mission main {
  print(int_to_str(double(1)));
}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("should fail typecheck");
    assert_golden("errors", "undefined_fn", &join_errors(errors.errors()));
}