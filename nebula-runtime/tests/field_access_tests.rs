use nebula_ir::lower;
use nebula_syntax::parse;
use nebula_types::typecheck;
use nebula_runtime::Runtime;

#[test]
fn expression_field_access_evaluates() {
    let src = r#"
sector geo {
  struct Point {
    x: Int;
    y: Int;
  }

  fn origin() -> Point {
    return Point { x: 42, y: 0 };
  }
}

mission main {
  print(int_to_str(geo.origin().x));
}
"#;
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    let mut runtime = Runtime::new(&ir);
    runtime.run(&ir).expect("run");
}