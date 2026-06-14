use nebula_fmt::format;

#[test]
fn fmt_roundtrip_is_stable() {
    let src = r#"
mission main {
  let x: Int = 1 plus 2;
  print(int_to_str(x));
}
"#;
    let once = format(src).expect("format");
    let twice = format(&once).expect("re-format");
    assert_eq!(once, twice);
}

#[test]
fn fmt_canonicalizes_brace_control_flow_to_end() {
    let src = r#"
mission main {
  while true do {
    print("loop");
  }
}
"#;
    let formatted = format(src).expect("format");
    assert!(formatted.contains("while true do\n"));
    assert!(formatted.contains("end\n"));
    assert!(!formatted.contains("do {"));
}