use std::error::Error as StdError;

use nebula_ast::{DiagnosticExtractor, DiagnosticJson, NebError};

use crate::{TypeError, TypecheckErrors};

fn extract_type_error(
    cause: &(dyn StdError + 'static),
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) -> bool {
    if let Some(errors) = cause.downcast_ref::<TypecheckErrors>() {
        for err in errors.errors() {
            out.push(err.to_diagnostic_json(file, source));
        }
        return true;
    }
    if let Some(err) = cause.downcast_ref::<TypeError>() {
        out.push(err.to_diagnostic_json(file, source));
        return true;
    }
    false
}

inventory::submit! {
    DiagnosticExtractor(extract_type_error)
}
