use std::error::Error as StdError;
use std::path::Path;

use miette::{Diagnostic, LabeledSpan, Report, SourceSpan as MietteSourceSpan};
use nebula_ast::{neb_code_from_miette, NebError};
use nebula_ir::IrError;
use nebula_load::LoadError;
use nebula_runtime::RuntimeError;
use nebula_syntax::{LexError, ParseError};
use nebula_types::{TypeError, TypecheckErrors};
use serde::Serialize;

/// Byte span with optional 1-based line/column when source text is available.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticSpan {
    pub file: Option<String>,
    pub start: usize,
    pub end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
}

/// Agent-oriented diagnostic record emitted by `nebula check --json` and `nebula run --json`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticJson {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<DiagnosticSpan>,
    pub message: String,
}

pub fn diagnostics_from_report(report: &Report) -> Vec<DiagnosticJson> {
    diagnostics_from_report_with_source(report, None)
}

pub fn diagnostics_from_report_with_source(
    report: &Report,
    fallback_source: Option<&str>,
) -> Vec<DiagnosticJson> {
    let (file, source) = source_context(report, fallback_source);
    let source = source.as_deref();
    let file = file.as_deref();
    let mut out = Vec::new();

    if try_extract_structured(report, source, file, &mut out) {
        return out;
    }

    collect_miette_diagnostic(&**report, source, file, &mut out);
    if out.is_empty() {
        push_fallback(report, &mut out);
    }
    out
}

pub fn emit_json_diagnostics(diagnostics: &[DiagnosticJson]) {
    let json = serde_json::to_string(diagnostics).expect("serialize diagnostics");
    eprintln!("{json}");
}

pub fn diagnostics_from_type_errors(
    path: impl AsRef<Path>,
    source: &str,
    errors: &TypecheckErrors,
) -> Vec<DiagnosticJson> {
    let file = path.as_ref().display().to_string();
    let mut out = Vec::new();
    push_neb_errors(errors.errors(), Some(source), Some(&file), &mut out);
    out
}

fn try_extract_structured(
    report: &Report,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) -> bool {
    for cause in report.chain() {
        if try_downcast_cause(cause, source, file, out) {
            return true;
        }
    }
    false
}

fn try_downcast_cause(
    cause: &(dyn StdError + 'static),
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) -> bool {
    if let Some(errors) = cause.downcast_ref::<TypecheckErrors>() {
        push_neb_errors(errors.errors(), source, file, out);
        return true;
    }
    if let Some(load_err) = cause.downcast_ref::<LoadError>() {
        return push_load_error(load_err, source, file, out);
    }
    if let Some(parse_err) = cause.downcast_ref::<ParseError>() {
        push_neb_error(parse_err, source, file, out);
        return true;
    }
    if let Some(lex_err) = cause.downcast_ref::<LexError>() {
        push_neb_error(lex_err, source, file, out);
        return true;
    }
    if let Some(type_err) = cause.downcast_ref::<TypeError>() {
        push_neb_error(type_err, source, file, out);
        return true;
    }
    if let Some(runtime_err) = cause.downcast_ref::<RuntimeError>() {
        push_neb_error(runtime_err, source, file, out);
        return true;
    }
    if let Some(ir_err) = cause.downcast_ref::<IrError>() {
        push_neb_error(ir_err, source, file, out);
        return true;
    }
    false
}

fn push_load_error(
    load_err: &LoadError,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) -> bool {
    if let LoadError::Parse { path, source: parse_err, .. } = load_err {
        let imported_source = std::fs::read_to_string(path).ok();
        let imported_file = path.display().to_string();
        push_neb_error(
            parse_err,
            imported_source.as_deref(),
            Some(imported_file.as_str()),
            out,
        );
        return true;
    }
    push_neb_error(load_err, source, file, out);
    true
}

fn push_neb_errors<E: NebError>(
    errors: impl IntoIterator<Item = E>,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) {
    for err in errors {
        push_neb_error(&err, source, file, out);
    }
}

fn push_neb_error(err: &impl NebError, source: Option<&str>, file: Option<&str>, out: &mut Vec<DiagnosticJson>) {
    out.push(diagnostic_json(err, source, file));
}

fn diagnostic_json(err: &impl NebError, source: Option<&str>, file: Option<&str>) -> DiagnosticJson {
    DiagnosticJson {
        code: err.neb_code().to_string(),
        span: err
            .neb_span()
            .map(|span| make_span(file, &span, source)),
        message: err.neb_message(),
    }
}

fn source_context(
    report: &Report,
    fallback_source: Option<&str>,
) -> (Option<String>, Option<String>) {
    if let Some(source_code) = report.source_code() {
        let span = MietteSourceSpan::from(0..1);
        if let Ok(contents) = source_code.read_span(&span, 0, 10_000) {
            let file = contents.name().map(str::to_string);
            let source = std::str::from_utf8(contents.data())
                .ok()
                .map(str::to_string)
                .or_else(|| fallback_source.map(str::to_string))
                .or_else(|| {
                    file.as_ref()
                        .and_then(|path| std::fs::read_to_string(path).ok())
                });
            return (file, source);
        }
    }

    (None, fallback_source.map(str::to_string))
}

fn collect_miette_diagnostic(
    err: &dyn Diagnostic,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) {
    if let Some(related) = err.related() {
        let related: Vec<_> = related.collect();
        if !related.is_empty() {
            for child in related {
                collect_miette_diagnostic(child, source, file, out);
            }
            return;
        }
    }

    if let Some(source_err) = err.diagnostic_source() {
        collect_miette_diagnostic(source_err, source, file, out);
        return;
    }

    push_miette_leaf(err, source, file, out);
}

/// Fallback when only a miette [`Report`] is available. Typed [`NebError`] APIs should
/// be preferred for JSON export.
fn push_miette_leaf(
    err: &dyn Diagnostic,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) {
    let code = err
        .code()
        .and_then(|code| neb_code_from_miette(&code.to_string()))
        .unwrap_or("NEB-E001")
        .to_string();
    out.push(DiagnosticJson {
        code,
        span: span_from_labels(err, source, file),
        message: err.to_string(),
    });
}

fn push_fallback(report: &Report, out: &mut Vec<DiagnosticJson>) {
    out.push(DiagnosticJson {
        code: "NEB-E001".to_string(),
        span: None,
        message: report.to_string(),
    });
}

fn span_from_labels(
    err: &dyn Diagnostic,
    source: Option<&str>,
    file: Option<&str>,
) -> Option<DiagnosticSpan> {
    let labels = err.labels()?;
    let label = labels.into_iter().next()?;
    Some(span_from_labeled(&label, source, file))
}

fn span_from_labeled(
    label: &LabeledSpan,
    source: Option<&str>,
    file: Option<&str>,
) -> DiagnosticSpan {
    let span = label.offset()..label.len() + label.offset();
    make_span(file, &span, source)
}

fn make_span(file: Option<&str>, span: &nebula_ast::Span, source: Option<&str>) -> DiagnosticSpan {
    let (line, column) = source
        .map(|text| line_column(text, span.start))
        .unwrap_or((None, None));
    DiagnosticSpan {
        file: file.map(str::to_string),
        start: span.start,
        end: span.end,
        line,
        column,
    }
}

fn line_column(source: &str, offset: usize) -> (Option<usize>, Option<usize>) {
    let clamped = offset.min(source.len());
    let mut line = 1usize;
    let mut column = 1usize;
    for ch in source.chars().take(clamped) {
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (Some(line), Some(column))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use nebula_syntax::parse;
    use nebula_types::{report_with_source, typecheck};

    use super::*;

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
}