use std::path::Path;

use miette::{Diagnostic, LabeledSpan, Report, SourceSpan as MietteSourceSpan};
use nebula_ast::Span;
use nebula_ir::IrError;
use nebula_load::LoadError;
use nebula_runtime::RuntimeError;
use nebula_syntax::ParseError;
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

    if let Some(errors) = report.downcast_ref::<TypecheckErrors>() {
        for type_error in errors.errors() {
            push_type_error(type_error, source, file, &mut out);
        }
        return out;
    }
    if let Some(parse_err) = report.downcast_ref::<ParseError>() {
        push_parse_error(parse_err, source, file, &mut out);
        return out;
    }
    if let Some(load_err) = report.downcast_ref::<LoadError>() {
        push_load_error(load_err, source, file, &mut out);
        return out;
    }
    if let Some(type_err) = report.downcast_ref::<TypeError>() {
        push_type_error(type_err, source, file, &mut out);
        return out;
    }
    if let Some(runtime_err) = report.downcast_ref::<RuntimeError>() {
        push_runtime_error(runtime_err, &mut out);
        return out;
    }
    if let Some(ir_err) = report.downcast_ref::<IrError>() {
        push_ir_error(ir_err, &mut out);
        return out;
    }

    collect_diagnostic(report.as_ref(), source, file, &mut out);
    if out.is_empty() {
        push_fallback(report, &mut out);
    }
    out
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

    (
        None,
        fallback_source.map(str::to_string),
    )
}

pub fn emit_json_diagnostics(diagnostics: &[DiagnosticJson]) {
    let json = serde_json::to_string(diagnostics).expect("serialize diagnostics");
    eprintln!("{json}");
}

fn collect_diagnostic(
    err: &dyn Diagnostic,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) {
    if let Some(related) = err.related() {
        let related: Vec<_> = related.collect();
        if !related.is_empty() {
            for child in related {
                collect_diagnostic(child, source, file, out);
            }
            return;
        }
    }

    if let Some(source_err) = err.diagnostic_source() {
        collect_diagnostic(source_err, source, file, out);
        return;
    }

    push_from_message(err.to_string(), span_from_labels(err, source, file), out);
}

fn push_fallback(report: &Report, out: &mut Vec<DiagnosticJson>) {
    let message = report.to_string();
    let code = extract_neb_code(&message).unwrap_or_else(|| "NEB-E001".to_string());
    out.push(DiagnosticJson {
        code,
        span: None,
        message,
    });
}

fn push_type_error(
    err: &TypeError,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) {
    let (code, span, message) = match err {
        TypeError::UndefinedIdent { name, span } => (
            "NEB-T001",
            span,
            format!("undefined identifier `{name}`"),
        ),
        TypeError::Mismatch {
            expected,
            found,
            span,
        } => (
            "NEB-T002",
            span,
            format!("type mismatch: expected {expected}, found {found}"),
        ),
        TypeError::ImmutableAssign { name, span } => (
            "NEB-T003",
            span,
            format!("cannot assign to immutable binding `{name}`"),
        ),
        TypeError::UndefinedFn { name, span } => (
            "NEB-T004",
            span,
            format!("undefined function `{name}`"),
        ),
        TypeError::UndefinedStruct { name, span } => (
            "NEB-T005",
            span,
            format!("undefined struct `{name}`"),
        ),
        TypeError::UndefinedProbe { name, span } => (
            "NEB-T006",
            span,
            format!("undefined probe `{name}`"),
        ),
        TypeError::MissingMain { span } => (
            "NEB-T007",
            span,
            "missing mission entry point `main`".to_string(),
        ),
        TypeError::UnknownField {
            struct_name,
            field,
            span,
        } => (
            "NEB-T008",
            span,
            format!("unknown field `{field}` on struct `{struct_name}`"),
        ),
        TypeError::DuplicateSymbol { kind, name, span } => (
            "NEB-T009",
            span,
            format!("duplicate {kind} `{name}`"),
        ),
    };

    out.push(DiagnosticJson {
        code: code.to_string(),
        span: Some(make_span(file, span, source)),
        message,
    });
}

fn push_parse_error(
    err: &ParseError,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) {
    match err {
        ParseError::Lex(lex) => {
            push_with_span(
                err.to_string(),
                extract_neb_code(&err.to_string()),
                &lex.span,
                source,
                file,
                out,
            );
        }
        ParseError::Unexpected { span, .. }
        | ParseError::Eof { span }
        | ParseError::DeprecatedComparison { span, .. }
        | ParseError::DeprecatedBraceBlock { span, .. } => {
            push_with_span(
                err.to_string(),
                extract_neb_code(&err.to_string()),
                span,
                source,
                file,
                out,
            );
        }
    }
}

