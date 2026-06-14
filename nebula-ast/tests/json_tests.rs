use nebula_syntax::parse;

#[test]
fn ast_serializes_to_json_with_spans() {
    let src = r#"
mission main {
  let x: Int = 1;
}
"#;
    let program = parse(src).expect("parse");
    let json = serde_json::to_value(&program).expect("serialize ast");
    assert!(json["items"].is_array());
    let mission = &json["items"][0]["node"]["Mission"];
    assert_eq!(mission["node"]["name"]["node"].as_str().unwrap(), "main");
    assert!(mission["node"]["name"]["span"]["start"].is_number());
}