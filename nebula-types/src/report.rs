use std::path::Path;

use miette::{Diagnostic, NamedSource, Report};
use thiserror::Error;

use crate::TypeError;

/// Multiple type errors reported together, preserving per-error miette metadata.
#[derive(Debug, Error, Diagnostic)]
#[error("found {error_count} type error(s)")]
#[diagnostic(code(nebula::typecheck_failed))]
pub struct TypecheckErrors {
    error_count: usize,
    #[related]
    errors: Vec<TypeError>,
}

impl TypecheckErrors {
    pub fn new(errors: Vec<TypeError>) -> Self {
        Self {
            error_count: errors.len(),
            errors,
        }
    }

    pub fn errors(&self) -> &[TypeError] {
        &self.errors
    }

    pub fn iter(&self) -> std::slice::Iter<'_, TypeError> {
        self.errors.iter()
    }
}

/// Attach source text so miette can render spans and labels.
pub fn report_with_source<E>(path: impl AsRef<Path>, source: &str, error: E) -> Report
where
    E: Diagnostic + std::error::Error + Send + Sync + 'static,
{
    Report::new(error).with_source_code(NamedSource::new(
        path.as_ref().display().to_string(),
        source.to_string(),
    ))
}