use std::path::Path;

use nebula_diagnostics::diagnostics_from_report_with_source;
use nebula_syntax::parse;
use nebula_types::{diagnostics_from_type_errors, report_with_source, typecheck};

#[test]
fn typed_errors_use_neb_message_not_display_templates() {
    let src = r#"
mission main {
  let x: Int = "not an int";
}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("typecheck");
    let diags = diagnostics_from_type_errors(Path::new("example.neb"), src, &errors);

    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "NEB-T002");
    assert!(diags[0].message.contains("type mismatch"));
    assert!(!diags[0].message.starts_with("NEB-"));
    assert!(!diags[0].message.contains('['));
    let span = diags[0].span.as_ref().expect("span");
    assert_eq!(span.file.as_deref(), Some("example.neb"));
    assert!(span.start < span.end);
    assert_eq!(span.line, Some(3));
}

#[test]
fn multiple_type_errors_emit_one_record_per_error() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int { return n times 2; }
}
mission main {
  print(int_to_str(double(1)));
}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("typecheck");
    let diags = diagnostics_from_type_errors(Path::new("example.neb"), src, &errors);

    assert_eq!(diags.len(), 2);
    assert!(diags.iter().any(|d| d.code == "NEB-T004"));
    assert!(diags.iter().any(|d| d.code == "NEB-T002"));
}

#[test]
fn report_fallback_resolves_code_from_miette_not_display() {
    let src = "mission main { let x: Int = \"nope\"; }";
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("typecheck");
    let report = report_with_source(Path::new("bad.neb"), src, errors);
    let diags = diagnostics_from_report_with_source(&report, Some(src));

    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "NEB-T002");
}

#[test]
fn json_roundtrip_is_valid_array() {
    let src = "mission main { let x: Int = \"nope\"; }";
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("typecheck");
    let diags = diagnostics_from_type_errors(Path::new("bad.neb"), src, &errors);
    let json = serde_json::to_string(&diags).expect("json");
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).expect("parse json");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["code"], "NEB-T002");
    assert!(parsed[0]["span"].is_object());
    assert!(parsed[0]["message"].is_string());
}