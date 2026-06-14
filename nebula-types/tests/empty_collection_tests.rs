use nebula_syntax::parse;
use nebula_types::typecheck;

#[test]
fn empty_list_uses_binding_annotation() {
    let src = r#"
mission main {
  let mut xs: List<Str> = [];
  push(xs, "a");
  print(int_to_str(len(xs)));
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("empty list should match List<Str> annotation");
}

#[test]
fn empty_map_uses_binding_annotation() {
    let src = r#"
mission main {
  let m: Map<Int, Str> = {};
  print("ok");
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("empty map should match Map<Int, Str> annotation");
}

#[test]
fn empty_list_uses_parameter_type() {
    let src = r#"
sector util {
  fn count(xs: List<Str>) -> Int {
    return len(xs);
  }
}

mission main {
  print(int_to_str(util.count([])));
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("empty list argument should use parameter List<Str>");
}

#[test]
fn map_literal_rejects_mismatched_value_type() {
    let src = r#"
mission main {
  let m: Map<Int, Str> = {1: "a", 2: 3};
  print("ok");
}
"#;
    let program = parse(src).expect("parse");
    assert!(
        typecheck(&program).is_err(),
        "map literal with a mismatched value type should fail to typecheck"
    );
}

#[test]
fn map_literal_rejects_mismatched_key_type() {
    let src = r#"
mission main {
  let m: Map<Int, Str> = {1: "a", "two": "b"};
  print("ok");
}
"#;
    let program = parse(src).expect("parse");
    assert!(
        typecheck(&program).is_err(),
        "map literal with a mismatched key type should fail to typecheck"
    );
}

#[test]
fn float_arithmetic_typechecks() {
    let src = r#"
mission main {
  let x: Float = 1.5;
  let y: Float = 2.5;
  let z: Float = x plus y;
  print(float_to_str(z div 2.0));
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("float arithmetic should typecheck");
}

#[test]
fn mixed_int_float_arithmetic_rejected() {
    let src = r#"
mission main {
  let x: Float = 1.5;
  let z: Float = x plus 2;
  print(float_to_str(z));
}
"#;
    let program = parse(src).expect("parse");
    assert!(
        typecheck(&program).is_err(),
        "mixing Int and Float operands should fail (no implicit coercion)"
    );
}

#[test]
fn at_returns_list_element_type() {
    let src = r#"
mission main {
  let xs: List<Str> = ["a"];
  print(at(xs, 0));
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("at should return the list element type (Str)");
}

#[test]
fn get_returns_map_value_type() {
    let src = r#"
mission main {
  let m: Map<Str, Int> = {"a": 1};
  print(int_to_str(get(m, "a")));
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("get should return the map value type (Int)");
}

#[test]
fn get_on_non_map_is_rejected() {
    let src = r#"
mission main {
  let xs: List<Int> = [1];
  print(int_to_str(get(xs, 0)));
}
"#;
    let program = parse(src).expect("parse");
    assert!(typecheck(&program).is_err(), "get on a List should fail to typecheck");
}

#[test]
fn at_with_non_int_index_is_rejected() {
    let src = r#"
mission main {
  let xs: List<Int> = [1];
  print(int_to_str(at(xs, "0")));
}
"#;
    let program = parse(src).expect("parse");
    assert!(typecheck(&program).is_err(), "at with a Str index should fail to typecheck");
}

#[test]
fn insert_typechecks_on_map_variable() {
    let src = r#"
mission main {
  let mut m: Map<Str, Int> = {"a": 1};
  insert(m, "b", 2);
  print(int_to_str(len(m)));
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("insert on a Map<Str,Int> variable should typecheck");
}

#[test]
fn insert_rejects_mismatched_value_type() {
    let src = r#"
mission main {
  let mut m: Map<Str, Int> = {"a": 1};
  insert(m, "b", "two");
  print(int_to_str(len(m)));
}
"#;
    let program = parse(src).expect("parse");
    assert!(
        typecheck(&program).is_err(),
        "insert with a Str value into Map<Str,Int> should fail to typecheck"
    );
}

#[test]
fn insert_rejects_non_map() {
    let src = r#"
mission main {
  let mut xs: List<Int> = [1];
  insert(xs, 0, 2);
  print(int_to_str(len(xs)));
}
"#;
    let program = parse(src).expect("parse");
    assert!(
        typecheck(&program).is_err(),
        "insert on a List should fail to typecheck"
    );
}

#[test]
fn len_accepts_map() {
    let src = r#"
mission main {
  let m: Map<Str, Int> = {"a": 1};
  print(int_to_str(len(m)));
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("len should accept a Map");
}

#[test]
fn empty_list_defaults_without_context() {
    let src = r#"
sector math {
  fn sum(xs: List<Int>) -> Int {
    return len(xs);
  }
}

mission main {
  print(int_to_str(math.sum([])));
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("empty list should default to List<Int> without mismatch");
}