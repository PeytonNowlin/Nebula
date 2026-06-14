use std::error::Error as StdError;

use nebula_ast::{DiagnosticExtractor, DiagnosticJson};

use crate::LoadError;

fn extract_load_error(
    cause: &(dyn StdError + 'static),
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) -> bool {
    if let Some(err) = cause.downcast_ref::<LoadError>() {
        out.extend(err.to_diagnostic_jsons(source, file));
        return true;
    }
    false
}

inventory::submit! {
    DiagnosticExtractor(extract_load_error)
}