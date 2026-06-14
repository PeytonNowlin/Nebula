use std::error::Error as StdError;

use nebula_ast::{DiagnosticExtractor, DiagnosticJson, NebError};

use crate::{LexError, ParseError};

fn extract_syntax_error(
    cause: &(dyn StdError + 'static),
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) -> bool {
    if let Some(err) = cause.downcast_ref::<LexError>() {
        out.push(err.to_diagnostic_json(file, source));
        return true;
    }
    if let Some(err) = cause.downcast_ref::<ParseError>() {
        out.push(err.to_diagnostic_json(file, source));
        return true;
    }
    false
}

inventory::submit! {
    DiagnosticExtractor(extract_syntax_error)
}
