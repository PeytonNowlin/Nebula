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
fn parse_expression_field_access() {
    let src = r#"
sector geo {
  struct Point { x: Int; y: Int; }
  fn origin() -> Point { return Point { x: 0, y: 0 }; }
}
mission main {
  let x: Int = geo.origin().x;
}
"#;
    parse(src).expect("expression field access should parse");
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
  let p: geo.Point = geo.origin();
}
"#;
    parse(src).expect("sector with struct should parse");
}

#[test]
fn parse_probe_call_in_expression() {
    let src = r#"
mission main {
  probe fetch_status(url: Str) -> Int;
  let status: Int = call fetch_status(url: "https://example.com");
}
"#;
    parse(src).expect("probe call in expression should parse");
}

#[test]
fn parse_if_while_with_end() {
    let src = r#"
mission main {
  let mut i: Int = 0;

  while i lt 3 do
    set i = i plus 1;
  end

  if i eq 3 then
    print("ok");
  else
    print("no");
  end
}
"#;
    parse(src).expect("if/while with end should parse");
}

#[test]
fn parse_nested_if_with_end() {
    let src = r#"
mission main {
  let x: Int = 1;

  if x eq 1 then
    if x lt 5 then
      print("nested");
    end
  end
}
"#;
    parse(src).expect("nested if with end should parse");
}

#[test]
fn parse_end_blocks_for_control_flow() {
    let src = r#"
mission main {
  while true do
    print("loop");
  end
}
"#;
    parse(src).expect("end blocks should parse");
}

#[test]
fn parse_telemetry_with_end() {
    let src = r#"
mission main {
  telemetry
    print("trace");
  end
}
"#;
    parse(src).expect("telemetry with end should parse");
}

#[test]
fn parse_import_with_semicolon() {
    let src = r#"
import "../std/math.neb";

mission main {
  print("ok");
}
"#;
    parse(src).expect("import with semicolon should parse");
}

#[test]
fn parse_probe_and_telemetry() {
    let src = r#"
mission main {
  probe log(level: Str, message: Str) -> Void;

  telemetry
    call log(level: "info", message: "starting");
    print("done");
  end
}
"#;
    parse(src).expect("probe and telemetry should parse");
}
