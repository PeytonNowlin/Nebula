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
fn parse_if_while_with_end() {
    let src = r#"
mission main {
  let mut i: Int = 0;

  while i less than 3 do
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
fn parse_brace_blocks_still_work() {
    let src = r#"
mission main {
  while true do {
    print("loop");
  }
}
"#;
    parse(src).expect("brace blocks should still parse");
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

  telemetry {
    call log(level: "info", message: "starting");
    print("done");
  }
}
"#;
    parse(src).expect("probe and telemetry should parse");
}