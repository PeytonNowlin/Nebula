use nebula_syntax::parse;

#[test]
fn parse_hello_mission() {
    let src = r#"
mission main {
  print("Hello from Nebula");
}
"#;
    let program = parse(src).expect("parse failed");
    assert_eq!(program.items.len(), 1);
}

#[test]
fn parse_keyword_operators() {
    let src = r#"
mission main {
  let x: Int = 1 plus 2 times 3;
}
"#;
    parse(src).expect("keyword operators should parse");
}

#[test]
fn parse_sector_and_struct() {
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
  let p: Point = origin();
}
"#;
    parse(src).expect("sector with struct should parse");
}

#[test]
fn parse_probe_and_telemetry() {
    let src = r#"
mission main {
  probe log(level: Str, message: Str) -> Void;

  telemetry {
    call log(level: "info", message: "starting");
    print("done");
  }
}
"#;
    parse(src).expect("probe and telemetry should parse");
}