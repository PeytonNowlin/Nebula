use miette::{Diagnostic, LabeledSpan, Report, SourceSpan as MietteSourceSpan};
use nebula_ast::{make_span, neb_code_from_miette, try_extract_from_cause};

pub use nebula_ast::{DiagnosticJson, DiagnosticSpan};

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

fn try_extract_structured(
    report: &Report,
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) -> bool {
    for cause in report.chain() {
        if try_extract_from_cause(cause, source, file, out) {
            return true;
        }
    }
    false
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