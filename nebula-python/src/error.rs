use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum EmitError {
    #[error("NEB-PY001 [emit_error] {message}")]
    #[diagnostic(code(nebula::emit_error))]
    Error { message: String },

    #[error("NEB-PY002 [emit_error] failed to copy runtime shim: {message}")]
    #[diagnostic(code(nebula::emit_runtime_copy))]
    RuntimeCopy { message: String },
}
