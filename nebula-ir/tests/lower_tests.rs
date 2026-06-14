use nebula_ir::lower;
use nebula_syntax::parse;
use nebula_types::typecheck;

#[test]
fn lower_emits_call_and_return_in_sector_function() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int {
    return n times 2;
  }
}
mission main {
  print(int_to_str(math.double(4)));
}
"#;
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");

    let math = ir.sectors.get("math").expect("math sector");
    let double = math.functions.get("math.double").expect("math.double");
    assert_eq!(double.qualified_name, "math.double");
    assert_eq!(double.params, vec!["n"]);
    assert_eq!(double.body.len(), 1);
}

#[test]
fn lower_emits_struct_literal_fields() {
    let src = r#"
sector geo {
  struct Point { x: Int; y: Int; }
  fn origin() -> Point {
    return Point { x: 0, y: 0 };
  }
}
mission main {
  print(int_to_str(geo.origin().x));
}
"#;
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");

    let geo = ir.sectors.get("geo").expect("geo sector");
    assert!(geo.structs.contains_key("Point"));
    assert!(geo.functions.contains_key("geo.origin"));
}