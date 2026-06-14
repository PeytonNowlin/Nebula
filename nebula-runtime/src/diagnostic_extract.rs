use std::error::Error as StdError;

use nebula_ast::{DiagnosticExtractor, DiagnosticJson, NebError};

use crate::RuntimeError;

fn extract_runtime_error(
    cause: &(dyn StdError + 'static),
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) -> bool {
    if let Some(err) = cause.downcast_ref::<RuntimeError>() {
        out.push(err.to_diagnostic_json(file, source));
        return true;
    }
    false
}

inventory::submit! {
    DiagnosticExtractor(extract_runtime_error)
}
