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
fn eof_error_includes_neb_code() {
    let err = parse("mission main { let x: Int = ").expect_err("unexpected eof");
    let msg = err.to_string();
    assert!(msg.contains("NEB-S003"), "expected eof error code, got: {msg}");
}