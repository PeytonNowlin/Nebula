use nebula_syntax::parse;

#[test]
fn parse_error_includes_neb_code() {
    let err = parse("mission main { if true then }").expect_err("invalid parse");
    let msg = err.to_string();
    assert!(msg.contains("NEB-S002"), "expected parse error code, got: {msg}");
}

#[test]
fn lex_error_includes_neb_code() {
    let err = parse("mission main { let $: Int = 1; }").expect_err("invalid char");
    let msg = err.to_string();
    assert!(msg.contains("NEB-S001"), "expected lex error code, got: {msg}");
}

#[test]
fn deprecated_less_than_is_rejected_with_canonical_hint() {
    let err = parse("mission main { while x less than 1 do end }").expect_err("synonym");
    let msg = err.to_string();
    assert!(msg.contains("NEB-S004"), "expected deprecated cmp code, got: {msg}");
    assert!(msg.contains("lt"), "expected canonical operator hint, got: {msg}");
    assert!(msg.contains("less than"), "expected found synonym, got: {msg}");
}

#[test]
fn deprecated_greater_than_is_rejected_with_canonical_hint() {
    let err = parse("mission main { while x greater than 1 do end }").expect_err("synonym");
    let msg = err.to_string();
    assert!(msg.contains("NEB-S004"), "expected deprecated cmp code, got: {msg}");
    assert!(msg.contains("gt"), "expected canonical operator hint, got: {msg}");
}

#[test]
fn deprecated_brace_control_flow_is_rejected() {
    let err = parse("mission main { while true do { print(\"x\"); } }").expect_err("brace block");
    let msg = err.to_string();
    assert!(msg.contains("NEB-S005"), "expected deprecated block code, got: {msg}");
    assert!(msg.contains("end"), "expected canonical block hint, got: {msg}");
}

#[test]
fn eof_error_includes_neb_code() {
    let err = parse("mission main { let x: Int = ").expect_err("unexpected eof");
    let msg = err.to_string();
    assert!(msg.contains("NEB-S003"), "expected eof error code, got: {msg}");
}