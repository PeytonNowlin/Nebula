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