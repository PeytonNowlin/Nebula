use nebula_syntax::parse;
use nebula_types::typecheck;

#[test]
fn expression_field_access_typechecks() {
    let src = r#"
sector geo {
  struct Point {
    x: Int;
    y: Int;
  }

  fn origin() -> Point {
    return Point { x: 0, y: 0 };
  }
}

mission main {
  let x: Int = geo.origin().x;
  let p: geo.Point = geo.origin();
  let y: Int = p.y;
  let z: Int = (geo.origin()).x;
}
"#;
    let program = parse(src).expect("parse");
    typecheck(&program).expect("expression field access should typecheck");
}