fn push_load_error(
    err: &LoadError,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) {
    match err {
        LoadError::Parse {
            path,
            source: parse_err,
            ..
        } => {
            let imported_source = std::fs::read_to_string(path).ok();
            let imported_file = Some(path.display().to_string());
            push_parse_error(
                parse_err,
                imported_source.as_deref(),
                imported_file.as_deref(),
                out,
            );
        }
        LoadError::NotFound { path, span }
        | LoadError::Circular { path, span }
        | LoadError::LibraryHasMission { path, span } => {
            push_with_span(
                err.to_string(),
                extract_neb_code(&err.to_string()),
                span,
                source,
                file,
                out,
            );
            let _ = path;
        }
        LoadError::Duplicate {
            existing,
            new,
            span,
            ..
        } => {
            push_with_span(
                err.to_string(),
                extract_neb_code(&err.to_string()),
                span,
                source,
                file,
                out,
            );
            let _ = (existing, new);
        }
        LoadError::Read { span, .. } => {
            push_with_span(
                err.to_string(),
                extract_neb_code(&err.to_string()),
                span,
                source,
                file,
                out,
            );
        }
    }
}

fn push_runtime_error(err: &RuntimeError, out: &mut Vec<DiagnosticJson>) {
    push_from_message(err.to_string(), None, out);
}

fn push_ir_error(err: &IrError, out: &mut Vec<DiagnosticJson>) {
    push_from_message(err.to_string(), None, out);
}

fn push_with_span(
    full_message: String,
    code: Option<String>,
    span: &Span,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) {
    let code = code.unwrap_or_else(|| "NEB-E001".to_string());
    out.push(DiagnosticJson {
        code,
        span: Some(make_span(file, span, source)),
        message: human_message(&full_message),
    });
}

fn push_from_message(message: String, span: Option<DiagnosticSpan>, out: &mut Vec<DiagnosticJson>) {
    let code = extract_neb_code(&message).unwrap_or_else(|| "NEB-E001".to_string());
    out.push(DiagnosticJson {
        code,
        span,
        message: human_message(&message),
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

fn make_span(file: Option<&str>, span: &Span, source: Option<&str>) -> DiagnosticSpan {
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

fn extract_neb_code(message: &str) -> Option<String> {
    let rest = message.strip_prefix("NEB-")?;
    let len = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric())
        .count();
    if len == 0 {
        return None;
    }
    Some(format!("NEB-{}", &rest[..len]))
}

fn human_message(message: &str) -> String {
    message
        .split_once("] ")
        .map(|(_, body)| body.to_string())
        .unwrap_or_else(|| message.to_string())
}

pub fn diagnostics_from_type_errors(
    path: impl AsRef<Path>,
    source: &str,
    errors: &TypecheckErrors,
) -> Vec<DiagnosticJson> {
    let file = path.as_ref().display().to_string();
    let mut out = Vec::new();
    for err in errors.errors() {
        push_type_error(err, Some(source), Some(&file), &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use nebula_syntax::parse;
    use nebula_types::{report_with_source, typecheck};

    use super::*;

    #[test]
    fn type_mismatch_json_includes_code_span_and_message() {
        let src = r#"
mission main {
  let x: Int = "not an int";
}
"#;
        let program = parse(src).expect("parse");
        let errors = typecheck(&program).expect_err("typecheck");
        let report = report_with_source(Path::new("example.neb"), src, errors);
        let diags = diagnostics_from_report_with_source(&report, Some(src));

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "NEB-T002");
        assert!(diags[0].message.contains("type mismatch"));
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
        let report = report_with_source(Path::new("example.neb"), src, errors);
        let diags = diagnostics_from_report_with_source(&report, Some(src));

        assert_eq!(diags.len(), 2);
        assert!(diags.iter().any(|d| d.code == "NEB-T004"));
        assert!(diags.iter().any(|d| d.code == "NEB-T002"));
    }

    #[test]
    fn json_roundtrip_is_valid_array() {
        let src = "mission main { let x: Int = \"nope\"; }";
        let program = parse(src).expect("parse");
        let errors = typecheck(&program).expect_err("typecheck");
        let report = report_with_source(Path::new("bad.neb"), src, errors);
        let diags = diagnostics_from_report_with_source(&report, Some(src));
        let json = serde_json::to_string(&diags).expect("json");
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).expect("parse json");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["code"], "NEB-T002");
        assert!(parsed[0]["span"].is_object());
        assert!(parsed[0]["message"].is_string());
    }
}