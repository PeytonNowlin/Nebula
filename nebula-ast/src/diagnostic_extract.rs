use std::error::Error as StdError;

use crate::DiagnosticJson;

pub type ExtractFn = fn(
    cause: &(dyn StdError + 'static),
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) -> bool;

/// Link-time registry entry for converting a concrete error type into JSON diagnostics.
pub struct DiagnosticExtractor(pub ExtractFn);

inventory::collect!(DiagnosticExtractor);

/// Walk registered extractors until one recognizes `cause`.
pub fn try_extract_from_cause(
    cause: &(dyn StdError + 'static),
    source: Option<&str>,
    file: Option<&str>,
    out: &mut Vec<DiagnosticJson>,
) -> bool {
    for extractor in inventory::iter::<DiagnosticExtractor> {
        if (extractor.0)(cause, source, file, out) {
            return true;
        }
    }
    false
}