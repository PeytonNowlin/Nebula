use std::path::Path;

use nebula_syntax::parse;
use nebula_types::{report_with_source, typecheck, TypeError};

#[test]
fn typecheck_errors_preserves_all_error_codes_in_report() {
    let src = r#"
sector math {
  fn double(n: Int) -> Int { return n times 2; }
}
mission main {
  print(int_to_str(double(1)));
}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("should fail");
    assert_eq!(errors.errors().len(), 2);

    let report = report_with_source(Path::new("example.neb"), src, errors);
    let rendered = format!("{report:?}");
    assert!(rendered.contains("nebula::typecheck_failed"));
    assert!(rendered.contains("nebula::undefined_fn"));
    assert!(rendered.contains("nebula::type_mismatch"));
    assert!(rendered.contains("NEB-T004"));
    assert!(rendered.contains("NEB-T002"));
}

#[test]
fn typecheck_errors_iter_matches_individual_variants() {
    let src = r#"
mission main {
  print(int_to_str(double(1)));
}
"#;
    let program = parse(src).expect("parse");
    let errors = typecheck(&program).expect_err("should fail");
    assert!(errors
        .iter()
        .any(|err| matches!(err, TypeError::UndefinedFn { .. })));
}
