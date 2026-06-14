use serde::Serialize;

use crate::Span;

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

pub fn make_span(file: Option<&str>, span: &Span, source: Option<&str>) -> DiagnosticSpan {
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