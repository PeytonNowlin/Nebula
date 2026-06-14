use nebula_ir::lower;
use nebula_syntax::parse;
use nebula_test_support::{compile_source, parse_and_typecheck, run_source};
use nebula_types::typecheck;

#[test]
fn typecheck_collects_sector_and_mission_symbols() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int { return n times 2; }
}
mission main {
  print(int_to_str(math.double(2)));
}
"#;
    let typed = parse_and_typecheck(src);
    assert!(typed.functions.contains_key("math.double"));
    assert!(typed.has_main);
}

#[test]
fn lower_builds_sector_functions_and_mission() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int { return n times 2; }
}
mission main {
  print(int_to_str(math.double(3)));
}
"#;
    let ir = compile_source(src);
    let math = ir.sectors.get("math").expect("math sector");
    assert!(math.functions.contains_key("math.double"));
    assert_eq!(math.functions["math.double"].params, vec!["n"]);
    assert!(!ir.mission.stmts.is_empty());
}

#[test]
fn lower_registers_mission_probe() {
    let src = r#"
mission main {
  probe log(level: Str, message: Str) -> Void;
  print("ready");
}
"#;
    let program = parse(src).expect("parse");
    let typed = typecheck(&program).expect("typecheck");
    let ir = lower(&typed).expect("lower");
    assert!(ir.probes.contains_key("log"));
}

#[test]
fn runtime_executes_arithmetic_and_print_path() {
    let src = r#"
mission main {
  let x: Int = 2 plus 3 times 4;
  print(int_to_str(x));
}
"#;
    run_source(src);
}
